/**
 * ErrorState — ONE banner per board (§4.9, §5.3).
 *
 * Tint at 10%, no border, honest copy that says what happened and what to do
 * next, plus a retry that reports its in-flight state so a double click cannot
 * fire two requests. `code` is shown in mono for support, never as the message.
 */
import { CircleAlert } from 'lucide-react';
import { useLocale } from '../../locale';

export interface ErrorStateProps {
  /** Human sentence: what happened. */
  message: string;
  /** Machine code for support (`http_500`, `rate_limited`, …). */
  code?: string;
  onRetry?: () => void;
  /** True while the retry request is in flight. */
  retrying?: boolean;
  /** Override the lead line; keep it factual. */
  title?: string;
}

export default function ErrorState({
  message,
  code,
  onRetry,
  retrying = false,
  title,
}: ErrorStateProps) {
  const { t } = useLocale();
  const headline = title ?? t('dashboard.boardError');
  return (
    <div className="club-error-banner" data-error-state="" role="alert">
      <CircleAlert size={16} aria-hidden="true" />
      <div className="club-error-banner-copy">
        <p className="club-state-title">{headline}</p>
        <p className="club-state-detail">{message}</p>
        {code ? <p className="club-state-code">{code}</p> : null}
      </div>
      {onRetry ? (
        <button type="button" className="club-button" data-control="button" onClick={onRetry} disabled={retrying}>
          {retrying ? t('dashboard.retrying') : t('common.tryAgain')}
        </button>
      ) : null}
    </div>
  );
}
