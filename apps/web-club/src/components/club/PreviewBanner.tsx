/**
 * PreviewBanner — the single board-level notice for preview data (§5.1 #2).
 *
 * Stamps `data-preview-banner`, which is what §7.2 check 11 looks for whenever
 * any `[data-metric][data-source="preview"]` exists on the screen. It is not
 * fine print: 12px, full-width, one per board.
 */
import { TriangleAlert } from 'lucide-react';
import { useLocale } from '../../locale';

export interface PreviewBannerProps {
  /** How many metrics on the board are preview-only (for the copy). */
  count?: number;
  children?: string;
}

export default function PreviewBanner({ count, children }: PreviewBannerProps) {
  const { t } = useLocale();
  const body = children ?? t('preview.body');
  return (
    <div className="club-preview-banner" data-preview-banner="" role="note">
      <TriangleAlert size={15} aria-hidden="true" />
      <span>
        <strong>{t('preview.lead')}</strong> {body}
        {typeof count === 'number' && count > 0 ? ` ${t('preview.count', { count })}` : null}
      </span>
    </div>
  );
}
