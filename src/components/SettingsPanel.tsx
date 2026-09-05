import "./SettingsPanel.css";
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

const PROVIDER_NAMES: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
};

/** Title case as a fallback, so a provider added later still reads properly. */
function nameFor(provider: string): string {
  return PROVIDER_NAMES[provider] ?? provider.charAt(0).toUpperCase() + provider.slice(1);
}

function Section({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="settings-section">
      <h2 className="settings-title">{title}</h2>
      {hint && <p className="settings-hint">{hint}</p>}
      {children}
    </section>
  );
}

export function SettingsPanel() {
  const { settings, providers, error, update } = useSettings();

  // Nothing is rendered until the real values arrive. Showing defaults first
  // would flash a configuration the user does not have, and any control touched
  // in that moment would save the wrong thing.
  if (!settings) {
    return <div className="settings settings--loading">Loading…</div>;
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
          Could not save: {error}
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

      <Section title="Check every">
        <select
          className="settings-select"
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

      <Section title="Side">
        <div className="settings-choices">
          {(["right", "left"] as RailSide[]).map((side) => (
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
      </Section>

      <Section title="Vertical position">
        <div className="settings-slider">
          <input
            type="range"
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
