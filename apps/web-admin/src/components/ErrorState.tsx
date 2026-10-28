import { useLocale } from '../locale';

interface ErrorStateProps {
  message: string;
  code?: string;
  onRetry?: () => void;
}

/**
 * One failed-request banner per page (docs/ui/PANEL-DESIGN.md §Data honesty:
 * "a failed request produces one `panel-error` with the code and a retry —
 * not per-card error copy repeated four times"). No value is rendered here; the
 * page itself has to mark its figures unavailable.
 */
export default function ErrorState({ message, code, onRetry }: ErrorStateProps) {
  const { t } = useLocale();
  return (
    <div className="panel-error" role="alert">
      <div>
        <strong>{t('common.error')}</strong>
        <p style={{ margin: '2px 0 0', color: 'var(--c-text-2)', fontSize: 'var(--fs-12)' }}>{message}</p>
        {code ? <small>{code}</small> : null}
      </div>
      {onRetry ? (
        <button type="button" onClick={onRetry} className="btn btn-secondary btn-sm">
          {t('common.tryAgain')}
        </button>
      ) : null}
    </div>
  );
}
