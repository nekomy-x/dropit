import { useCallback } from 'react';
import { getDashboardNow, type DashboardNow } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';

/**
 * Dashboard — "backend not implemented yet": GET /v1/admin/dashboard/now
 * has no backend route yet, so the client method rejects locally with a
 * 501 stub and the page renders a waiting-for-backend empty state.
 * The live admin surface is Payouts + Incidents (F6).
 */
export default function DashboardPage() {
  const fetchNow = useCallback(() => getDashboardNow(), []);
  const { data, loading, error } = useAsyncData<DashboardNow>(fetchNow);

  void data;

  return (
    <div>
      <h1 className="page-title">Platform dashboard</h1>
      <p className="page-subtitle">
        Live platform metrics, refreshed on every visit.
      </p>

      <div className="mt-6">
        {loading ? (
          <p className="text-sm text-neutral-400">Loading platform metrics…</p>
        ) : null}

        {error ? (
          error.notImplemented ? (
            <p
              className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
              role="status"
            >
              Live platform metrics are waiting for the backend —{' '}
              <code>GET /v1/admin/dashboard/now</code> is not implemented yet.
              The live admin functions are{' '}
              <a href="/payouts" className="text-brand-500 hover:text-brand-600">
                Payouts
              </a>{' '}
              and{' '}
              <a href="/incidents" className="text-brand-500 hover:text-brand-600">
                Incidents
              </a>{' '}
              (F6).
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
