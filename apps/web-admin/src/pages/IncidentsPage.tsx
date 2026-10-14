import { useCallback, useState } from 'react';
import {
  ApiRequestError,
  listIncidents,
  resolveIncident,
  type IncidentDto,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

/**
 * Incidents page — a LIVE admin function (F6):
 *   GET /v1/admin/incidents            → {items: [{id, kind, message}]}
 *   POST /v1/admin/incidents/:id/resolve → {resolved: true}
 * The backend lists only OPEN incidents; resolving removes a row from
 * the next fetch.
 */
export default function IncidentsPage() {
  const fetchIncidents = useCallback(() => listIncidents(), []);
  const { data, loading, error, reload } = useAsyncData(fetchIncidents);

  const [resolvingId, setResolvingId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const onResolve = async (incident: IncidentDto) => {
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

  return (
    <div>
      <h1 className="page-title">Incidents</h1>
      <p className="page-subtitle">
        Payout failures, ledger invariant violations and reconciliation
        discrepancies raised by the F6 backend. Resolve an incident once the
        underlying cause is handled.
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
          data.items.length === 0 ? (
            <p className="text-sm text-neutral-400">
              No open incidents. All quiet.
            </p>
          ) : (
            <ul className="space-y-4">
              {data.items.map((incident) => (
                <li key={incident.id} className="card">
                  <div className="flex items-start justify-between gap-4">
                    <div>
                      <p className="mt-0.5 font-mono text-xs text-neutral-500">
                        {incident.kind} · {incident.id}
                      </p>
                      <p className="mt-2 text-sm text-neutral-300">
                        {incident.message}
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
    </div>
  );
}
