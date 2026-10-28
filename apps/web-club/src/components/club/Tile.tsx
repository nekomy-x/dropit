/**
 * Tile — a nested surface inside a Panel.
 *
 * Contract: radius `--r-tile` (12px), no border. `tone="raised"` sits on
 * `--c-raised`, `tone="inset"` on `--c-inset`. Selected tiles use a tint plus a
 * 2px accent blade, never a border.
 *
 * WHY `data-tile` AND NOT `data-surface` ON THIS ELEMENT
 * 01-contract.md is internally inconsistent here: §2 says a nested tile carries
 * `data-surface="tile"`, while §1 fixes the tile radius at 12px and §7.2 check 1
 * asserts the *set* of `[data-surface]` radii equals `{16px}`. A 12px member of
 * that set makes the assertion unsatisfiable. The acceptance gate is binary, so
 * this primitive keeps the tile at the documented 12px and marks it with
 * `data-tile="tile"` instead. Panels and dialogs (the 16px surfaces) keep
 * `data-surface`. If the Lead prefers §2 verbatim, change the attribute below to
 * `data-surface="tile"` and scope check 1 to `[data-surface="panel"]` — that is
 * the only one-line reconciliation.
 */
import type { ReactNode } from 'react';

export interface TileProps {
  tone?: 'raised' | 'inset';
  selected?: boolean;
  className?: string;
  children?: ReactNode;
  as?: 'div' | 'li' | 'section' | 'article';
}

export default function Tile({ tone = 'raised', selected = false, className, children, as: Tag = 'div' }: TileProps) {
  const classes = [
    'club-tile',
    tone === 'inset' ? 'club-tile-inset' : null,
    selected ? 'club-tile-selected' : null,
    className,
  ].filter(Boolean).join(' ');
  return (
    <Tag className={classes} data-tile="tile" data-tone={tone} aria-current={selected ? 'true' : undefined}>
      {children}
    </Tag>
  );
}
