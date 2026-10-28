/**
 * Chip — a compact status label (§4.9).
 *
 * Pill radius is allowed only here and on the Segmented runner. Tint background,
 * no border, normal case. `tone` is a semantic choice the page makes; the word
 * inside is what actually carries the status.
 */
import type { ReactNode } from 'react';

export type ChipTone = 'live' | 'warn' | 'error' | 'neutral';

export interface ChipProps {
  tone?: ChipTone;
  children: ReactNode;
  className?: string;
  /** When given, the chip becomes a control with a real hit target. */
  onClick?: () => void;
  title?: string;
}

export default function Chip({ tone = 'neutral', children, className, onClick, title }: ChipProps) {
  const classes = ['club-chip', `club-chip-${tone}`, className].filter(Boolean).join(' ');
  if (onClick) {
    return (
      <button type="button" className={classes} onClick={onClick} data-control="button" title={title}>
        {children}
      </button>
    );
  }
  return <span className={classes} title={title}>{children}</span>;
}
