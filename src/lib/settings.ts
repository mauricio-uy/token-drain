import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";

/** Mirrors `RailSide` in `src-tauri/src/window/placement.rs`. */
export type RailSide = "right" | "left";

/** Mirrors `Settings` in `src-tauri/src/settings.rs`. */
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

export async function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
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

  useEffect(() => {
    let cancelled = false;

    void Promise.all([getSettings(), listProviders(), getLaunchAtLogin()]).then(
      ([loaded, known, launch]) => {
        if (cancelled) return;
        setSettings(loaded);
        setProviders(known);
        setLaunch(launch);
      },
    );

    return () => {
      cancelled = true;
    };
  }, []);

  const updateLaunchAtLogin = useCallback((enabled: boolean) => {
    setLaunch(enabled);

    void setLaunchAtLogin(enabled)
      .then((actual) => {
        setError(null);
        // Corrected from what the OS reports afterwards, so a write that
        // silently failed snaps the checkbox back rather than lying about it.
        setLaunch(actual);
      })
      .catch((cause: unknown) => {
        setError(String(cause));
        void getLaunchAtLogin().then(setLaunch);
      });
  }, []);

  const update = useCallback(
    (change: Partial<Settings>) => {
      setSettings((current) => {
        if (!current) return current;

        const next = { ...current, ...change };

        void saveSettings(next)
          .then((stored) => {
            setError(null);
            setSettings(stored);
          })
          .catch((cause: unknown) => {
            // Say so rather than silently reverting: the setting is live in
            // this session either way, and a control that snaps back with no
            // explanation is worse than one that admits it did not persist.
            setError(String(cause));
          });

        return next;
      });
    },
    [],
  );

  return { settings, providers, launchAtLogin, error, update, updateLaunchAtLogin };
}
