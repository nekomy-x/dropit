import { useCallback, useState } from 'react';
import {
  listPayments,
  type AdminPayment,
  type Page,
  type PaymentFilters,
  type PaymentStatus,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

const STATUS_OPTIONS: ReadonlyArray<PaymentStatus> = [
  'captured',
  'failed',
  'refunded',
  'partially_refunded',
  'cancelled',
];

const STATUS_BADGE: Record<PaymentStatus, string> = {
  intent: 'bg-neutral-800 text-neutral-300',
  processing: 'bg-blue-900/40 text-blue-300',
  captured: 'bg-green-900/40 text-green-300',
  partially_refunded: 'bg-amber-900/40 text-amber-300',
  refunded: 'bg-amber-900/40 text-amber-300',
  failed: 'bg-red-900/40 text-red-300',
  cancelling: 'bg-neutral-800 text-neutral-300',
  cancelled: 'bg-neutral-800 text-neutral-500',
};

/** GET /v1/admin/payments?filters — payment operations table. */
export default function PaymentsPage() {
  const [status, setStatus] = useState<PaymentStatus | ''>('');
  const [clubId, setClubId] = useState('');

  const filters: PaymentFilters = {};
  if (status !== '') filters.status = status;
  if (clubId.trim() !== '') filters.clubId = clubId.trim();

  const fetchPayments = useCallback(
    () => listPayments(filters),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [status, clubId],
  );
  const { data, loading, error, reload } = useAsyncData<Page<AdminPayment>>(
    fetchPayments,
  );

  return (
    <div>
      <h1 className="page-title">Payments</h1>
      <p className="page-subtitle">
        All guest payments with their lifecycle status. Split details per
        payment (`/payments/:id/splits`) arrive with the F5 backend.
      </p>

      <div className="mt-6 flex flex-wrap items-end gap-4">
        <div>
          <label htmlFor="payment-status" className="field-label">
            Status
          </label>
          <select
            id="payment-status"
            className="field-input"
            value={status}
            onChange={(e) => setStatus(e.target.value as PaymentStatus | '')}
          >
            <option value="">All</option>
            {STATUS_OPTIONS.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label htmlFor="payment-club" className="field-label">
            Club ID
          </label>
          <input
            id="payment-club"
            type="text"
            placeholder="UUID"
            className="field-input"
            value={clubId}
            onChange={(e) => setClubId(e.target.value)}
          />
        </div>
      </div>

      <div className="mt-6">
        {loading ? <Loading label="Loading payments…" /> : null}
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
              No payments match the current filters.
            </p>
          ) : (
            <div className="card overflow-x-auto p-0">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-neutral-800 text-left text-xs uppercase tracking-wider text-neutral-500">
                    <th className="py-3 px-6 font-medium">Payment</th>
                    <th className="py-3 px-4 font-medium">Amount</th>
                    <th className="py-3 px-4 font-medium">Status</th>
                    <th className="py-3 px-4 font-medium">Refunded</th>
                    <th className="py-3 px-6 font-medium">Created</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((payment) => (
                    <tr
                      key={payment.id}
                      className="border-b border-neutral-800/60"
                    >
                      <td className="py-3 px-6 font-mono text-xs text-neutral-400">
                        {payment.id}
                      </td>
                      <td className="py-3 px-4 font-medium text-neutral-100">
                        {(payment.amountMinor / 100).toFixed(2)}{' '}
                        {payment.currency}
                      </td>
                      <td className="py-3 px-4">
                        <span
                          className={`rounded-full px-2.5 py-0.5 text-xs font-semibold uppercase tracking-wider ${
                            STATUS_BADGE[payment.status]
                          }`}
                        >
                          {payment.status}
                        </span>
                      </td>
                      <td className="py-3 px-4 text-neutral-400">
                        {payment.refundedMinor > 0
                          ? `${(payment.refundedMinor / 100).toFixed(2)} ${payment.currency}`
                          : '—'}
                      </td>
                      <td className="py-3 px-6 text-neutral-400">
                        {new Date(payment.createdAt).toLocaleString()}
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
