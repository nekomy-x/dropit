import { useCallback } from 'react';
import { getDashboardNow, type DashboardNow } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

/** GET /v1/admin/dashboard/now — sessions, req/min, turnover, open incidents. */
export default function DashboardPage() {
  const fetchNow = useCallback(() => getDashboardNow(), []);
  const { data, loading, error, reload } = useAsyncData<DashboardNow>(fetchNow);

  return (
    <div>
      <h1 className="page-title">Platform dashboard</h1>
      <p className="page-subtitle">
        Live platform metrics, refreshed on every visit.
      </p>

      <div className="mt-6">
        {loading ? <Loading label="Loading platform metrics…" /> : null}
        {error ? (
          <ErrorState
            message={error.message}
            code={error.code}
            onRetry={reload}
          />
        ) : null}
        {data ? (
          <div className="grid gap-6 md:grid-cols-2 xl:grid-cols-4">
            <div className="card">
              <p className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Active sessions
              </p>
              <p className="mt-3 text-3xl font-bold text-white">
                {data.activeSessions}
              </p>
            </div>
            <div className="card">
              <p className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Requests / minute
              </p>
              <p className="mt-3 text-3xl font-bold text-white">
                {data.requestsPerMinute}
              </p>
            </div>
            <div className="card">
              <p className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Turnover today
              </p>
              <p className="mt-3 text-3xl font-bold text-brand-500">
                {(data.turnoverTodayMinor / 100).toFixed(2)}{' '}
                {data.currency}
              </p>
            </div>
            <div className="card">
              <p className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Open incidents
              </p>
              <p
                className={`mt-3 text-3xl font-bold ${
                  data.openIncidents > 0 ? 'text-red-400' : 'text-white'
                }`}
              >
                {data.openIncidents}
              </p>
              {data.openIncidents > 0 ? (
                <a
                  href="/incidents"
                  className="mt-2 inline-block text-xs font-medium text-brand-500 hover:text-brand-600"
                >
                  Review incidents →
                </a>
              ) : null}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}
