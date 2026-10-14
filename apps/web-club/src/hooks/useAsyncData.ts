import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ApiRequestError, NotImplementedStubError } from '../api/client';

export interface AsyncState<T> {
  data: T | null;
  loading: boolean;
  error: { message: string; code?: string; notImplemented?: boolean } | null;
}

/**
 * Data-fetching hook for club pages: runs `fetcher`, tracks loading/error,
 * and redirects to /login on 401 (expired/absent Bearer token — the
 * in-memory access token is cleared on 401, see api/client.ts).
 *
 * "backend not implemented yet" stubs surface as `error.notImplemented`
 * so pages can render a waiting-for-backend empty state instead of a
 * retryable failure.
 *
 * `fetcher` must be a stable callback (wrap in useCallback at the call site).
 */
export function useAsyncData<T>(fetcher: () => Promise<T>): AsyncState<T> & {
  reload: () => void;
} {
  const navigate = useNavigate();
  const [state, setState] = useState<AsyncState<T>>({
    data: null,
    loading: true,
    error: null,
  });
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setState((prev) => ({ ...prev, loading: true, error: null }));

    fetcher()
      .then((data) => {
        if (!cancelled) setState({ data, loading: false, error: null });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        if (err instanceof ApiRequestError && err.status === 401) {
          navigate('/login', { replace: true });
          return;
        }
        setState({
          data: null,
          loading: false,
          error: {
            message: err instanceof Error ? err.message : 'Request failed',
            code:
              err instanceof ApiRequestError || err instanceof NotImplementedStubError
                ? err.code
                : undefined,
            notImplemented: err instanceof NotImplementedStubError,
          },
        });
      });

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fetcher, tick, navigate]);

  const reload = useCallback(() => setTick((t) => t + 1), []);

  return { ...state, reload };
}
