import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { latestWrite } from "./latestWrite";
import { liveSnapshot } from "./liveSnapshot";

/** Mirrors `RailSide` in `src-tauri/src/window/placement.rs`. */
export type RailSide = "right" | "left";

/** Persisted preferences mirrored from the Rust settings contract. */
export type Settings = {
  pollIntervalSeconds: number;
  /**
   * Providers the user has switched off. Stored as the disabled set, not the
   * enabled one, so a provider added in a later version arrives switched on.
   */
  disabledProviders: string[];
  railSide: RailSide;
  /** Nudge from vertical centre, in logical pixels. Positive moves down. */
  verticalOffset: number;
  notificationsEnabled: boolean;
  /** Percentages worth interrupting at. Sorted and deduplicated by the backend. */
  notificationThresholds: number[];
};

/** Read the normalized settings currently in force. */
export async function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

/** Open or focus the existing settings window. */
export async function openSettingsWindow(): Promise<void> {
  await invoke("open_settings");
}

/** Follow docking preferences in the rail without loading the settings form. */
export function useRailSide(): RailSide {
  const [side, setSide] = useState<RailSide>("right");
  useEffect(() => liveSnapshot(
    (receive) => listen<Settings>("settings-updated", (event) => receive(event.payload)),
    getSettings,
    (settings) => setSide(settings.railSide),
  ), []);
  return side;
}

/**
 * Whether the app is registered to launch at login.
 *
 * Deliberately not part of `Settings`: that is the operating system's state, a
 * registry entry the user can change without going near this app, so it is read
 * from there every time rather than mirrored into our own file where the two
 * could drift apart.
 */
export async function getLaunchAtLogin(): Promise<boolean> {
  return invoke<boolean>("get_launch_at_login");
}

/** Returns the state read back from the OS afterwards, not the state asked for. */
export async function setLaunchAtLogin(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_launch_at_login", { enabled });
}

/** List provider identifiers in the backend's display order. */
export async function listProviders(): Promise<string[]> {
  return invoke<string[]>("list_providers");
}

/**
 * Store new settings and get back what was actually kept.
 *
 * The backend clamps out-of-range values, so the return is the configuration
 * really in force — which is what the screen should show.
 */
export async function saveSettings(value: Settings): Promise<Settings> {
  return invoke<Settings>("set_settings", { value });
}

type SettingsForm = {
  settings: Settings | null;
  providers: string[];
  launchAtLogin: boolean;
  /** The last save that failed, if any. */
  error: string | null;
  update: (change: Partial<Settings>) => void;
  updateLaunchAtLogin: (enabled: boolean) => void;
};

/**
 * The settings, loaded once and saved on every change.
 *
 * There is no Save button: each control writes immediately and the rail reacts
 * at once, so the window is a set of live controls rather than a form with a
 * pending state that can disagree with what is on screen.
 *
 * The state is set optimistically so the control does not lag the pointer, then
 * corrected from what the backend actually stored. The two differ only when a
 * value was clamped, and then the corrected one is the truth.
 */
export function useSettings(): SettingsForm {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [providers, setProviders] = useState<string[]>([]);
  const [launchAtLogin, setLaunch] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const current = useRef<Settings | null>(null);
  const [queueSave] = useState(() => latestWrite(saveSettings, (stored) => {
    current.current = stored;
    setSettings(stored);
    setError(null);
  }, (cause) => setError(`Could not save settings: ${String(cause)}`)));
  const [queueLaunch] = useState(() => latestWrite(
    setLaunchAtLogin,
    (actual) => {
      setLaunch(actual);
      setError(null);
    },
    (cause) => setError(`Could not change launch at login: ${String(cause)}`),
  ));

  useEffect(() => {
    let cancelled = false;

    void Promise.all([getSettings(), listProviders(), getLaunchAtLogin()]).then(
      ([loaded, known, launch]) => {
        if (cancelled) return;
        current.current = loaded;
        setSettings(loaded);
        setProviders(known);
        setLaunch(launch);
      },
      (cause: unknown) => {
        if (!cancelled) setError(`Could not load settings: ${String(cause)}`);
      },
    );

    return () => {
      cancelled = true;
    };
  }, []);

  const updateLaunchAtLogin = useCallback((enabled: boolean) => {
    setLaunch(enabled);

    queueLaunch(enabled);
  }, [queueLaunch]);

  const update = useCallback(
    (change: Partial<Settings>) => {
      if (!current.current) return;
      const next = { ...current.current, ...change };
      current.current = next;
      setSettings(next);
      // Side effects belong in the event handler, never in a React updater:
      // Strict Mode may replay updaters to check that they are pure.
      queueSave(next);
    },
    [queueSave],
  );

  return { settings, providers, launchAtLogin, error, update, updateLaunchAtLogin };
}
