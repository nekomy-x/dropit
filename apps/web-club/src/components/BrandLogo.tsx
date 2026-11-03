/**
 * BrandLogo — the DROP IT lockup: the headphone mark and the wordmark whose
 * letters are cut out of a bar field. It is the product's only wordmark, so it
 * is one component for every surface that shows it: the club rail, the DJ
 * console entry, the connect header, the phone mockup, and the sign-in card on
 * every panel — club, DJ and admin.
 *
 * The admin sign-in used to draw its own lockup (a tinted tile with the mark
 * plus the word as DOM text), which made the operator's way in look like a
 * different product from the way the same person signs in everywhere else.
 *
 * Canonical copy: edit here, then run `node scripts/sync-ui-tokens.mjs`, which
 * writes byte-identical copies to both panel apps. The bars are painted from
 * `--c-*` in `brand-logo.css` (also canonical here), never from a literal.
 */
import Icon from './Icon';

interface BrandLogoProps {
  size?: 'tiny' | 'small' | 'medium' | 'large';
  className?: string;
}

/** Bar heights as a fraction of the field, taken from the owner's artwork. */
const LEVELS = [0.38, 0.31, 0.25, 0.2, 0.18, 0.2, 0.25, 0.33, 0.42, 0.5, 0.58, 0.65, 0.71, 0.76, 0.79, 0.77, 0.71, 0.64, 0.56, 0.48, 0.41, 0.36];

export default function BrandLogo({ size = 'medium', className = '' }: BrandLogoProps) {
  // Gradient and mask ids are global in SVG: one per size keeps two lockups on
  // the same screen from resolving each other's definitions.
  const id = `dropit-wave-${size}`;
  const markSize = size === 'large' ? 19 : size === 'medium' ? 16 : size === 'small' ? 14 : 9;

  return (
    <span
      className={`brand-logo brand-logo-${size} ${className}`}
      role="img"
      aria-label="DROP IT"
    >
      <Icon name="lucide:headphones" size={markSize} strokeWidth={1.8} />
      <svg className="brand-logo-word" viewBox="0 0 108 27" aria-hidden="true">
        <defs>
          <linearGradient id={`${id}-mint`} x1="0" x2="1" y1="0" y2="0">
            <stop offset="0" className="brand-logo-stop-a" />
            <stop offset="1" className="brand-logo-stop-b" />
          </linearGradient>
          <mask id={`${id}-letters`}>
            <rect width="108" height="27" fill="black" />
            <text x="0" y="22" fill="white" fontFamily="Inter, sans-serif" fontSize="23" fontWeight="750" letterSpacing="-.8">DROP IT</text>
          </mask>
        </defs>
        <text className="brand-logo-letters" x="0" y="22" fontFamily="Inter, sans-serif" fontSize="23" fontWeight="750" letterSpacing="-.8">DROP IT</text>
        <g mask={`url(#${id}-letters)`}>
          {LEVELS.map((level, index) => (
            <rect
              key={index}
              x={(108 / LEVELS.length) * index}
              y={27 * (1 - level)}
              width={108 / LEVELS.length + 0.2}
              height={27 * level}
              fill={`url(#${id}-mint)`}
              className="brand-logo-bar"
              style={{ animationDelay: `${index * -73}ms` }}
            />
          ))}
        </g>
      </svg>
    </span>
  );
}
