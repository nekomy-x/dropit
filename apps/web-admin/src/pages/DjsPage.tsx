import { useCallback } from 'react';
import { listDjs, type AdminDj } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';

/**
 * DJs — "backend not implemented yet": GET /v1/admin/djs has no backend
 * route yet, so the client method rejects locally with a 501 stub and
 * the page renders a waiting-for-backend empty state.
 */
export default function DjsPage() {
  const fetchDjs = useCallback(() => listDjs(), []);
  const { data, loading, error } = useAsyncData<{ items: AdminDj[] }>(fetchDjs);

  return (
    <div>
      <h1 className="page-title">DJs</h1>
      <p className="page-subtitle">
        All DJs across clubs. Blocking and suspension actions ship with the
        anti-fraud phase (F8).
      </p>

      <div className="mt-6">
        {loading ? (
          <p className="text-sm text-neutral-400">Loading DJs…</p>
        ) : null}

        {error ? (
          error.notImplemented ? (
            <p
              className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
              role="status"
            >
              DJ registry is waiting for the backend —{' '}
              <code>GET /v1/admin/djs</code> is not implemented yet.
            </p>
          ) : (
            <p className="text-sm text-red-300" role="alert">
              {error.message}
              {error.code ? ` (${error.code})` : ''}
            </p>
          )
        ) : null}

        {data && data.items.length > 0 ? (
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
        ) : null}
      </div>
    </div>
  );
}
