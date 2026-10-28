import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import {
  ApiRequestError,
  createPricing,
  getPricing,
  NotImplementedStubError,
  type PricingPolicy,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import { useLocale, type Translate } from '../locale';
import PageHeading from '../components/PageHeading';
import {
  EmptyState,
  ErrorState,
  ListRow,
  Metric,
  Panel,
  SkeletonMetric,
  Skeleton,
  Tile,
  ToastRegion,
  useToasts,
} from '../components/club';

/** Platform limits per Q2 (1–10 EUR) — validated again on the API side. */
const MIN_PRICE_MINOR = 100;
const MAX_PRICE_MINOR = 1000;

const LABEL_CLASS = 'mb-1 block text-[length:var(--fs-12)] font-medium text-[color:var(--c-text-2)]';
const NOTE_CLASS = 'text-[length:var(--fs-12)] leading-[1.5] text-[color:var(--c-text-2)]';
const HINT_CLASS = 'text-[length:var(--fs-11)] leading-[1.45] text-[color:var(--c-text-muted-sm)]';
const ERROR_CLASS = 'text-[length:var(--fs-13)] leading-[1.5] text-[color:var(--c-error)]';

/**
 * Formatters follow the language the user picked, not a hardcoded tag: a
 * Spanish panel reads `3,00 €` and `15,0 %`, an English one `€3.00` and `15.0%`.
 * They are cached per locale because the history list formats a row per policy.
 */
function cached<T>(cache: Map<string, T>, locale: string, build: () => T): T {
  const hit = cache.get(locale);
  if (hit) return hit;
  const created = build();
  cache.set(locale, created);
  return created;
}

const MONEY_CACHE = new Map<string, Intl.NumberFormat>();
const DATE_CACHE = new Map<string, Intl.DateTimeFormat>();
const SHARE_CACHE = new Map<string, Intl.NumberFormat>();

/**
 * Money with thousands separators, never `toFixed` (§4.6, F15).
 *
 * Returns null for a missing or non-finite amount so the caller renders the
 * honest `unavailable` word: a policy row the API returned without a readable
 * price must not print `NaN €`.
 */
function formatMoney(minor: number | null | undefined, locale: string): string | null {
  if (typeof minor !== 'number' || !Number.isFinite(minor)) return null;
  const formatter = cached(
    MONEY_CACHE,
    locale,
    () => new Intl.NumberFormat(locale, { style: 'currency', currency: 'EUR' }),
  );
  return formatter.format(minor / 100);
}

function formatDate(value: string | null | undefined, locale: string): string | null {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  const formatter = cached(
    DATE_CACHE,
    locale,
    () => new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }),
  );
  return formatter.format(date);
}

/** Basis points → `15.0%` (`15,0 %` in Spanish, `15,0 %` in German). */
function formatShare(basisPoints: number | null | undefined, locale: string): string | null {
  if (typeof basisPoints !== 'number' || !Number.isFinite(basisPoints)) return null;
  const formatter = cached(
    SHARE_CACHE,
    locale,
    () =>
      new Intl.NumberFormat(locale, {
        style: 'percent',
        minimumFractionDigits: 1,
        maximumFractionDigits: 1,
      }),
  );
  return formatter.format(basisPoints / 10000);
}

function validatePriceMinor(value: string, locale: string, t: Translate): string | null {
  const minor = Number(value);
  if (value.trim() === '' || !Number.isInteger(minor) || minor <= 0) {
    return t('pricing.invalidCents', { example: formatMoney(300, locale) ?? '' });
  }
  if (minor < MIN_PRICE_MINOR) {
    return t('pricing.belowMinimum', { min: formatMoney(MIN_PRICE_MINOR, locale) ?? '' });
  }
  if (minor > MAX_PRICE_MINOR) {
    return t('pricing.aboveMaximum', { max: formatMoney(MAX_PRICE_MINOR, locale) ?? '' });
  }
  return null;
}

function describeFailure(err: unknown, action: string, t: Translate): string {
  if (err instanceof ApiRequestError) {
    const status = `HTTP ${err.status}${err.code ? ` · ${err.code}` : ''}`;
    const message = err.message.replace(/\.$/, '');
    return t('pricing.failureHttp', { action, status, message });
  }
  if (err instanceof NotImplementedStubError) {
    return t('pricing.failureStub', { action });
  }
  return t('pricing.failureNetwork', { action });
}

/**
 * Request price — the guest price policy from the club API.
 *
 * Functional contract:
 *  - F16 a price change takes an explicit confirmation that shows the OLD and
 *    the NEW price, because the current policy is closed and cannot be edited
 *    back by clicking again;
 *  - the 1–10 EUR platform range is still validated client-side;
 *  - "applies to new requests only" stays visible next to the action that
 *    changes the price, not only in the page description.
 */
export default function PricingPage() {
  const { t, locale } = useLocale();
  const { toasts, push, dismiss } = useToasts();

  const fetchPricing = useCallback(() => getPricing(), []);
  const { data, loading, error, reload } = useAsyncData<{ items: PricingPolicy[] }>(fetchPricing);

  const [priceMinor, setPriceMinor] = useState('');
  const [validation, setValidation] = useState<string | null>(null);
  const [pendingMinor, setPendingMinor] = useState<number | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // Reviewing a price change is a two-step action, so it behaves like one:
  // opening it moves focus to Cancel, closing it returns focus to the submit
  // button, and Escape dismisses it. Focus must never fall to <body> (RT-03).
  const reviewTriggerRef = useRef<HTMLButtonElement | null>(null);
  const cancelRef = useRef<HTMLButtonElement | null>(null);
  const reviewWasOpen = useRef(false);

  useEffect(() => {
    if (pendingMinor !== null) {
      reviewWasOpen.current = true;
      cancelRef.current?.focus();
    } else if (reviewWasOpen.current) {
      reviewWasOpen.current = false;
      reviewTriggerRef.current?.focus();
    }
  }, [pendingMinor]);

  useEffect(() => {
    if (pendingMinor === null) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setPendingMinor(null);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [pendingMinor]);

  const activePolicy = data?.items.find((policy) => policy.validTo === null) ?? null;
  const unavailable = Boolean(error);

  const oldPriceLabel = activePolicy
    ? formatMoney(activePolicy.priceMinor, locale) ?? t('common.unavailable')
    : t('pricing.noActivePrice');
  const newPriceLabel = pendingMinor === null ? '' : formatMoney(pendingMinor, locale) ?? '';

  const reviewPrice = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (submitting) return;
    setSubmitError(null);
    const problem = validatePriceMinor(priceMinor, locale, t);
    setValidation(problem);
    if (problem) {
      setPendingMinor(null);
      return;
    }
    setPendingMinor(Number(priceMinor));
  };

  const applyPrice = async () => {
    if (pendingMinor === null || submitting) return;
    setSubmitError(null);
    setSubmitting(true);
    try {
      await createPricing({ priceMinor: pendingMinor, djShareBp: null });
      const applied = formatMoney(pendingMinor, locale) ?? '';
      setPriceMinor('');
      setPendingMinor(null);
      setValidation(null);
      push({
        tone: 'live',
        title: t('pricing.toastChangedTitle', { price: applied }),
        message: t('pricing.toastChangedBody'),
      });
      reload();
    } catch (err) {
      const message = describeFailure(err, t('pricing.actionChangePrice'), t);
      setSubmitError(message);
      push({ tone: 'error', title: t('pricing.toastFailedTitle'), message });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="club-workspace-content">
      <PageHeading
        title={t('pricing.title')}
      />

      {error ? (
        <div className="mb-4">
          <ErrorState
            title={t('pricing.errorTitle')}
            message={`${error.message}. ${t('pricing.errorTail')}`}
            code={error.code}
            onRetry={reload}
          />
        </div>
      ) : null}

      <div className="grid items-start gap-4 lg:grid-cols-2">
        <Panel
          title={t('pricing.guestPricePanel')}
          detail={t('pricing.guestPriceDetail')}
        >
          {loading ? (
            <div role="status" aria-label={t('pricing.loadingCurrent')}>
              <SkeletonMetric />
            </div>
          ) : (
            <Metric
              label={t('pricing.currentPrice')}
              value={activePolicy ? formatMoney(activePolicy.priceMinor, locale) : null}
              source={unavailable || (activePolicy !== null && formatMoney(activePolicy.priceMinor, locale) === null) ? 'unavailable' : 'live'}
              unavailableLabel={t('pricing.noActivePrice')}
              caption={
                unavailable
                  ? t('pricing.captionFailed')
                  : activePolicy
                    ? t('pricing.captionActive', {
                        date: formatDate(activePolicy.validFrom, locale) ?? t('common.unavailable'),
                      })
                    : t('pricing.captionNoPolicy')
              }
            />
          )}

          <form onSubmit={reviewPrice} className="mt-5 grid gap-4" noValidate>
            <div>
              <label htmlFor="price-minor" className={LABEL_CLASS}>
                {t('pricing.newPriceLabel')}
              </label>
              <input
                id="price-minor"
                type="number"
                inputMode="numeric"
                min={MIN_PRICE_MINOR}
                max={MAX_PRICE_MINOR}
                step={1}
                placeholder="300"
                className="club-field"
                value={priceMinor}
                onChange={(event) => {
                  setPriceMinor(event.target.value);
                  setValidation(null);
                }}
                aria-describedby="price-hint"
                aria-invalid={validation ? true : undefined}
                disabled={submitting || Boolean(error?.notImplemented)}
              />
              <p id="price-hint" className={`mt-1 ${HINT_CLASS}`}>
                {t('pricing.priceHint', {
                  min: formatMoney(MIN_PRICE_MINOR, locale) ?? '',
                  max: formatMoney(MAX_PRICE_MINOR, locale) ?? '',
                  example: formatMoney(300, locale) ?? '',
                })}
              </p>
            </div>

            {validation ? (
              <p className={ERROR_CLASS} role="alert">
                {validation}
              </p>
            ) : null}
            {submitError ? (
              <p className={ERROR_CLASS} role="alert">
                {submitError}
              </p>
            ) : null}

            {pendingMinor !== null ? (
              <Tile tone="inset">
                <p className="text-[length:var(--fs-13)] font-semibold text-[color:var(--c-text)]">
                  {t('pricing.changeQuestion')}
                </p>
                <div className="mt-3 grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-3">
                  <div>
                    <p className={LABEL_CLASS}>{t('pricing.oldPrice')}</p>
                    <p className="font-mono text-[length:var(--fs-20)] leading-[1.2] text-[color:var(--c-text-2)]">
                      {oldPriceLabel}
                    </p>
                  </div>
                  <div>
                    <p className={LABEL_CLASS}>{t('pricing.newPrice')}</p>
                    <p className="font-mono text-[length:var(--fs-20)] leading-[1.2] text-[color:var(--c-text)]">
                      {newPriceLabel}
                    </p>
                  </div>
                </div>
                <p className={`mt-3 ${NOTE_CLASS}`}>{t('pricing.reviewNote')}</p>
                <div className="mt-3 flex flex-wrap gap-2">
                  <button
                    type="button"
                    className="club-button club-button-primary"
                    onClick={() => { void applyPrice(); }}
                    disabled={submitting}
                  >
                    {submitting ? t('common.saving') : t('pricing.apply')}
                  </button>
                  <button
                    ref={cancelRef}
                    type="button"
                    className="club-button club-button-quiet"
                    onClick={() => setPendingMinor(null)}
                    disabled={submitting}
                  >
                    {t('common.cancel')}
                  </button>
                </div>
              </Tile>
            ) : (
              <>
                <button
                  ref={reviewTriggerRef}
                  type="submit"
                  className="club-button club-button-primary"
                  disabled={submitting || priceMinor === '' || Boolean(error?.notImplemented)}
                >
                  {t('pricing.review')}
                </button>
                <p className={HINT_CLASS}>{t('pricing.reviewHint')}</p>
              </>
            )}
          </form>
        </Panel>

        <Panel title={t('pricing.historyPanel')} detail={t('pricing.historyDetail')}>
          {loading ? (
            <div className="flex flex-col gap-3" role="status" aria-label={t('pricing.loadingHistory')}>
              <Skeleton height={44} />
              <Skeleton height={44} />
              <Skeleton height={44} width="75%" />
            </div>
          ) : null}

          {error?.notImplemented ? (
            <EmptyState
              title={t('pricing.historyUnavailable')}
              detail={t('pricing.historyStub')}
            />
          ) : null}

          {unavailable && !error?.notImplemented ? (
            <EmptyState
              title={t('pricing.historyUnavailable')}
              detail={t('pricing.historyFailed')}
            />
          ) : null}

          {data && data.items.length === 0 ? (
            <EmptyState
              title={t('pricing.historyEmpty')}
              detail={t('pricing.historyEmptyDetail')}
            />
          ) : null}

          {data && data.items.length > 0 ? (
            <ul className="club-list">
              {data.items.map((policy) => (
                <ListRow
                  key={policy.id}
                  title={formatMoney(policy.priceMinor, locale) ?? t('common.unavailable')}
                  detail={`${formatDate(policy.validFrom, locale) ?? t('common.unavailable')} → ${policy.validTo ? formatDate(policy.validTo, locale) ?? t('common.unavailable') : t('pricing.activeNow')}`}
                  meta={
                    policy.djShareBp === null
                      ? t('pricing.defaultShare')
                      : t('pricing.djShare', { share: formatShare(policy.djShareBp, locale) ?? t('common.unavailable') })
                  }
                />
              ))}
            </ul>
          ) : null}
        </Panel>
      </div>

      <ToastRegion toasts={toasts} onDismiss={dismiss} />
    </div>
  );
}
