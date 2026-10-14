import { useCallback, useState, type FormEvent } from 'react';
import {
  ApiRequestError,
  createPricing,
  getPricing,
  NotImplementedStubError,
  type PricingPolicy,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

/** Platform limits per Q2 (1–10 EUR) — validated again on the API side. */
const MIN_PRICE_MINOR = 100;
const MAX_PRICE_MINOR = 1000;

function formatMinor(minor: number): string {
  return `${(minor / 100).toFixed(2)} EUR`;
}

function validatePriceMinor(value: string): string | null {
  const minor = Number(value);
  if (!Number.isInteger(minor) || minor <= 0) {
    return 'Enter a price in cents, e.g. 300 for 3.00 EUR';
  }
  if (minor < MIN_PRICE_MINOR) {
    return 'Minimum price is 1.00 EUR';
  }
  if (minor > MAX_PRICE_MINOR) {
    return 'Maximum price is 10.00 EUR';
  }
  return null;
}

/**
 * Pricing page — "backend not implemented yet": GET/POST /v1/club/pricing
 * have no backend routes yet (pricing_policy reference data lands with F3+).
 * The client methods reject locally with a 501 stub; the page renders a
 * waiting-for-backend empty state.
 */
export default function PricingPage() {
  const fetchPricing = useCallback(() => getPricing(), []);
  const { data, loading, error, reload } = useAsyncData<{ items: PricingPolicy[] }>(
    fetchPricing,
  );

  const [priceMinor, setPriceMinor] = useState('');
  const [validation, setValidation] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSubmitError(null);

    const priceError = validatePriceMinor(priceMinor);
    setValidation(priceError);
    if (priceError) return;

    setSubmitting(true);
    try {
      // djShareBp: null = platform default share (Q1: 30% commission,
      // remainder 50/50 club/DJ) — per-club override UI lands with F7 design.
      await createPricing(
        { priceMinor: Number(priceMinor), djShareBp: null },
        crypto.randomUUID(),
      );
      setPriceMinor('');
      reload();
    } catch (err) {
      setSubmitError(
        err instanceof ApiRequestError || err instanceof NotImplementedStubError
          ? err.message
          : 'Could not save pricing',
      );
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div>
      <h1 className="page-title">Pricing</h1>
      <p className="page-subtitle">
        Set the price guests pay per request. Changes apply to new requests
        only — paid requests keep the price they were created with.
      </p>

      {loading ? <Loading label="Loading pricing history…" /> : null}
      {error ? (
        error.notImplemented ? (
          <p
            className="mt-6 rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
            role="status"
          >
            Pricing is waiting for the backend —{' '}
            <code>GET/POST /v1/club/pricing</code> are not implemented yet
            (F3+). The form below is a local 501 stub until then.
          </p>
        ) : (
          <ErrorState message={error.message} code={error.code} onRetry={reload} />
        )
      ) : null}

      <div className="mt-6 grid gap-6 lg:grid-cols-2">
        <section className="card">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
            Current price
          </h2>
          <p className="mt-4 text-sm text-neutral-400">
            Unknown until the pricing backend lands.
          </p>

          <form onSubmit={onSubmit} className="mt-6 space-y-4" noValidate>
            <div>
              <label htmlFor="price-minor" className="field-label">
                New price (in cents)
              </label>
              <input
                id="price-minor"
                type="number"
                inputMode="numeric"
                min={MIN_PRICE_MINOR}
                max={MAX_PRICE_MINOR}
                step={1}
                placeholder="300"
                className="field-input"
                value={priceMinor}
                onChange={(e) => setPriceMinor(e.target.value)}
                aria-describedby="price-hint"
              />
              <p id="price-hint" className="mt-1 text-xs text-neutral-500">
                Platform limits: {formatMinor(MIN_PRICE_MINOR)} –{' '}
                {formatMinor(MAX_PRICE_MINOR)}. Example: 300 = 3.00 EUR.
              </p>
            </div>

            {validation ? (
              <p className="text-sm text-red-300" role="alert">
                {validation}
              </p>
            ) : null}
            {submitError ? (
              <p className="text-sm text-red-300" role="alert">
                {submitError}
              </p>
            ) : null}

            <button
              type="submit"
              className="btn-primary"
              disabled={submitting || priceMinor === ''}
            >
              {submitting ? 'Saving…' : 'Set new price'}
            </button>
          </form>
        </section>

        <section className="card">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
            History
          </h2>
          <div className="mt-4">
            {data && data.items.length > 0 ? (
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-neutral-800 text-left text-xs uppercase tracking-wider text-neutral-500">
                    <th className="py-2 pr-4 font-medium">Price</th>
                    <th className="py-2 pr-4 font-medium">Valid from</th>
                    <th className="py-2 pr-4 font-medium">Valid to</th>
                    <th className="py-2 font-medium">DJ share</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((policy) => (
                    <tr
                      key={policy.id}
                      className="border-b border-neutral-800/60"
                    >
                      <td className="py-2.5 pr-4 font-medium text-neutral-100">
                        {formatMinor(policy.priceMinor)}
                      </td>
                      <td className="py-2.5 pr-4 text-neutral-400">
                        {new Date(policy.validFrom).toLocaleDateString()}
                      </td>
                      <td className="py-2.5 pr-4 text-neutral-400">
                        {policy.validTo
                          ? new Date(policy.validTo).toLocaleDateString()
                          : 'active'}
                      </td>
                      <td className="py-2.5 text-neutral-400">
                        {policy.djShareBp === null
                          ? 'platform default'
                          : `${(policy.djShareBp / 100).toFixed(1)} %`}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <p className="text-sm text-neutral-400">
                No pricing policies yet — waiting for the backend.
              </p>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
