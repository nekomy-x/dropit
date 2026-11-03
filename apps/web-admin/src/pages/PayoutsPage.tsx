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
import StatusBadge from '../components/StatusBadge';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import { useLocale } from '../locale';

/**
 * Payouts page — a LIVE admin function (F6):
 *   GET /v1/admin/payouts           — payouts from the last week
 *   POST /v1/admin/payouts/run      — manual trigger of the payout cycle
 *   POST /v1/admin/reconciliation/run — manual reconciliation trigger
 */
export default function PayoutsPage() {
  const { t, locale } = useLocale();
  const money = (minor: number) => new Intl.NumberFormat(locale, { style: 'currency', currency: 'EUR', minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(minor / 100);
  const count = (value: number) => new Intl.NumberFormat(locale).format(value);
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
        err instanceof ApiRequestError ? err.message : t('payouts.cycleFailed'),
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
        err instanceof ApiRequestError ? err.message : t('payouts.reconciliationFailed'),
      );
    } finally {
      setRunning(false);
    }
  };

  return (
    <div>
      <PageHeading
        title={t('payouts.title')}
        action={(
          <>
            <button type="button" className="btn btn-primary" onClick={onRunCycle} disabled={running}>
              <Icon name="lucide:arrow-down-to-line" size={15} />
              {running ? t('payouts.running') : t('payouts.runCycle')}
            </button>
            <button type="button" className="btn btn-secondary" onClick={onRunReconciliation} disabled={running}>
              <Icon name="lucide:scan-search" size={15} />
              {running ? t('payouts.running') : t('payouts.runReconciliation')}
            </button>
          </>
        )}
      />

      <p className="admin-page-note">{t('payouts.note')}</p>

      {runResult ? (
        <p className="admin-toast" role="status">
          {t('payouts.cycleComplete', { count: count(runResult.payouts_created), amount: money(runResult.total_minor) })}
        </p>
      ) : null}

      {reconResult ? (
        <p className="admin-toast" role="status">
          {t('payouts.reconciliationRun', { id: reconResult.run_id, status: reconResult.status })}
        </p>
      ) : null}

      {actionError ? <p className="panel-error" role="alert" style={{ marginTop: 16 }}>{actionError}</p> : null}

      {error ? <ErrorState message={error.message} code={error.code} onRetry={reload} /> : null}
      {loading && !data ? <Loading label={t('payouts.loading')} /> : null}

      {data ? (
        data.items.length === 0 ? (
          <div className="panel-empty" role="status">
            <strong>{t('payouts.emptyTitle')}</strong>
            <span>{t('payouts.emptyBody')}</span>
          </div>
        ) : (
          <section className="panel-card" aria-label={t('payouts.cardLabel')}>
            <header className="panel-card-head">
              <div>
                <h2 className="panel-card-title">{t('payouts.lastSevenDays')}</h2>
                <p className="panel-card-sub">{t('payouts.cardSubtitle')}</p>
              </div>
              <div className="panel-card-tools">
                <span className="chip chip-neutral">{t('payouts.count', { count: count(data.items.length) })}</span>
              </div>
            </header>

            <div className="table-wrap">
              <table className="data-table data-table--stack" style={{ minWidth: 760 }}>
                <thead>
                  <tr>
                    <th scope="col">{t('payouts.colPayout')}</th>
                    <th scope="col">{t('payouts.colPayee')}</th>
                    <th scope="col" className="is-num">{t('payouts.colAmount')}</th>
                    <th scope="col">{t('payouts.colStatus')}</th>
                    <th scope="col" className="is-num">{t('payouts.colAttempts')}</th>
                    <th scope="col">{t('payouts.colFinal')}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((payout) => (
                    <tr key={payout.id}>
                      <td data-label="" className="cell-primary is-mono">{payout.id}</td>
                      <td data-label={t('payouts.colPayee')}>
                        <span className="chip chip-neutral">{payout.payee_type}</span>
                        <span className="is-mono" style={{ marginLeft: 8, color: 'var(--c-text-muted-sm)' }}>{payout.payee_id.slice(0, 8)}</span>
                      </td>
                      <td data-label={t('payouts.colAmount')} className="is-num is-strong">{money(payout.amount_minor)}</td>
                      <td data-label={t('payouts.colStatus')}><StatusBadge status={payout.status} /></td>
                      <td data-label={t('payouts.colAttempts')} className="is-num">{payout.attempts}</td>
                      <td data-label={t('payouts.colFinal')}>{payout.is_final ? t('payouts.yes') : t('payouts.no')}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        )
      ) : null}
    </div>
  );
}
