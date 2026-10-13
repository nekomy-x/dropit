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
      className="rounded-xl border border-red-900/60 bg-red-950/40 p-6"
      role="alert"
    >
      <p className="text-sm font-semibold text-red-200">
        Something went wrong
      </p>
      <p className="mt-1 text-sm text-red-300/80">{message}</p>
      {code ? (
        <p className="mt-1 font-mono text-xs text-red-400/60">{code}</p>
      ) : null}
      {onRetry ? (
        <button type="button" onClick={onRetry} className="btn-secondary mt-4">
          Try again
        </button>
      ) : null}
    </div>
  );
}
