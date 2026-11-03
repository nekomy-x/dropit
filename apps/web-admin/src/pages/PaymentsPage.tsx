import { useCallback, useState } from 'react';
import { listPayments, type AdminPayment, type PaymentStatus } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';
import PageHeading from '../components/PageHeading';
import StatusBadge from '../components/StatusBadge';
import Icon from '../components/Icon';
import Select from '../components/ui/Select';
import { useLocale } from '../locale';
import { STATUS_MESSAGE_KEY } from '../messages/status';

const STATUS_OPTIONS: ReadonlyArray<PaymentStatus> = [
  'intent',
  'processing',
  'captured',
  'failed',
  'refunded',
  'partially_refunded',
  'cancelling',
  'cancelled',
];

export default function PaymentsPage() {
  const { t, locale } = useLocale();
  const count = (value: number) => new Intl.NumberFormat(locale).format(value);
  const money = (minor: number, currency: string) =>
    new Intl.NumberFormat(locale, { style: 'currency', currency, minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(minor / 100);
  const [status, setStatus] = useState<PaymentStatus | ''>('');
  const fetchPayments = useCallback(
    () => listPayments(status ? { status } : {}),
    [status],
  );
  const { data, loading, error, reload } = useAsyncData<{ items: AdminPayment[] }>(fetchPayments);

  return (
    <div>
      <PageHeading
        title={t('payments.title')}
        action={(
          <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
            <Icon name="lucide:refresh-cw" size={15} />
            {loading ? t('common.refreshing') : t('common.refresh')}
          </button>
        )}
      />

      <section className="panel-card" aria-label={t('payments.cardLabel')}>
        <header className="panel-card-head">
          <div>
            <h2 className="panel-card-title">{t('payments.cardTitle')}</h2>
            <p className="panel-card-sub">{t('payments.cardSubtitle')}</p>
          </div>
          <div className="panel-card-tools">
            {data ? <span className="chip chip-neutral">{t('payments.count', { count: count(data.items.length) })}</span> : null}
          </div>
        </header>

        <div className="admin-filter-single">
          <Select
            id="payment-status"
            label={t('payments.statusLabel')}
            value={status}
            onChange={(next) => setStatus(next as PaymentStatus | '')}
            options={[
              { value: '', label: t('payments.allPayments') },
              ...STATUS_OPTIONS.map((option) => {
                const key = STATUS_MESSAGE_KEY[option];
                return { value: option, label: key ? t(key) : option.replaceAll('_', ' ') };
              }),
            ]}
          />
        </div>

        {error ? <ErrorState message={error.message} code={error.code} onRetry={reload} /> : null}
        {loading && !data ? <Loading label={t('payments.loading')} /> : null}

        {data && data.items.length === 0 ? (
          <div className="panel-empty" role="status">
            <strong>{t('payments.emptyTitle')}</strong>
            <span>{t('payments.emptyBody')}</span>
          </div>
        ) : null}

        {data && data.items.length > 0 ? (
          <div className="table-wrap">
            <table className="data-table data-table--stack" style={{ minWidth: 760 }}>
              <thead>
                <tr>
                  <th scope="col">{t('payments.colPayment')}</th>
                  <th scope="col">{t('payments.colRequest')}</th>
                  <th scope="col" className="is-num">{t('payments.colAmount')}</th>
                  <th scope="col">{t('payments.colStatus')}</th>
                  <th scope="col">{t('payments.colCreated')}</th>
                </tr>
              </thead>
              <tbody>
                {data.items.map((payment) => (
                  <tr key={payment.id}>
                    <td data-label="" className="cell-primary is-mono">{payment.id.slice(0, 12)}</td>
                    <td data-label={t('payments.colRequest')} className="is-mono is-muted">{payment.requestId.slice(0, 12)}</td>
                    <td data-label={t('payments.colAmount')} className="is-num">
                      <span className="is-strong">{money(payment.amountMinor, payment.currency)}</span>
                      {payment.refundedMinor > 0 ? (
                        <span style={{ display: 'block', color: 'var(--c-text-muted-sm)', fontSize: 'var(--fs-11)' }}>
                          {t('payments.refunded', { amount: money(payment.refundedMinor, payment.currency) })}
                        </span>
                      ) : null}
                    </td>
                    <td data-label={t('payments.colStatus')}>
                      <div style={{ display: 'grid', justifyItems: 'start', gap: 4 }}>
                        <StatusBadge status={payment.status} />
                        {payment.failureReason ? (
                          <p style={{ maxWidth: 240, margin: 0, color: 'var(--c-error)', fontSize: 'var(--fs-11)' }}>{payment.failureReason}</p>
                        ) : null}
                      </div>
                    </td>
                    <td data-label={t('payments.colCreated')} className="is-mono" style={{ color: 'var(--c-text-muted)' }}>{new Date(payment.createdAt).toLocaleString(locale)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : null}
      </section>
    </div>
  );
}
