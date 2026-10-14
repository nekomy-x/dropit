/** Full-width loading indicator for pages awaiting their first API response. */
export default function Loading({ label = 'Loading…' }: { label?: string }) {
  return (
    <div
      className="flex items-center justify-center gap-3 rounded-xl border border-neutral-800 bg-neutral-900 p-10 text-sm text-neutral-400"
      role="status"
      aria-live="polite"
    >
      <span
        className="h-4 w-4 animate-spin rounded-full border-2 border-neutral-600 border-t-brand-500"
        aria-hidden="true"
      />
      {label}
    </div>
  );
}

/**
 * Skeleton rows (dizayn-zadanie §5: «состояния — загрузка
 * (скелетоны)»): серые пульсирующие заглушки в форме таблиц/карточек
 * вместо пустоты — структура видна до прихода данных.
 */
export function SkeletonRows({ rows = 5, cols = 3 }: { rows?: number; cols?: number }) {
  return (
    <div
      className="rounded-xl border border-neutral-800 bg-neutral-900 p-4"
      role="status"
      aria-label="Loading table"
    >
      {Array.from({ length: rows }).map((_, r) => (
        <div
          key={r}
          className="flex items-center gap-4 border-b border-neutral-800 py-3 last:border-0"
        >
          {Array.from({ length: cols }).map((_, c) => (
            <div
              key={c}
              className="h-4 animate-pulse rounded bg-neutral-700"
              style={{ width: `${[60, 25, 15][c % 3]}%` }}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

/** Skeleton для плиток дашборда (большие числа). */
export function SkeletonTiles({ tiles = 4 }: { tiles?: number }) {
  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4" role="status" aria-label="Loading stats">
      {Array.from({ length: tiles }).map((_, i) => (
        <div key={i} className="rounded-xl border border-neutral-800 bg-neutral-900 p-5">
          <div className="mb-3 h-3 w-1/2 animate-pulse rounded bg-neutral-700" />
          <div className="h-8 w-2/3 animate-pulse rounded bg-neutral-700" />
        </div>
      ))}
    </div>
  );
}
