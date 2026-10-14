import { useCallback } from 'react';
import { listGuests, type AdminGuest } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';

/**
 * Guests — "backend not implemented yet": GET /v1/admin/guests has no
 * backend route yet, so the client method rejects locally with a 501
 * stub and the page renders a waiting-for-backend empty state.
 */
export default function GuestsPage() {
  const fetchGuests = useCallback(() => listGuests(), []);
  const { data, loading, error } = useAsyncData<{ items: AdminGuest[] }>(
    fetchGuests,
  );

  return (
    <div>
      <h1 className="page-title">Guests</h1>
      <p className="page-subtitle">
        Guest devices (no registration — device tokens only). Blocking a
        device ships with the anti-fraud phase (F8).
      </p>

      <div className="mt-6">
        {loading ? (
          <p className="text-sm text-neutral-400">Loading guests…</p>
        ) : null}

        {error ? (
          error.notImplemented ? (
            <p
              className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
              role="status"
            >
              Guest registry is waiting for the backend —{' '}
              <code>GET /v1/admin/guests</code> is not implemented yet.
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
                  <th className="py-3 px-6 font-medium">Device</th>
                  <th className="py-3 px-4 font-medium">Platform</th>
                  <th className="py-3 px-4 font-medium">App version</th>
                  <th className="py-3 px-4 font-medium">First seen</th>
                  <th className="py-3 px-6 font-medium">Last seen</th>
                </tr>
              </thead>
              <tbody>
                {data.items.map((guest) => (
                  <tr key={guest.id} className="border-b border-neutral-800/60">
                    <td className="py-3 px-6 font-mono text-xs text-neutral-400">
                      {guest.id}
                    </td>
                    <td className="py-3 px-4 text-neutral-400">
                      {guest.platform}
                    </td>
                    <td className="py-3 px-4 text-neutral-400">
                      {guest.appVersion}
                    </td>
                    <td className="py-3 px-4 text-neutral-400">
                      {new Date(guest.firstSeen).toLocaleString()}
                    </td>
                    <td className="py-3 px-6 text-neutral-400">
                      {new Date(guest.lastSeen).toLocaleString()}
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
