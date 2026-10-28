/**
 * Icons owned by the QR studio.
 *
 * They live here rather than in `components/Icon.tsx` on purpose: the shared
 * icon map is the panel's single surface and is edited by other workstreams, and
 * a studio glyph appearing or disappearing with someone else's commit is a
 * defect nobody would look for. The four marks below are the only ones this
 * feature needs, they are imported straight from `lucide-react`, and nothing
 * outside `src/qr`/`src/components/qr` reads them.
 *
 * The wrapper copies `Icon`'s contract (`role`/`aria-label`, decorative by
 * default, `currentColor`, 1.75 stroke) so a studio icon and a panel icon look
 * identical on the same screen.
 */
import {
  CircleAlert,
  ImageUp,
  Link2,
  Printer,
  RotateCcw,
  Save,
  ScanLine,
  TriangleAlert,
  Ticket,
  type LucideIcon,
} from 'lucide-react';

export type QrIconName = 'save' | 'print' | 'link' | 'scan' | 'image' | 'warn' | 'inspect' | 'reset' | 'ticket';

const GLYPHS: Record<QrIconName, LucideIcon> = {
  save: Save,
  print: Printer,
  link: Link2,
  scan: ScanLine,
  image: ImageUp,
  warn: CircleAlert,
  inspect: TriangleAlert,
  reset: RotateCcw,
  ticket: Ticket,
};

export interface QrIconProps {
  name: QrIconName;
  size?: number;
  /** Accessible name; omit for a decorative icon next to a text label. */
  label?: string;
  className?: string;
}

export default function QrIcon({ name, size = 15, label, className }: QrIconProps) {
  const Glyph = GLYPHS[name];
  return (
    <span
      className={className}
      style={{ display: 'inline-flex', flex: '0 0 auto', width: size, height: size }}
      role={label ? 'img' : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      <Glyph size={size} strokeWidth={1.75} aria-hidden="true" focusable="false" />
    </span>
  );
}
