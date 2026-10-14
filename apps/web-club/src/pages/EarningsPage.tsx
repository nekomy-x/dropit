import { useCallback } from 'react';
import { getEarnings, type EarningsSummary } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';

/**
 * Earnings page — "backend not implemented yet": GET /v1/club/earnings has
 * no backend route yet, so the client method rejects locally with a 501
 * stub and the page renders a waiting-for-backend empty state.
 */
export default function EarningsPage() {
  const fetchEarnings = useCallback(() => getEarnings('today'), []);
  const { data, loading, error } = useAsyncData<EarningsSummary>(fetchEarnings);

  void data;

  return (
    <div>
      <h1 className="page-title">Earnings</h1>
      <p className="page-subtitle">
        Turnover from paid requests, net of the DROP IT commission.
      </p>

      <div className="mt-6">
        {loading ? (
          <p className="text-sm text-neutral-400">Loading earnings…</p>
        ) : null}

        {error ? (
          error.notImplemented ? (
            <p
              className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
              role="status"
            >
              Earnings reporting is waiting for the backend —{' '}
              <code>GET /v1/club/earnings</code> is not implemented yet. Payout
              cycles themselves are live (F6) and run on the admin side.
            </p>
          ) : (
            <p className="text-sm text-red-300" role="alert">
              {error.message}
              {error.code ? ` (${error.code})` : ''}
            </p>
          )
        ) : null}
      </div>
    </div>
  );
}
