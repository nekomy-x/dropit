import { useCallback, useState, type FormEvent } from 'react';
import {
  ApiRequestError,
  getPlatformSettings,
  patchPlatformSettings,
  type PlatformSettings,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

/** All share/commission fields are basis points: 0..10000 (10000 = 100%). */
const BP_MAX = 10000;

interface FormState {
  commissionMode: 'percent' | 'fixed';
  commissionPercentBp: string;
  commissionFixedMinor: string;
  minPriceMinor: string;
  maxPriceMinor: string;
  djShareOfRemainderBp: string;
}

function toForm(settings: PlatformSettings): FormState {
  return {
    commissionMode: settings.commissionMode,
    commissionPercentBp: String(settings.commissionPercentBp),
    commissionFixedMinor: String(settings.commissionFixedMinor),
    minPriceMinor: String(settings.minPriceMinor),
    maxPriceMinor: String(settings.maxPriceMinor),
    djShareOfRemainderBp: String(settings.djShareOfRemainderBp),
  };
}

/**
 * Client-side validation (mirrors the API contract):
 * - bp fields must be integers in 0..10000
 * - all minor-unit fields must be non-negative integers
 * - min price < max price
 * - in percent mode the percent must be set; in fixed mode the fixed amount
 */
function validate(form: FormState): Record<string, string> {
  const errors: Record<string, string> = {};

  const bp = (value: string): number | null => {
    const n = Number(value);
    if (!Number.isInteger(n) || n < 0 || n > BP_MAX) return null;
    return n;
  };

  const minor = (value: string): number | null => {
    const n = Number(value);
    if (!Number.isInteger(n) || n < 0) return null;
    return n;
  };

  if (form.commissionMode === 'percent') {
    if (bp(form.commissionPercentBp) === null) {
      errors.commissionPercentBp =
        'Commission must be an integer in 0..10000 basis points (e.g. 3000 = 30%).';
    }
  } else {
    if (minor(form.commissionFixedMinor) === null || form.commissionFixedMinor === '0') {
      errors.commissionFixedMinor =
        'Fixed commission must be a positive integer in cents (e.g. 50 = 0.50 EUR).';
    }
  }

  if (bp(form.djShareOfRemainderBp) === null) {
    errors.djShareOfRemainderBp =
      'DJ share must be an integer in 0..10000 basis points (5000 = 50%).';
  }

  const minPrice = minor(form.minPriceMinor);
  const maxPrice = minor(form.maxPriceMinor);
  if (minPrice === null) {
    errors.minPriceMinor = 'Minimum price must be a non-negative integer in cents.';
  }
  if (maxPrice === null) {
    errors.maxPriceMinor = 'Maximum price must be a non-negative integer in cents.';
  }
  if (minPrice !== null && maxPrice !== null && minPrice >= maxPrice) {
    errors.maxPriceMinor = 'Minimum price must be strictly less than the maximum.';
  }

  return errors;
}

/** GET/PATCH /v1/admin/platform-settings — commission, price limits, DJ share. */
export default function PlatformSettingsPage() {
  const fetchSettings = useCallback(() => getPlatformSettings(), []);
  const { data, loading, error, reload } =
    useAsyncData<PlatformSettings>(fetchSettings);

  const [form, setForm] = useState<FormState | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // Seed the form once data arrives (and has not been edited yet).
  if (data && form === null) {
    setForm(toForm(data));
  }

  const update = (field: keyof FormState, value: string) => {
    if (!form) return;
    setForm({ ...form, [field]: value });
    setSaved(false);
  };

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!form) return;
    setSubmitError(null);
    setSaved(false);

    const validation = validate(form);
    setErrors(validation);
    if (Object.keys(validation).length > 0) return;

    setSubmitting(true);
    try {
      await patchPlatformSettings({
        commissionMode: form.commissionMode,
        commissionPercentBp: Number(form.commissionPercentBp),
        commissionFixedMinor: Number(form.commissionFixedMinor),
        minPriceMinor: Number(form.minPriceMinor),
        maxPriceMinor: Number(form.maxPriceMinor),
        djShareOfRemainderBp: Number(form.djShareOfRemainderBp),
      });
      setSaved(true);
      reload();
    } catch (err) {
      setSubmitError(
        err instanceof ApiRequestError
          ? err.message
          : 'Could not save settings',
      );
    } finally {
      setSubmitting(false);
    }
  };

  const fieldError = (field: keyof FormState) =>
    errors[field] ? (
      <p className="mt-1 text-sm text-red-300" role="alert">
        {errors[field]}
      </p>
    ) : null;

  return (
    <div>
      <h1 className="page-title">Platform settings</h1>
      <p className="page-subtitle">
        Commission, price limits and DJ revenue share. Changes apply to new
        requests only; every payment keeps an immutable snapshot of the rules
        it was charged under.
      </p>

      <div className="mt-6">
        {loading ? <Loading label="Loading platform settings…" /> : null}
        {error ? (
          <ErrorState
            message={error.message}
            code={error.code}
            onRetry={reload}
          />
        ) : null}

        {form ? (
          <form onSubmit={onSubmit} className="card max-w-2xl" noValidate>
            <fieldset className="mb-6">
              <legend className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Commission
              </legend>

              <div className="mt-4 flex gap-4" role="radiogroup" aria-label="Commission mode">
                <label className="flex items-center gap-2 text-sm text-neutral-200">
                  <input
                    type="radio"
                    name="commission-mode"
                    value="percent"
                    checked={form.commissionMode === 'percent'}
                    onChange={() => update('commissionMode', 'percent')}
                  />
                  Percentage
                </label>
                <label className="flex items-center gap-2 text-sm text-neutral-200">
                  <input
                    type="radio"
                    name="commission-mode"
                    value="fixed"
                    checked={form.commissionMode === 'fixed'}
                    onChange={() => update('commissionMode', 'fixed')}
                  />
                  Fixed amount
                </label>
              </div>

              {form.commissionMode === 'percent' ? (
                <div className="mt-4">
                  <label htmlFor="commission-percent-bp" className="field-label">
                    Commission (basis points, 10000 = 100%)
                  </label>
                  <input
                    id="commission-percent-bp"
                    type="number"
                    min={0}
                    max={BP_MAX}
                    step={1}
                    className="field-input"
                    value={form.commissionPercentBp}
                    onChange={(e) => update('commissionPercentBp', e.target.value)}
                  />
                  <p className="mt-1 text-xs text-neutral-500">
                    Current default: 3000 bp = 30% per request.
                  </p>
                  {fieldError('commissionPercentBp')}
                </div>
              ) : (
                <div className="mt-4">
                  <label htmlFor="commission-fixed-minor" className="field-label">
                    Fixed commission (cents)
                  </label>
                  <input
                    id="commission-fixed-minor"
                    type="number"
                    min={0}
                    step={1}
                    className="field-input"
                    value={form.commissionFixedMinor}
                    onChange={(e) => update('commissionFixedMinor', e.target.value)}
                  />
                  <p className="mt-1 text-xs text-neutral-500">
                    Example: 50 = 0.50 EUR per request.
                  </p>
                  {fieldError('commissionFixedMinor')}
                </div>
              )}
            </fieldset>

            <fieldset className="mb-6">
              <legend className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Price limits
              </legend>
              <div className="mt-4 grid gap-4 sm:grid-cols-2">
                <div>
                  <label htmlFor="min-price-minor" className="field-label">
                    Minimum price (cents)
                  </label>
                  <input
                    id="min-price-minor"
                    type="number"
                    min={0}
                    step={1}
                    className="field-input"
                    value={form.minPriceMinor}
                    onChange={(e) => update('minPriceMinor', e.target.value)}
                  />
                  <p className="mt-1 text-xs text-neutral-500">
                    Default: 100 = 1.00 EUR.
                  </p>
                  {fieldError('minPriceMinor')}
                </div>
                <div>
                  <label htmlFor="max-price-minor" className="field-label">
                    Maximum price (cents)
                  </label>
                  <input
                    id="max-price-minor"
                    type="number"
                    min={0}
                    step={1}
                    className="field-input"
                    value={form.maxPriceMinor}
                    onChange={(e) => update('maxPriceMinor', e.target.value)}
                  />
                  <p className="mt-1 text-xs text-neutral-500">
                    Default: 1000 = 10.00 EUR.
                  </p>
                  {fieldError('maxPriceMinor')}
                </div>
              </div>
            </fieldset>

            <fieldset className="mb-6">
              <legend className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Revenue split
              </legend>
              <div className="mt-4">
                <label htmlFor="dj-share-bp" className="field-label">
                  DJ share of the remainder (basis points, 10000 = 100%)
                </label>
                <input
                  id="dj-share-bp"
                  type="number"
                  min={0}
                  max={BP_MAX}
                  step={1}
                  className="field-input"
                  value={form.djShareOfRemainderBp}
                  onChange={(e) => update('djShareOfRemainderBp', e.target.value)}
                />
                <p className="mt-1 text-xs text-neutral-500">
                  After the platform commission, the remainder is split between
                  club and DJ. Default: 5000 bp = 50/50.
                </p>
                {fieldError('djShareOfRemainderBp')}
              </div>
            </fieldset>

            {submitError ? (
              <p className="mb-4 text-sm text-red-300" role="alert">
                {submitError}
              </p>
            ) : null}
            {saved ? (
              <p className="mb-4 text-sm text-green-300" role="status">
                Settings saved. The change is recorded in the audit log.
              </p>
            ) : null}

            <button type="submit" className="btn-primary" disabled={submitting}>
              {submitting ? 'Saving…' : 'Save settings'}
            </button>
          </form>
        ) : null}
      </div>
    </div>
  );
}
