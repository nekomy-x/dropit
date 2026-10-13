import { useCallback } from 'react';
import { getAuditLog, type AuditLogEntry, type Page } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

/** GET /v1/admin/audit-log — append-only trail of admin actions & money. */
export default function AuditLogPage() {
  const fetchLog = useCallback(() => getAuditLog(), []);
  const { data, loading, error, reload } = useAsyncData<Page<AuditLogEntry>>(
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
        {loading ? <Loading label="Loading audit log…" /> : null}
        {error ? (
          <ErrorState
            message={error.message}
            code={error.code}
            onRetry={reload}
          />
        ) : null}
        {data ? (
          data.items.length === 0 ? (
            <p className="text-sm text-neutral-400">No entries yet.</p>
          ) : (
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
                    <tr
                      key={entry.id}
                      className="border-b border-neutral-800/60"
                    >
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
          )
        ) : null}
      </div>
    </div>
  );
}
