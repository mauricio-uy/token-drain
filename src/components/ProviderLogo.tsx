import "./ProviderLogo.css";
import claudeMark from "../assets/logos/claude.svg?raw";
import codexMark from "../assets/logos/codex.svg?raw";
import opencodeMark from "../assets/logos/opencode.svg?raw";

/**
 * Provider marks identify the subscription reported by each badge.
 * The artwork stays in individual assets so the component has fixed, audited
 * markup sources. OpenCode's MIT license is retained alongside its SVG.
 */

type LogoProps = {
  /** Edge length: a number of CSS pixels, or any CSS length. */
  size?: number | string;
};

/**
 * Renders one of the imported files.
 *
 * The markup is inlined at build time from a fixed path — no value that reaches
 * this component ever becomes markup — and inlining rather than using an `<img>`
 * is what lets `currentColor` and the dimming rules in `ProviderBadge.css` reach
 * inside the glyph.
 */
function Mark({ markup, size = 24 }: LogoProps & { markup: string }) {
  return (
    <span
      className="provider-mark"
      style={{ width: size, height: size }}
      dangerouslySetInnerHTML={{ __html: markup }}
    />
  );
}

/** Fallback for a provider with no mark of its own. */
function GenericMark({ size = 24 }: LogoProps) {
  return (
    <span className="provider-mark" style={{ width: size, height: size }}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} role="img" aria-label="Provider">
        <circle cx="12" cy="12" r="7" />
      </svg>
    </span>
  );
}

/** Render the monochrome mark for a known provider, or a neutral fallback. */
export function ProviderLogo({ provider, size }: { provider: string } & LogoProps) {
  switch (provider) {
    case "claude":
      return <Mark markup={claudeMark} size={size} />;
    case "codex":
      return <Mark markup={codexMark} size={size} />;
    case "opencode-go":
      return <Mark markup={opencodeMark} size={size} />;
    default:
      return <GenericMark size={size} />;
  }
}
