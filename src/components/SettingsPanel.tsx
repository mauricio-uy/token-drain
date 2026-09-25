import "./SettingsPanel.css";
import { useEffect, useId, useState } from "react";
import { ProviderLogo } from "./ProviderLogo";
import { UpdateStatus } from "./UpdateStatus";
import { RepositoryLink } from "./RepositoryLink";
import {
  applyTheme,
  getVerticalOffsetRange,
  useSettings,
  type RailSide,
  type Settings,
  type Theme,
} from "../lib/settings";
import { getAppVersion, openLogDirectory } from "../lib/diagnostics";

/**
 * Poll intervals worth offering.
 *
 * A fixed list rather than a free number field: the backend clamps anything out
 * of range anyway, and a control that silently rewrites what you typed is worse
 * than one that never let you type it. The floor exists to keep the app from
 * hammering the providers, so it is not presented as a choice.
 */
const INTERVALS: { seconds: number; label: string }[] = [
  { seconds: 60, label: "1 minute" },
  { seconds: 120, label: "2 minutes" },
  { seconds: 300, label: "5 minutes" },
  { seconds: 600, label: "10 minutes" },
  { seconds: 900, label: "15 minutes" },
  { seconds: 1800, label: "30 minutes" },
  { seconds: 3600, label: "1 hour" },
];

/** Mirrors `MAX_VERTICAL_OFFSET` in `src-tauri/src/settings.rs`. */
const MAX_VERTICAL_OFFSET = 400;

/** Mirror `MIN_UI_SCALE` and `MAX_UI_SCALE` in `src-tauri/src/settings.rs`. */
const MIN_UI_SCALE = 70;
const MAX_UI_SCALE = 100;

/**
 * Marks offered as chips.
 *
 * Any percentage is storable, but a free numeric editor for a set of integers is
 * a lot of interface for a decision with three sensible answers. The backend
 * accepts whatever a hand-edited file contains; this offers the useful ones.
 */
const OFFERED_THRESHOLDS = [50, 80, 90, 95];

const PROVIDER_NAMES: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
  "opencode-go": "OpenCode Go",
};

/** Title case as a fallback, so a provider added later still reads properly. */
function nameFor(provider: string): string {
  return PROVIDER_NAMES[provider] ?? provider.charAt(0).toUpperCase() + provider.slice(1);
}

/**
 * One collapsible section.
 *
 * Every section starts closed, with no way to ask for otherwise. A settings
 * window is opened to change one thing, and a screen that opens as a wall of
 * controls makes the user scroll past everything they did not come for. Closed,
 * the window is a table of contents: the titles say what is configurable, and
 * the user opens the one row they came for.
 */
function Section({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: string;
  children: React.ReactNode;
}) {
  const titleId = useId();
  return (
    <details className="settings-section">
      <summary className="settings-summary">
        <h2 id={titleId} className="settings-title">{title}</h2>
      </summary>
      <div className="settings-section-body" role="group" aria-labelledby={titleId}>
        {hint && <p className="settings-hint">{hint}</p>}
        {children}
      </div>
    </details>
  );
}

/**
 * A named control inside a section.
 *
 * Sections used to hold one control each, so the section title named it. A
 * category holds several, and a control with no name of its own would be left
 * to be identified by its position under the title.
 */
function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="settings-field">
      <h3 className="settings-field-title">{label}</h3>
      {children}
    </div>
  );
}

/**
 * A slider that saves when released rather than on every step of a drag.
 *
 * Each save re-docks or re-lays out the rail, and a stream of them mid-drag
 * makes the slider stutter. The value on screen follows the pointer the whole
 * time; only the save waits.
 */
function ReleasedSlider({
  label,
  min,
  max,
  step,
  value,
  describe,
  reset,
  onCommit,
}: {
  label: string;
  min: number;
  max: number;
  step: number;
  value: number;
  describe: (value: number) => string;
  /** The button that returns the slider to its default. */
  reset: { value: number; label: string };
  onCommit: (value: number) => void;
}) {
  const [draft, setDraft] = useState<number | null>(null);

  // A stored value beyond the range sits at the end it overshoots.
  const shown = Math.min(max, Math.max(min, draft ?? value));

  const commit = () => {
    if (draft === null) return;
    setDraft(null);
    if (draft !== value) onCommit(draft);
  };

  return (
    <>
      <div className="settings-slider">
        <input
          type="range"
          aria-label={label}
          aria-valuetext={describe(shown)}
          min={min}
          max={max}
          step={step}
          value={shown}
          onChange={(event) => setDraft(Number(event.target.value))}
          onPointerUp={commit}
          onKeyUp={commit}
          onBlur={commit}
        />
        <button
          type="button"
          className="settings-reset"
          onClick={() => {
            setDraft(null);
            onCommit(reset.value);
          }}
          disabled={shown === reset.value}
        >
          {reset.label}
        </button>
      </div>
      <p className="settings-hint">{describe(shown)}</p>
    </>
  );
}

/**
 * The vertical position slider.
 *
 * Its ends are the limits of travel on the rail's current monitor, so every
 * step moves the rail and both ends reach an edge.
 */
function VerticalPosition({
  side,
  offset,
  onCommit,
}: {
  side: RailSide;
  offset: number;
  onCommit: (offset: number) => void;
}) {
  const [range, setRange] = useState<[number, number]>([
    -MAX_VERTICAL_OFFSET,
    MAX_VERTICAL_OFFSET,
  ]);

  useEffect(() => {
    let cancelled = false;
    void getVerticalOffsetRange(side).then(
      (next) => {
        if (!cancelled) setRange(next);
      },
      // Keep the fallback range: the backend clamps whatever is chosen anyway.
      () => {},
    );
    return () => {
      cancelled = true;
    };
  }, [side]);

  return (
    <ReleasedSlider
      label="Vertical position"
      min={range[0]}
      max={range[1]}
      step={1}
      value={offset}
      describe={describeOffset}
      reset={{ value: 0, label: "Centre" }}
      onCommit={onCommit}
    />
  );
}

/** Render live settings controls that persist each accepted edit immediately. */
export function SettingsPanel() {
  const { settings, providers, launchAtLogin, error, update, updateLaunchAtLogin } =
    useSettings();
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null);

  useEffect(() => {
    void getAppVersion().then(setAppVersion, () => setDiagnosticsError("Could not read app version."));
  }, []);

  // This window follows the theme it is editing, so a change is seen at once.
  const theme = settings?.theme;
  useEffect(() => {
    if (theme) applyTheme(theme);
  }, [theme]);

  // Nothing is rendered until the real values arrive. Showing defaults first
  // would flash a configuration the user does not have, and any control touched
  // in that moment would save the wrong thing.
  if (!settings) {
    return (
      <div className="settings settings--loading">
        {error ? <p className="settings-error" role="alert">{error}</p> : "Loading…"}
      </div>
    );
  }

  const disabled = new Set(settings.disabledProviders);
  const everythingOff = providers.length > 0 && providers.every((id) => disabled.has(id));

  const toggleProvider = (provider: string, enabled: boolean) => {
    const next = new Set(disabled);
    if (enabled) next.delete(provider);
    else next.add(provider);
    update({ disabledProviders: [...next] });
  };

  return (
    <div className="settings">
      {error && (
        <p className="settings-error" role="alert">
          {error}
        </p>
      )}

      <Section
        title="Providers"
        hint="Switching one off stops the requests as well as hiding the badge."
      >
        <ul className="settings-providers">
          {providers.map((provider) => (
            <li key={provider}>
              <label className="settings-check">
                <input
                  type="checkbox"
                  checked={!disabled.has(provider)}
                  onChange={(event) => toggleProvider(provider, event.target.checked)}
                />
                {/* The label already names the provider; without this the
                    mark's own label doubles it, and the checkbox is announced
                    as "Claude Claude". */}
                <span className="settings-provider-mark" aria-hidden="true">
                  <ProviderLogo provider={provider} size={16} />
                </span>
                {nameFor(provider)}
              </label>
            </li>
          ))}
        </ul>
        {everythingOff && (
          <p className="settings-warning">
            With every provider off the rail is empty. It stays reachable from the tray.
          </p>
        )}
      </Section>

      <Section
        title="Check every"
        hint="How often each provider is asked for fresh figures."
      >
        <select
          className="settings-select"
          aria-label="Check every"
          value={nearestInterval(settings.pollIntervalSeconds)}
          onChange={(event) =>
            update({ pollIntervalSeconds: Number(event.target.value) })
          }
        >
          {INTERVALS.map((interval) => (
            <option key={interval.seconds} value={interval.seconds}>
              {interval.label}
            </option>
          ))}
        </select>
      </Section>

      <Section title="Appearance" hint="How the app looks, where the rail sits, and how big it is.">
        <Field label="Theme">
          <div className="settings-choices" role="radiogroup" aria-label="Theme">
            {(["dark", "light"] as Theme[]).map((theme) => (
              <label key={theme} className="settings-check">
                <input
                  type="radio"
                  name="theme"
                  checked={settings.theme === theme}
                  onChange={() => update({ theme })}
                />
                {theme === "dark" ? "Dark" : "Light"}
              </label>
            ))}
          </div>
        </Field>

        <Field label="Side">
          <div className="settings-choices" role="radiogroup" aria-label="Side">
            {(["left", "right"] as RailSide[]).map((side) => (
              <label key={side} className="settings-check">
                <input
                  type="radio"
                  name="rail-side"
                  checked={settings.railSide === side}
                  onChange={() => update({ railSide: side })}
                />
                {side === "right" ? "Right edge" : "Left edge"}
              </label>
            ))}
          </div>
        </Field>

        <Field label="Vertical position">
          <VerticalPosition
            side={settings.railSide}
            offset={settings.verticalOffset}
            onCommit={(verticalOffset) => update({ verticalOffset })}
          />
        </Field>

        <Field label="Size">
          <ReleasedSlider
            label="Size"
            min={MIN_UI_SCALE}
            max={MAX_UI_SCALE}
            step={5}
            value={settings.uiScale}
            describe={(scale) =>
              scale === MAX_UI_SCALE ? "Full size" : `${scale}% of full size. Text keeps its size.`}
            reset={{ value: MAX_UI_SCALE, label: "Full size" }}
            onCommit={(uiScale) => update({ uiScale })}
          />
        </Field>
      </Section>

      <Section
        title="Notifications"
        hint="A toast the first time a window passes each mark, once per quota period."
      >
        <label className="settings-check">
          <input
            type="checkbox"
            checked={settings.notificationsEnabled}
            onChange={(event) => update({ notificationsEnabled: event.target.checked })}
          />
          Notify me
        </label>

        <div className="settings-chips" aria-label="Alert thresholds">
          {OFFERED_THRESHOLDS.map((threshold) => {
            const on = settings.notificationThresholds.includes(threshold);
            return (
              <button
                key={threshold}
                type="button"
                className={`settings-chip ${on ? "is-on" : ""}`}
                aria-pressed={on}
                disabled={!settings.notificationsEnabled}
                onClick={() =>
                  update({
                    notificationThresholds: on
                      ? settings.notificationThresholds.filter((value) => value !== threshold)
                      : [...settings.notificationThresholds, threshold],
                  })
                }
              >
                {threshold}%
              </button>
            );
          })}
        </div>
      </Section>

      <Section title="Startup" hint="Whether the rail is up before you look for it.">
        <label className="settings-check">
          <input
            type="checkbox"
            checked={launchAtLogin}
            onChange={(event) => updateLaunchAtLogin(event.target.checked)}
          />
          Launch at login
        </label>
      </Section>

      <Section
        title="Updates"
        hint="Check for updates now, or enable automatic updates at startup and every six hours."
      >
        <label className="settings-check">
          <input
            type="checkbox"
            checked={settings.automaticUpdatesEnabled}
            onChange={(event) => update({ automaticUpdatesEnabled: event.target.checked })}
          />
          Download and install updates automatically
        </label>
        <p className="settings-hint settings-hint--after-control">
          Updates are accepted only when their signature matches Token Drain's
          release key. Windows closes and restarts the app while installing.
        </p>
        <UpdateStatus />
      </Section>
      <Section title="Diagnostics" hint="Information to include when reporting a problem.">
        <p className="settings-hint">Version {appVersion ?? "Loading…"}</p>
        <button
          type="button"
          className="settings-action"
          onClick={() => {
            setDiagnosticsError(null);
            void openLogDirectory().catch(() => setDiagnosticsError("Could not open the log directory."));
          }}
        >
          Open log directory
        </button>
        {diagnosticsError && <p className="settings-error" role="alert">{diagnosticsError}</p>}
      </Section>
      <RepositoryLink />
    </div>
  );
}

/**
 * The listed interval closest to what is stored.
 *
 * A settings file edited by hand can hold a value no option matches. Snapping
 * the select to the nearest one keeps the control from showing an empty box,
 * without rewriting the stored value behind the user's back.
 */
function nearestInterval(seconds: Settings["pollIntervalSeconds"]): number {
  return INTERVALS.reduce((best, interval) =>
    Math.abs(interval.seconds - seconds) < Math.abs(best.seconds - seconds) ? interval : best,
  ).seconds;
}

function describeOffset(offset: number): string {
  if (offset === 0) return "Centred";
  return offset > 0 ? `${offset} px below centre` : `${-offset} px above centre`;
}
