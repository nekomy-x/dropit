/**
 * Loading and skeleton states, in the shared panel language.
 *
 * `skeleton` (panel.css) is the only loading surface: a neutral raised block
 * that pulses, collapses under `prefers-reduced-motion`, and reads correctly in
 * both themes. Nothing here invents data — the shapes only hold the layout.
 */

import { useLocale } from '../locale';

/** Full-width loading indicator for pages awaiting their first API response. */
export default function Loading({ label }: { label?: string }) {
  const { t } = useLocale();
  return (
    <div
      className="panel-card"
      role="status"
      aria-live="polite"
      style={{ display: 'flex', alignItems: 'center', gap: 12, color: 'var(--c-text-2)', fontSize: 'var(--fs-13)' }}
    >
      <span className="skeleton" style={{ width: 16, height: 16, borderRadius: 'var(--r-chip)', flex: '0 0 auto' }} aria-hidden="true" />
      {label ?? t('common.loading')}
    </div>
  );
}

/**
 * Skeleton rows (dizayn-zadanie §5: «состояния — загрузка (скелетоны)»):
 * the shape of the table that is about to arrive, instead of an empty box.
 */
export function SkeletonRows({ rows = 5, cols = 3 }: { rows?: number; cols?: number }) {
  const { t } = useLocale();
  return (
    <div className="panel-card" role="status" aria-label={t('shared.loadingTable')}>
      {Array.from({ length: rows }).map((_, r) => (
        <div
          key={r}
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 16,
            padding: '14px 0',
            borderBottom: r === rows - 1 ? 0 : '1px solid var(--c-hairline-row)',
          }}
        >
          {Array.from({ length: cols }).map((_, c) => (
            <span
              key={c}
              className="skeleton"
              style={{ height: 12, width: `${[60, 25, 15][c % 3]}%` }}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

/** Skeletons for the KPI row (large numbers, no text). */
export function SkeletonTiles({ tiles = 4 }: { tiles?: number }) {
  const { t } = useLocale();
  return (
    <div className="stat-grid" role="status" aria-label={t('shared.loadingStats')}>
      {Array.from({ length: tiles }).map((_, i) => (
        <div key={i} className="stat-card">
          <span className="skeleton" style={{ display: 'block', height: 12, width: '50%' }} />
          <span className="skeleton" style={{ display: 'block', height: 28, width: '66%', marginTop: 14 }} />
          <span className="skeleton" style={{ display: 'block', height: 10, width: '42%', marginTop: 12 }} />
        </div>
      ))}
    </div>
  );
}
