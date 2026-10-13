import { useCallback, useState } from 'react';
import {
  ApiRequestError,
  listIncidents,
  resolveIncident,
  type Incident,
  type Page,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

/** GET /v1/admin/incidents + POST /:id/resolve — payout/reconciliation alerts. */
export default function IncidentsPage() {
  const fetchIncidents = useCallback(() => listIncidents(), []);
  const { data, loading, error, reload } = useAsyncData<Page<Incident>>(
    fetchIncidents,
  );

  const [resolvingId, setResolvingId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const onResolve = async (incident: Incident) => {
    setActionError(null);
    setResolvingId(incident.id);
    try {
      await resolveIncident(incident.id);
      reload();
    } catch (err) {
      setActionError(
        err instanceof ApiRequestError ? err.message : 'Resolve failed',
      );
    } finally {
      setResolvingId(null);
    }
  };

  const open = data?.items.filter((i) => i.status === 'open') ?? [];
  const resolved = data?.items.filter((i) => i.status === 'resolved') ?? [];

  return (
    <div>
      <h1 className="page-title">Incidents</h1>
      <p className="page-subtitle">
        Payout failures, ledger invariant violations and reconciliation
        discrepancies. Every resolution is written to the audit log.
      </p>

      {actionError ? (
        <p className="mt-4 text-sm text-red-300" role="alert">
          {actionError}
        </p>
      ) : null}

      <div className="mt-6">
        {loading ? <Loading label="Loading incidents…" /> : null}
        {error ? (
          <ErrorState
            message={error.message}
            code={error.code}
            onRetry={reload}
          />
        ) : null}
        {data ? (
          open.length === 0 ? (
            <p className="text-sm text-neutral-400">
              No open incidents. All quiet.
            </p>
          ) : (
            <ul className="space-y-4">
              {open.map((incident) => (
                <li key={incident.id} className="card">
                  <div className="flex items-start justify-between gap-4">
                    <div>
                      <p className="text-sm font-semibold text-white">
                        {incident.subject}
                      </p>
                      <p className="mt-0.5 font-mono text-xs text-neutral-500">
                        {incident.kind} · {incident.id}
                      </p>
                      <p className="mt-2 text-sm text-neutral-300">
                        {incident.details}
                      </p>
                      <p className="mt-2 text-xs text-neutral-500">
                        opened {new Date(incident.createdAt).toLocaleString()}
                      </p>
                    </div>
                    <button
                      type="button"
                      className="btn-primary shrink-0"
                      onClick={() => onResolve(incident)}
                      disabled={resolvingId === incident.id}
                    >
                      {resolvingId === incident.id
                        ? 'Resolving…'
                        : 'Resolve'}
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )
        ) : null}
      </div>

      {resolved.length > 0 ? (
        <section className="mt-10">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
            Recently resolved
          </h2>
          <ul className="mt-4 space-y-3">
            {resolved.map((incident) => (
              <li
                key={incident.id}
                className="rounded-lg border border-neutral-800 bg-neutral-900/50 p-4 text-sm"
              >
                <p className="text-neutral-200">{incident.subject}</p>
                <p className="mt-1 text-xs text-neutral-500">
                  {incident.kind} · resolved{' '}
                  {incident.resolvedAt
                    ? new Date(incident.resolvedAt).toLocaleString()
                    : '—'}
                </p>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
