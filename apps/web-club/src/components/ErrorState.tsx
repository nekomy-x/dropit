interface ErrorStateProps {
  /** Human-readable message from the API error envelope, or a fallback. */
  message: string;
  /** i18n-able machine code (`http_503`, `rate_limited`, …) for debugging. */
  code?: string;
  onRetry?: () => void;
}

/** Error banner with optional retry; used by every page's failed state. */
export default function ErrorState({ message, code, onRetry }: ErrorStateProps) {
  return (
    <div
      className="ops-error-state rounded-md border border-rose-400/20 bg-rose-400/[0.06] p-4"
      role="alert"
    >
      <p className="text-sm font-semibold text-rose-200">
        Something went wrong
      </p>
      <p className="mt-1 text-sm text-rose-200/85">{message}</p>
      {code ? (
        <p className="mt-1 font-mono text-xs text-rose-200/65">{code}</p>
      ) : null}
      {onRetry ? (
        <button type="button" onClick={onRetry} className="btn-secondary mt-4">
          Try again
        </button>
      ) : null}
    </div>
  );
}
