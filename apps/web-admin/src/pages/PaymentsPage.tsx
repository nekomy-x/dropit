import { useCallback, useState } from 'react';
import { listPayments, type AdminPayment, type PaymentStatus } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';

const STATUS_OPTIONS: ReadonlyArray<PaymentStatus> = [
  'captured',
  'failed',
  'refunded',
  'partially_refunded',
  'cancelled',
];

/**
 * Payments — "backend not implemented yet": GET /v1/admin/payments has no
 * backend route yet (the payment surface implemented in F5 is the guest
 * confirm flow + Stripe webhooks), so the client method rejects locally
 * with a 501 stub and the page renders a waiting-for-backend empty state.
 * The disabled filter controls below reserve the UI for the real endpoint.
 */
export default function PaymentsPage() {
  const [status] = useState<PaymentStatus | ''>('');
  const [clubId] = useState('');

  void status;
  void clubId;

  const fetchPayments = useCallback(() => listPayments(), []);
  const { data, loading, error } = useAsyncData<{ items: AdminPayment[] }>(
    fetchPayments,
  );

  return (
    <div>
      <h1 className="page-title">Payments</h1>
      <p className="page-subtitle">
        All guest payments with their lifecycle status. Split details per
        payment arrive with the payments admin endpoint.
      </p>

      <div className="mt-6">
        {loading ? (
          <p className="text-sm text-neutral-400">Loading payments…</p>
        ) : null}

        {error ? (
          error.notImplemented ? (
            <p
              className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
              role="status"
            >
              The payments table is waiting for the backend —{' '}
              <code>GET /v1/admin/payments</code> is not implemented yet
              (F5 implements the guest confirm flow and webhooks; the admin
              read endpoint is still pending). The filters below are inert
              stubs.
            </p>
          ) : (
            <p className="text-sm text-red-300" role="alert">
              {error.message}
              {error.code ? ` (${error.code})` : ''}
            </p>
          )
        ) : null}
      </div>

      {data && data.items.length > 0 ? (
        <div className="card mt-6 overflow-x-auto p-0">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-neutral-800 text-left text-xs uppercase tracking-wider text-neutral-500">
                <th className="py-3 px-6 font-medium">Payment</th>
                <th className="py-3 px-4 font-medium">Amount</th>
                <th className="py-3 px-4 font-medium">Status</th>
                <th className="py-3 px-6 font-medium">Created</th>
              </tr>
            </thead>
            <tbody>
              {data.items.map((payment) => (
                <tr key={payment.id} className="border-b border-neutral-800/60">
                  <td className="py-3 px-6 font-mono text-xs text-neutral-400">
                    {payment.id}
                  </td>
                  <td className="py-3 px-4 font-medium text-neutral-100">
                    {(payment.amountMinor / 100).toFixed(2)} {payment.currency}
                  </td>
                  <td className="py-3 px-4 text-neutral-400">{payment.status}</td>
                  <td className="py-3 px-6 text-neutral-400">
                    {new Date(payment.createdAt).toLocaleString()}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}

      <div className="mt-6 flex flex-wrap items-end gap-4 opacity-50" aria-hidden="true">
        <div>
          <label htmlFor="payment-status" className="field-label">
            Status
          </label>
          <select
            id="payment-status"
            className="field-input"
            value={status}
            disabled
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
            disabled
            readOnly
          />
        </div>
      </div>
    </div>
  );
}
