import { useLocale } from '../locale';

/** Full-width loading indicator for pages awaiting their first API response. */
export default function Loading({ label }: { label?: string }) {
  const { t } = useLocale();
  const text = label ?? t('common.loading');
  return (
    <div
      className="flex items-center justify-center gap-3 rounded-md border border-neutral-800 bg-[#151515] p-10 text-sm text-neutral-400"
      role="status"
      aria-live="polite"
    >
      <span
        className="h-4 w-4 animate-spin rounded-full border-2 border-neutral-600 border-t-brand-500"
        aria-hidden="true"
      />
      {text}
    </div>
  );
}

/**
 * Skeleton rows (dizayn-zadanie §5: «состояния — загрузка
 * (скелетоны)»): серые пульсирующие заглушки в форме таблиц/карточек
 * вместо пустоты — структура видна до прихода данных.
 */
export function SkeletonRows({ rows = 5, cols = 3 }: { rows?: number; cols?: number }) {
  const { t } = useLocale();
  return (
    <div
      className="rounded-md border border-neutral-800 bg-[#151515] p-4"
      role="status"
      aria-label={t('common.loadingTable')}
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
  const { t } = useLocale();
  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4" role="status" aria-label={t('common.loadingStats')}>
      {Array.from({ length: tiles }).map((_, i) => (
        <div key={i} className="rounded-md border border-neutral-800 bg-[#151515] p-5">
          <div className="mb-3 h-3 w-1/2 animate-pulse rounded bg-neutral-700" />
          <div className="h-8 w-2/3 animate-pulse rounded bg-neutral-700" />
        </div>
      ))}
    </div>
  );
}
