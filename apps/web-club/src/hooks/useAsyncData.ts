import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ApiRequestError } from '../api/client';

export interface AsyncState<T> {
  data: T | null;
  loading: boolean;
  error: { message: string; code?: string } | null;
}

/**
 * Minimal data-fetching hook for the F7 skeleton pages:
 * runs `fetcher`, tracks loading/error, and redirects to /login on 401
 * (expired httpOnly session — auth model Part I §3 of the tech map).
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
            code: err instanceof ApiRequestError ? err.code : undefined,
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
