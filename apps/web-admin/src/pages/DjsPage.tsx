import { useCallback } from 'react';
import { listDjs, type AdminDj, type Page } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

/** GET /v1/admin/djs — read-only DJ registry (block/suspend lands in F8). */
export default function DjsPage() {
  const fetchDjs = useCallback(() => listDjs(), []);
  const { data, loading, error, reload } = useAsyncData<Page<AdminDj>>(fetchDjs);

  return (
    <div>
      <h1 className="page-title">DJs</h1>
      <p className="page-subtitle">
        All DJs across clubs. Blocking and suspension actions ship with the
        anti-fraud phase (F8).
      </p>

      <div className="mt-6">
        {loading ? <Loading label="Loading DJs…" /> : null}
        {error ? (
          <ErrorState
            message={error.message}
            code={error.code}
            onRetry={reload}
          />
        ) : null}
        {data ? (
          data.items.length === 0 ? (
            <p className="text-sm text-neutral-400">No DJs registered yet.</p>
          ) : (
            <div className="card overflow-x-auto p-0">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-neutral-800 text-left text-xs uppercase tracking-wider text-neutral-500">
                    <th className="py-3 px-6 font-medium">DJ</th>
                    <th className="py-3 px-4 font-medium">Club</th>
                    <th className="py-3 px-4 font-medium">Stripe</th>
                    <th className="py-3 px-6 font-medium">Status</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((dj) => (
                    <tr key={dj.id} className="border-b border-neutral-800/60">
                      <td className="py-3 px-6">
                        <p className="font-medium text-neutral-100">
                          {dj.displayName}
                        </p>
                        <p className="text-xs text-neutral-500">{dj.email}</p>
                      </td>
                      <td className="py-3 px-4 font-mono text-xs text-neutral-400">
                        {dj.clubId ?? '— unassigned —'}
                      </td>
                      <td className="py-3 px-4 text-neutral-400">
                        {dj.stripeOnboardingStatus}
                      </td>
                      <td className="py-3 px-6">
                        <span className="rounded-full bg-neutral-800 px-2.5 py-0.5 text-xs font-semibold uppercase tracking-wider text-neutral-300">
                          {dj.status}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        ) : null}
      </div>
    </div>
  );
}
