import { useCallback, useState } from 'react';
import {
  ApiRequestError,
  listPayouts,
  runPayoutCycle,
  runReconciliation,
  type PayoutRunResponse,
  type ReconciliationResponse,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

const STATUS_BADGE: Record<string, string> = {
  scheduled: 'bg-blue-900/40 text-blue-300',
  in_flight: 'bg-amber-900/40 text-amber-300',
  paid: 'bg-green-900/40 text-green-300',
  failed: 'bg-red-900/40 text-red-300',
  on_hold: 'bg-neutral-800 text-neutral-300',
};

function badgeClass(status: string): string {
  return STATUS_BADGE[status] ?? 'bg-neutral-800 text-neutral-300';
}

/**
 * Payouts page — a LIVE admin function (F6):
 *   GET /v1/admin/payouts           — payouts from the last week
 *   POST /v1/admin/payouts/run      — manual trigger of the payout cycle
 *   POST /v1/admin/reconciliation/run — manual reconciliation trigger
 */
export default function PayoutsPage() {
  const fetchPayouts = useCallback(() => listPayouts(), []);
  const { data, loading, error, reload } = useAsyncData(fetchPayouts);

  const [running, setRunning] = useState(false);
  const [runResult, setRunResult] = useState<PayoutRunResponse | null>(null);
  const [reconResult, setReconResult] = useState<ReconciliationResponse | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const onRunCycle = async () => {
    setActionError(null);
    setRunResult(null);
    setReconResult(null);
    setRunning(true);
    try {
      const result = await runPayoutCycle();
      setRunResult(result);
      reload();
    } catch (err) {
      setActionError(
        err instanceof ApiRequestError ? err.message : 'Payout cycle failed',
      );
    } finally {
      setRunning(false);
    }
  };

  const onRunReconciliation = async () => {
    setActionError(null);
    setRunResult(null);
    setReconResult(null);
    setRunning(true);
    try {
      const result = await runReconciliation();
      setReconResult(result);
    } catch (err) {
      setActionError(
        err instanceof ApiRequestError ? err.message : 'Reconciliation failed',
      );
    } finally {
      setRunning(false);
    }
  };

  return (
    <div>
      <h1 className="page-title">Payouts</h1>
      <p className="page-subtitle">
        Weekly payout cycle (prod: cron Mon 10:00 UTC): net per payee,
        10.00 EUR minimum threshold, final payouts without threshold on
        block/leave. Transfers are mock-provider (Test Mode) until ADR-004.
      </p>

      <div className="mt-6 flex flex-wrap gap-3">
        <button
          type="button"
          className="btn-primary"
          onClick={onRunCycle}
          disabled={running}
        >
          {running ? 'Running…' : 'Run payout cycle'}
        </button>
        <button
          type="button"
          className="btn-secondary"
          onClick={onRunReconciliation}
          disabled={running}
        >
          {running ? 'Running…' : 'Run reconciliation'}
        </button>
      </div>

      {runResult ? (
        <p
          className="mt-4 rounded-lg border border-green-900/60 bg-green-950/30 px-4 py-3 text-sm text-green-200"
          role="status"
        >
          Payout cycle complete — {runResult.payouts_created} payout(s) created,
          total {(runResult.total_minor / 100).toFixed(2)} EUR.
        </p>
      ) : null}

      {reconResult ? (
        <p
          className="mt-4 rounded-lg border border-neutral-800 bg-neutral-900/60 px-4 py-3 text-sm text-neutral-300"
          role="status"
        >
          Reconciliation run{' '}
          <span className="font-mono text-xs">{reconResult.run_id}</span> —
          status <span className="font-semibold">{reconResult.status}</span>.
        </p>
      ) : null}

      {actionError ? (
        <p className="mt-4 text-sm text-red-300" role="alert">
          {actionError}
        </p>
      ) : null}

      <div className="mt-6">
        {loading ? <Loading label="Loading payouts…" /> : null}
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
              No payouts in the last week. Run the cycle to settle pending
              balances above the threshold.
            </p>
          ) : (
            <div className="card overflow-x-auto p-0">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-neutral-800 text-left text-xs uppercase tracking-wider text-neutral-500">
                    <th className="py-3 px-6 font-medium">Payout</th>
                    <th className="py-3 px-4 font-medium">Payee</th>
                    <th className="py-3 px-4 font-medium">Amount</th>
                    <th className="py-3 px-4 font-medium">Status</th>
                    <th className="py-3 px-4 font-medium">Attempts</th>
                    <th className="py-3 px-6 font-medium">Final</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((payout) => (
                    <tr
                      key={payout.id}
                      className="border-b border-neutral-800/60"
                    >
                      <td className="py-3 px-6 font-mono text-xs text-neutral-400">
                        {payout.id}
                      </td>
                      <td className="py-3 px-4">
                        <span className="rounded-full bg-neutral-800 px-2 py-0.5 text-xs font-semibold uppercase tracking-wider text-neutral-300">
                          {payout.payee_type}
                        </span>
                        <span className="ml-2 font-mono text-xs text-neutral-500">
                          {payout.payee_id.slice(0, 8)}
                        </span>
                      </td>
                      <td className="py-3 px-4 font-medium text-neutral-100">
                        {(payout.amount_minor / 100).toFixed(2)} EUR
                      </td>
                      <td className="py-3 px-4">
                        <span
                          className={`rounded-full px-2.5 py-0.5 text-xs font-semibold uppercase tracking-wider ${badgeClass(payout.status)}`}
                        >
                          {payout.status}
                        </span>
                      </td>
                      <td className="py-3 px-4 text-neutral-400">
                        {payout.attempts}
                      </td>
                      <td className="py-3 px-6 text-neutral-400">
                        {payout.is_final ? 'yes' : 'no'}
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
