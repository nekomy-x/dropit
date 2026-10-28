/**
 * The sign preview — one draw, shown at any size.
 *
 * The studio's contract is that what is on screen *is* the export: this
 * component renders the exact SVG string the export pipeline receives, so a
 * difference between them is impossible by construction. It only scales it.
 *
 * `dangerouslySetInnerHTML` is used deliberately: the string is produced by our
 * own `qrCardSvg()` from typed values, and every user-supplied character in it
 * is escaped by `escapeXml()` before it reaches the markup. Rendering it as a
 * React tree instead would mean re-implementing the compositor in JSX and then
 * having two drawings to keep in sync.
 */
import { useId, useMemo } from 'react';

export interface QrSignPreviewProps {
  /** A complete `<svg>` document string. */
  svg: string;
  /** Accessible description of what the preview shows. */
  label: string;
  className?: string;
  /** Draw a hairline edge around the sign so white paper is visible on white. */
  edge?: boolean;
}

export default function QrSignPreview({ svg, label, className, edge = true }: QrSignPreviewProps) {
  const id = useId();
  // A React key on the wrapper forces the browser to replace the node when the
  // document changes, which avoids stale SVG defs ids being reused.
  const markup = useMemo(() => ({ __html: svg }), [svg]);
  return (
    <div
      className={['qr-sign-preview', edge ? 'is-edged' : null, className].filter(Boolean).join(' ')}
      data-qr-sign={id}
      role="img"
      aria-label={label}
      dangerouslySetInnerHTML={markup}
    />
  );
}
