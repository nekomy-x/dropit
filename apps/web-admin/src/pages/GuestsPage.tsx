import { useCallback } from 'react';
import { listGuests, type AdminGuest, type Page } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

/** GET /v1/admin/guests — guest devices (read; block lands in F8). */
export default function GuestsPage() {
  const fetchGuests = useCallback(() => listGuests(), []);
  const { data, loading, error, reload } = useAsyncData<Page<AdminGuest>>(
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
        {loading ? <Loading label="Loading guests…" /> : null}
        {error ? (
          <ErrorState
            message={error.message}
            code={error.code}
            onRetry={reload}
          />
        ) : null}
        {data ? (
          data.items.length === 0 ? (
            <p className="text-sm text-neutral-400">No guest devices yet.</p>
          ) : (
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
          )
        ) : null}
      </div>
    </div>
  );
}
