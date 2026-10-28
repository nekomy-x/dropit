import { useCallback, useEffect, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { ApiRequestError, NotImplementedStubError } from '../api/client';

export interface AsyncState<T> {
  data: T | null;
  loading: boolean;
  error: { message: string; code?: string; notImplemented?: boolean } | null;
  /**
   * Honesty fields (F18). Additive: every existing call site that reads only
   * `data` / `loading` / `error` keeps compiling and behaves exactly as before.
   *
   * `failedAt` — ms epoch of the latest failed fetch, `null` while the latest
   *   attempt succeeded (or no attempt has finished yet).
   * `staleSince` — ms epoch of the successful fetch that produced the `data`
   *   currently on screen; set only while the latest attempt failed. It is
   *   `null` together with `data === null` ("no data now") and non-null when
   *   `data !== null` ("this is old data"). A page can therefore stop
   *   presenting previous values as current instead of silently keeping them.
   *
   * A successful fetch clears both; starting a retry leaves them in place so a
   * page that renders "showing data from …" does not flip back to "live" while
   * the new request is still in flight.
   */
  failedAt: number | null;
  staleSince: number | null;
}

export interface AsyncDataResult<T> extends AsyncState<T> {
  reload: () => void;
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
export function useAsyncData<T>(
  fetcher: () => Promise<T>,
  refreshInterval?: number,
): AsyncDataResult<T> {
  const navigate = useNavigate();
  const location = useLocation();
  const [state, setState] = useState<AsyncState<T>>({
    data: null,
    loading: true,
    error: null,
    failedAt: null,
    staleSince: null,
  });
  const [tick, setTick] = useState(0);
  // Wall-clock of the last successful fetch; `staleSince` reports it while the
  // latest attempt is failing, so the age of the stale data is measurable.
  const lastSuccessAt = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setState((prev) => ({ ...prev, loading: true, error: null }));

    const load = async () => {
      try {
        const data = await fetcher();
        if (!cancelled) {
          lastSuccessAt.current = Date.now();
          setState({
            data,
            loading: false,
            error: null,
            failedAt: null,
            staleSince: null,
          });
        }
      } catch (err: unknown) {
        if (cancelled) return;
        if (err instanceof ApiRequestError && err.status === 401) {
          navigate(window.location.pathname.startsWith('/dj') ? '/dj/login' : '/login', { replace: true, state: { from: `${location.pathname}${location.search}${location.hash}` } });
          return;
        }
        setState((previous) => ({
          data: previous.data,
          loading: false,
          error: {
            message: err instanceof Error ? err.message : 'Request failed',
            code:
              err instanceof ApiRequestError || err instanceof NotImplementedStubError
                ? err.code
                : undefined,
            notImplemented: err instanceof NotImplementedStubError,
          },
          // Previous data may still be on screen, but it is no longer being
          // presented as current: `staleSince` says when it was fetched.
          failedAt: Date.now(),
          staleSince: previous.data !== null ? lastSuccessAt.current : null,
        }));
      }

      // Schedule only after completion: slow responses never overlap polling.
      if (!cancelled && refreshInterval && refreshInterval > 0) {
        timer = setTimeout(() => { void load(); }, refreshInterval);
      }
    };
    void load();

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fetcher, tick, navigate, refreshInterval]);

  const reload = useCallback(() => setTick((t) => t + 1), []);

  return { ...state, reload };
}
