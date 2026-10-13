import { useCallback, useState } from 'react';
import { getEarnings, type EarningsSummary } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

const PERIODS: ReadonlyArray<{ value: string; label: string }> = [
  { value: 'today', label: 'Today' },
  { value: 'week', label: 'This week' },
  { value: 'month', label: 'This month' },
];

function formatMinor(minor: number, currency: string): string {
  return `${(minor / 100).toFixed(2)} ${currency}`;
}

/** GET /v1/club/earnings?period= — gross, net and request count per period. */
export default function EarningsPage() {
  const [period, setPeriod] = useState('today');

  const fetchEarnings = useCallback(() => getEarnings(period), [period]);
  const { data, loading, error, reload } =
    useAsyncData<EarningsSummary>(fetchEarnings);

  return (
    <div>
      <h1 className="page-title">Earnings</h1>
      <p className="page-subtitle">
        Turnover from paid requests, net of the DROP IT commission.
      </p>

      <div
        className="mt-6 inline-flex rounded-lg border border-neutral-800 bg-neutral-900 p-1"
        role="group"
        aria-label="Earnings period"
      >
        {PERIODS.map((item) => (
          <button
            key={item.value}
            type="button"
            onClick={() => setPeriod(item.value)}
            className={`rounded-md px-3 py-1.5 text-sm font-medium transition-colors ${
              period === item.value
                ? 'bg-brand-700 text-white'
                : 'text-neutral-400 hover:text-white'
            }`}
            aria-pressed={period === item.value}
          >
            {item.label}
          </button>
        ))}
      </div>

      <div className="mt-6">
        {loading ? <Loading label="Loading earnings…" /> : null}
        {error ? (
          <ErrorState
            message={error.message}
            code={error.code}
            onRetry={reload}
          />
        ) : null}
        {data ? (
          <div className="grid gap-6 md:grid-cols-3">
            <div className="card">
              <p className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Gross turnover
              </p>
              <p className="mt-3 text-3xl font-bold text-white">
                {formatMinor(data.grossMinor, data.currency)}
              </p>
              <p className="mt-1 text-xs text-neutral-500">
                {data.requestsCount} paid requests
              </p>
            </div>
            <div className="card">
              <p className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Your net
              </p>
              <p className="mt-3 text-3xl font-bold text-brand-500">
                {formatMinor(data.netMinor, data.currency)}
              </p>
              <p className="mt-1 text-xs text-neutral-500">
                after the platform commission
              </p>
            </div>
            <div className="card">
              <p className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Payouts
              </p>
              <p className="mt-3 text-sm text-neutral-300">
                Weekly payouts, sent when your balance reaches the 10.00 EUR
                minimum threshold.
              </p>
              <p className="mt-2 text-xs text-neutral-500">
                Payout history appears here once the F6 backend lands.
              </p>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}
