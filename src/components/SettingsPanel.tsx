import "./SettingsPanel.css";
import { useId } from "react";
import { ProviderLogo } from "./ProviderLogo";
import { useSettings, type RailSide, type Settings } from "../lib/settings";

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

export function SettingsPanel() {
  const { settings, providers, launchAtLogin, error, update, updateLaunchAtLogin } =
    useSettings();

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

      <Section title="Appearance" hint="Where the rail sits on screen.">
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
          <div className="settings-slider">
            <input
              type="range"
              aria-label="Vertical position"
              aria-valuetext={describeOffset(settings.verticalOffset)}
              min={-MAX_VERTICAL_OFFSET}
              max={MAX_VERTICAL_OFFSET}
              step={10}
              value={settings.verticalOffset}
              onChange={(event) => update({ verticalOffset: Number(event.target.value) })}
            />
            <button
              type="button"
              className="settings-reset"
              onClick={() => update({ verticalOffset: 0 })}
              disabled={settings.verticalOffset === 0}
            >
              Centre
            </button>
          </div>
          <p className="settings-hint">{describeOffset(settings.verticalOffset)}</p>
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
