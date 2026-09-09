import "./ProviderLogo.css";
import claudeMark from "../assets/logos/claude.svg?raw";
import codexMark from "../assets/logos/codex.svg?raw";
import opencodeMark from "../assets/logos/opencode.svg?raw";

/**
 * Provider marks.
 *
 * Used to identify whose quota a badge is reporting. That is nominative use —
 * naming a service to say which one this is — not branding, and the app never
 * suggests any endorsement or partnership.
 *
 * The artwork lives in `src/assets/logos` rather than being pasted in here, so
 * the files the vendors publish stay the single source of truth and can be
 * replaced wholesale without touching this component.
 *
 * **The marks render in `currentColor`, not in their brand colours.** Colour in
 * this widget means exactly one thing — how close a quota is to running out —
 * and Anthropic's clay sits a few degrees of hue from `--level-high`, so a
 * brand-coloured mark inside a green ring would read as a warning that is not
 * there. Identity is carried by the shape, which is unaltered.
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
      return <Mark markup={claudeMark} size={size} />;
    case "codex":
      return <Mark markup={codexMark} size={size} />;
    case "opencode-go":
      return <Mark markup={opencodeMark} size={size} />;
    default:
      return <GenericMark size={size} />;
  }
}
