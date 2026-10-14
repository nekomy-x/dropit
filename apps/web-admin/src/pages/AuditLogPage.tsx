import { useCallback } from 'react';
import { getAuditLog, type AuditLogEntry } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';

/**
 * Audit log — "backend not implemented yet": GET /v1/admin/audit-log has
 * no backend route yet, so the client method rejects locally with a 501
 * stub and the page renders a waiting-for-backend empty state.
 */
export default function AuditLogPage() {
  const fetchLog = useCallback(() => getAuditLog(), []);
  const { data, loading, error } = useAsyncData<{ items: AuditLogEntry[] }>(
    fetchLog,
  );

  return (
    <div>
      <h1 className="page-title">Audit log</h1>
      <p className="page-subtitle">
        Append-only record of every admin action touching clubs, payments and
        platform settings. Entries are never edited or deleted.
      </p>

      <div className="mt-6">
        {loading ? (
          <p className="text-sm text-neutral-400">Loading audit log…</p>
        ) : null}

        {error ? (
          error.notImplemented ? (
            <p
              className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
              role="status"
            >
              The audit log is waiting for the backend —{' '}
              <code>GET /v1/admin/audit-log</code> is not implemented yet.
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
                  <th className="py-3 px-6 font-medium">When</th>
                  <th className="py-3 px-4 font-medium">Actor</th>
                  <th className="py-3 px-4 font-medium">Action</th>
                  <th className="py-3 px-6 font-medium">Entity</th>
                </tr>
              </thead>
              <tbody>
                {data.items.map((entry) => (
                  <tr key={entry.id} className="border-b border-neutral-800/60">
                    <td className="py-3 px-6 text-neutral-400">
                      {new Date(entry.at).toLocaleString()}
                    </td>
                    <td className="py-3 px-4 text-neutral-400">
                      <span className="font-mono text-xs">
                        {entry.actorType}:{entry.actorId.slice(0, 8)}
                      </span>
                    </td>
                    <td className="py-3 px-4 font-medium text-neutral-100">
                      {entry.action}
                    </td>
                    <td className="py-3 px-6 font-mono text-xs text-neutral-400">
                      {entry.entity}
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
