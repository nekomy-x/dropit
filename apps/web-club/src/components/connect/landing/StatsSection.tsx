import Reveal from '../Reveal';
import SectionHead from './SectionHead';
import { useLocale } from '../../../connect/locale';
import { TrendChart } from './charts';
import {
  DEFAULT_PRICE_MINOR,
  PREVIOUS_WEEK_REQUESTS,
  SAMPLE_WEEK_REQUESTS,
  SERVICE_FEE_MINOR,
  TOP_TRACKS,
  clubShareMinor,
  sumMinor,
  weekdayLabels,
} from './scenario';

const SHARE_MINOR = clubShareMinor(DEFAULT_PRICE_MINOR);
const CURRENT_REVENUE = SAMPLE_WEEK_REQUESTS.map((requests) => requests * SHARE_MINOR);
const PREVIOUS_REVENUE = PREVIOUS_WEEK_REQUESTS.map((requests) => requests * SHARE_MINOR);
const WEEK_TOTAL = sumMinor(CURRENT_REVENUE);
const REQUEST_TOTAL = SAMPLE_WEEK_REQUESTS.reduce((total, requests) => total + requests, 0);
const GUEST_SPEND = REQUEST_TOTAL * DEFAULT_PRICE_MINOR;
const SERVICE_TOTAL = REQUEST_TOTAL * SERVICE_FEE_MINOR;
const TOP_COUNT = TOP_TRACKS[0]?.requests ?? 1;

function DayLegend({ tone, label }: { tone: 'current' | 'previous'; label: string }) {
  return (
    <span className="flex items-center gap-1.5 text-[11px] text-drop-muted">
      <span
        className={`h-1.5 w-1.5 rounded-full ${
          tone === 'current' ? 'bg-drop-accent' : 'bg-drop-muted'
        }`}
        aria-hidden="true"
      />
      {label}
    </span>
  );
}

export default function StatsSection() {
  const { t, language, locale } = useLocale();
  const days = weekdayLabels(language);
  const currentPoints = days.map((label, index) => ({
    label,
    value: CURRENT_REVENUE[index] ?? 0,
  }));

  const int = (value: number) => Math.round(value).toLocaleString(language);
  const money = (minor: number) =>
    new Intl.NumberFormat(locale, {
      style: 'currency',
      currency: 'EUR',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(minor / 100);

  const chartLabel = [
    `${t('chartTitle')}.`,
    `${t('statsLegendCurrent')}: ${days
      .map((day, index) => `${day} ${money(CURRENT_REVENUE[index] ?? 0)}`)
      .join(', ')}.`,
    `${t('statsLegendPrevious')}: ${days
      .map((day, index) => `${day} ${money(PREVIOUS_REVENUE[index] ?? 0)}`)
      .join(', ')}.`,
  ].join(' ');

  return (
    <section id="guest" className="border-t border-drop-line bg-drop-bg">
      <div className="mx-auto w-full max-w-[1440px] px-6 py-16 lg:px-12 lg:py-24">
        <SectionHead eyebrow={t('statsEyebrow')} headline={t('statsTitle')} note={t('statsNote')} />

        <div className="mt-14 grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1.35fr)_minmax(0,1fr)]">
          <Reveal className="h-full">
            <div className="connect-stats-group flex h-full flex-col rounded-card border border-drop-line bg-drop-surface p-6">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h3 className="text-[15px] font-semibold text-drop-text">{t('chartTitle')}</h3>
                  <p className="mt-1 text-[11px] text-drop-muted">
                    {t('statsScenario', { price: money(DEFAULT_PRICE_MINOR) })}
                  </p>
                </div>
                <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
                  <DayLegend tone="current" label={t('statsLegendCurrent')} />
                  <DayLegend tone="previous" label={t('statsLegendPrevious')} />
                </div>
              </div>

              <div className="mt-3">
                <TrendChart
                  points={currentPoints}
                  previous={PREVIOUS_REVENUE}
                  seriesLabels={{
                    current: t('statsLegendCurrent'),
                    previous: t('statsLegendPrevious'),
                  }}
                  height={280}
                  valueFormat={(value) => int(value / 100)}
                  axisFormat={(value) => int(value / 100)}
                  tooltipFormat={money}
                  ariaLabel={chartLabel}
                />
              </div>

              <dl className="mt-auto flex flex-col gap-3 border-t border-drop-line pt-4 sm:grid sm:grid-cols-3 sm:gap-4">
                <div className="flex items-baseline justify-between gap-3 sm:block">
                  <dt className="text-[11px] text-drop-muted">{t('statsWeekTotal')}</dt>
                  <dd className="text-[15px] font-medium text-drop-text tabular sm:mt-1">
                    {money(WEEK_TOTAL)}
                  </dd>
                </div>
                <div className="flex items-baseline justify-between gap-3 sm:block">
                  <dt className="text-[11px] text-drop-muted">{t('statsPayments')}</dt>
                  <dd className="text-[15px] font-medium text-drop-text tabular sm:mt-1">
                    {int(REQUEST_TOTAL)}
                  </dd>
                </div>
                <div className="flex items-baseline justify-between gap-3 sm:block">
                  <dt className="text-[11px] text-drop-muted">{t('statsAvgNight')}</dt>
                  <dd className="text-[15px] font-medium text-drop-text tabular sm:mt-1">
                    {money(WEEK_TOTAL / days.length)}
                  </dd>
                </div>
              </dl>
            </div>
          </Reveal>

          <Reveal delay={80} className="h-full">
            <div className="connect-stats-group flex h-full flex-col rounded-card border border-drop-line bg-drop-surface p-6">
              <h3 className="text-[15px] font-semibold text-drop-text">{t('topRequests')}</h3>

              <ul className="mt-2 flex flex-1 flex-col">
                {TOP_TRACKS.map((track, index) => (
                  <li
                    key={track.title}
                    className="flex flex-1 flex-col justify-center border-t border-drop-line py-2.5 first:border-t-0"
                  >
                    <div className="flex items-baseline gap-3">
                      <span className="w-4 shrink-0 font-mono text-[11px] text-drop-muted tabular">
                        {String(index + 1).padStart(2, '0')}
                      </span>
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-[13px] text-drop-text">{track.title}</p>
                        <p className="truncate text-[11px] text-drop-muted">{track.artist}</p>
                      </div>
                      <span className="text-[13px] font-medium text-drop-text tabular">
                        {int(track.requests)}
                      </span>
                    </div>
                    <div className="ml-7 mt-2 h-1.5 overflow-hidden rounded-full bg-drop-surface2">
                      <div
                        className="h-full rounded-full bg-drop-accent"
                        style={{ width: `${(track.requests / TOP_COUNT) * 100}%` }}
                      />
                    </div>
                  </li>
                ))}
              </ul>

              <p className="border-t border-drop-line pt-3 text-[11px] text-drop-muted">
                {t('topRequestsMeta', { requests: int(REQUEST_TOTAL) })}
              </p>
            </div>
          </Reveal>

          <Reveal delay={140} className="lg:col-span-2">
            <div className="connect-stats-group rounded-card border border-drop-line bg-drop-surface p-6">
              <div className="grid gap-6 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.2fr)] lg:items-center lg:gap-10">
                <div>
                  <p className="text-[11px] text-drop-muted">{t('payoutNext')}</p>
                  <p className="mt-2 font-display text-[34px] leading-none text-drop-text tabular">
                    {money(WEEK_TOTAL)}
                  </p>
                  <p className="mt-2 max-w-[340px] text-[11px] leading-relaxed text-drop-muted">
                    {t('payoutDescription', {
                      payments: int(REQUEST_TOTAL),
                      share: money(SHARE_MINOR),
                      fee: money(SERVICE_FEE_MINOR),
                    })}
                  </p>
                </div>

                <dl className="grid grid-cols-1 gap-3 sm:grid-cols-3 sm:gap-4">
                  <div>
                    <dt className="text-[11px] text-drop-muted">{t('payoutGuestSpend')}</dt>
                    <dd className="mt-1 text-[15px] font-medium text-drop-text tabular">
                      {money(GUEST_SPEND)}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-[11px] text-drop-muted">{t('payoutServiceFee')}</dt>
                    <dd className="mt-1 text-[15px] font-medium text-drop-muted tabular">
                      {money(SERVICE_TOTAL)}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-[11px] text-drop-muted">{t('payoutClubNet')}</dt>
                    <dd className="mt-1 text-[15px] font-medium text-drop-text tabular">
                      {money(WEEK_TOTAL)}
                    </dd>
                  </div>
                </dl>
              </div>
            </div>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
