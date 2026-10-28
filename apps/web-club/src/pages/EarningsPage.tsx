import { useCallback } from 'react';
import { useLocale } from '../locale';
import { getEarnings, type EarningsSummary } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import PageHeading from '../components/PageHeading';
import {
  EmptyState,
  ErrorState,
  Metric,
  Panel,
  Skeleton,
  Tile,
} from '../components/club';

const HINT_CLASS = 'text-[length:var(--fs-11)] leading-[1.45] text-[color:var(--c-text-muted-sm)]';

/** Money with thousands separators and the active locale, never `toFixed` (§4.6, F15). */
function formatMoney(minor: number, currency: string, locale: string): string {
  return new Intl.NumberFormat(locale, { style: 'currency', currency }).format(minor / 100);
}

/**
 * Earnings — the club settlement numbers from `GET /v1/club/earnings`.
 *
 * Functional contract (F15), verified against the SQL in
 * `crates/gateway-api/src/panel_routes.rs` `club_earnings`:
 *  - the `today` period is `p.captured_at > now() - interval '1 day'`, i.e. a
 *    ROLLING 24 hours, not a calendar day. The page says so instead of "today";
 *  - `net_minor` is the sum of `split` rows with `payee_type = 'club'` — the
 *    club's own share after the platform commission, with the DJ share in a
 *    separate split row and therefore NOT included. The label says "club share",
 *    not "net revenue";
 *  - `gross_minor` is the sum of captured `payment.amount_minor` over the same
 *    window, and `payments` is the count of those captured payments;
 *  - money is formatted with Intl (thousands separators), and there is an empty
 *    state when no payment was captured in the window.
 */
export default function EarningsPage() {
  const { t, locale } = useLocale();
  const fetchEarnings = useCallback(() => getEarnings('today'), []);
  const { data, loading, error, reload } = useAsyncData<EarningsSummary>(fetchEarnings);

  const unavailable = Boolean(error);
  const currency = data?.currency ?? 'EUR';
  const hasPayments = Boolean(data && data.requestsCount > 0);

  return (
    <div className="club-workspace-content">
      <PageHeading
        title={t('earnings.title')}
      />

      {error ? (
        <div className="mb-4">
          <ErrorState
            title={t('earnings.errorTitle')}
            message={`${error.message} ${t('earnings.errorTail')}`}
            code={error.code}
            onRetry={reload}
          />
        </div>
      ) : null}

      <Panel
        title={t('earnings.periodDay')}
        detail={t('earnings.windowNote')}
      >
        {loading ? (
          <div
            className="grid gap-3 sm:grid-cols-3"
            role="status"
            aria-label={t('earnings.loading')}
          >
            {[0, 1, 2].map((index) => (
              <Tile key={index} tone="inset">
                <div className="flex flex-col gap-2">
                  <Skeleton height={12} width="55%" />
                  <Skeleton height={28} width="70%" />
                </div>
              </Tile>
            ))}
          </div>
        ) : null}

        {!loading && data && data.requestsCount === 0 ? (
          <EmptyState
            title={t('earnings.emptyTitle')}
            detail={t('earnings.emptyBody')}
          />
        ) : null}

        {!loading && (hasPayments || unavailable) ? (
          <div className="grid gap-3 sm:grid-cols-3">
            <Tile tone="inset">
              <Metric
                label={t('earnings.net')}
                value={hasPayments && data ? formatMoney(data.netMinor, currency, locale) : null}
                source={unavailable ? 'unavailable' : 'live'}
                caption={t('earnings.netCaption')}
              />
            </Tile>
            <Tile tone="inset">
              <Metric
                label={t('earnings.gross')}
                value={hasPayments && data ? formatMoney(data.grossMinor, currency, locale) : null}
                source={unavailable ? 'unavailable' : 'live'}
                caption={t('earnings.grossCaption')}
              />
            </Tile>
            <Tile tone="inset">
              <Metric
                label={t('earnings.payments')}
                value={hasPayments && data ? new Intl.NumberFormat(locale).format(data.requestsCount) : null}
                source={unavailable ? 'unavailable' : 'live'}
                caption={t('earnings.paymentsCaption')}
              />
            </Tile>
          </div>
        ) : null}

        <p className={`mt-4 ${HINT_CLASS}`}>
          {t('earnings.windowLabel')} <span className="font-mono">now() − 1 day</span>{' '}
          {t('earnings.windowTail')} <span className="font-mono">GET /v1/club/earnings?period=today</span>.
        </p>
      </Panel>
    </div>
  );
}