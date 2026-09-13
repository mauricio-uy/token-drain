import "./ProviderLogo.css";
import opencodeMark from "../assets/logos/opencode.svg?raw";

/**
 * Provider identifiers.
 *
 * Claude and Codex use neutral abbreviations instead of third-party artwork.
 * Their names identify the compatible subscriptions without redistributing or
 * modifying vendor logos. OpenCode's MIT-licensed mark is retained with its
 * notice in `src/assets/logos/OPENCODE-LICENSE.txt`.
 */

type LogoProps = {
  /** Edge length in CSS pixels. */
  size?: number;
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

function LetterMark({
  letters,
  label,
  size = 24,
}: LogoProps & { letters: string; label: string }) {
  return (
    <span
      className="provider-mark provider-mark--letters"
      style={{ width: size, height: size, fontSize: Math.max(7, size * 0.36) }}
      aria-label={label}
      role="img"
    >
      {letters}
    </span>
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
export function ProviderLogo({ provider, size }: { provider: string; size?: number }) {
  switch (provider) {
    case "claude":
      return <LetterMark letters="CL" label="Claude" size={size} />;
    case "codex":
      return <LetterMark letters="CX" label="Codex" size={size} />;
    case "opencode-go":
      return <Mark markup={opencodeMark} size={size} />;
    default:
      return <GenericMark size={size} />;
  }
}
