import { useMemo, useState, type ReactNode } from 'react';
import Icon from '../Icons';
import { useLocale } from '../../../connect/locale';
import { TrendChart, type TrendPoint } from './charts';
import {
  DEFAULT_NIGHTS,
  DEFAULT_PRICE_MINOR,
  DEFAULT_SCANS,
  WEEKDAY_SHARE,
  clubShareMinor,
  weekdayLabels,
} from './scenario';

const PRICE_PRESETS = [100, 200, 300, 500];

function StepButton({
  icon,
  onClick,
  label,
  disabled,
}: {
  icon: 'plus' | 'minus';
  onClick: () => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      className="flex h-8 w-8 items-center justify-center rounded-lg border border-drop-line text-drop-muted transition-colors duration-200 hover:border-drop-accent/50 hover:text-drop-accent active:scale-95 disabled:cursor-not-allowed disabled:opacity-35 disabled:hover:border-drop-line disabled:hover:text-drop-muted"
    >
      <Icon name={icon} size={13} strokeWidth={2.2} />
    </button>
  );
}

function ControlRow({
  label,
  sub,
  children,
}: {
  label: string;
  sub: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 border-t border-drop-line py-3">
      <div className="min-w-0">
        <p className="text-[13px] text-drop-text">{label}</p>
        <p className="mt-0.5 text-[11px] text-drop-muted">{sub}</p>
      </div>
      {children}
    </div>
  );
}

export default function CalculatorCard() {
  const { t, language, locale } = useLocale();
  const [priceMinor, setPriceMinor] = useState(DEFAULT_PRICE_MINOR);
  const [scans, setScans] = useState(DEFAULT_SCANS);
  const [nights, setNights] = useState(DEFAULT_NIGHTS);

  const clubPerRequest = clubShareMinor(priceMinor);
  const clubPerMonth = clubPerRequest * scans * nights;
  const requestsPerMonth = scans * nights;
  const clubPerNight = clubPerRequest * scans;
  const guestTurnover = priceMinor * scans * nights;

  // No tweening: a number that animates passes through values the model never
  // produced, and a reader who looks mid-transition reads a wrong figure.
  const int = (value: number) => Math.round(value).toLocaleString(language);
  const money = (minor: number) =>
    new Intl.NumberFormat(locale, {
      style: 'currency',
      currency: 'EUR',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(minor / 100);
  const euro = (minor: number) =>
    new Intl.NumberFormat(locale, {
      style: 'currency',
      currency: 'EUR',
      minimumFractionDigits: 0,
      maximumFractionDigits: 2,
    }).format(minor / 100);

  const days = weekdayLabels(language);

  const points: TrendPoint[] = useMemo(
    () =>
      days.map((label, index) => {
        const requests = Math.round((WEEKDAY_SHARE[index] ?? 0) * scans);
        return {
          label,
          value: requests * clubPerRequest,
          note: `${requests.toLocaleString(language)} × ${money(clubPerRequest)}`,
        };
      }),
    [days, scans, clubPerRequest, language],
  );

  // Mean of the bars actually plotted, not of the model: at scans=10 the
  // rounded weekday series averages 17,71 € while the control says 17,00 €.
  const chartMean = points.reduce((total, point) => total + point.value, 0) / points.length;
  const chartLabel = `${t('calcChartTitle')}: ${points
    .map((point) => `${point.label} ${money(point.value)}`)
    .join(', ')}`;

  return (
    <div className="connect-calculator flex flex-1 flex-col gap-6 rounded-card border border-drop-line bg-drop-surface p-6 sm:p-7">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <h3 className="text-[16px] font-semibold text-drop-text">{t('calcTitle')}</h3>
        <span className="rounded-full border border-drop-line px-3 py-1 text-[11px] text-drop-muted">
          {t('calcScenario')}
        </span>
      </header>

      <div className="grid grid-cols-1 gap-7 lg:grid-cols-[minmax(0,300px)_minmax(0,1fr)] lg:gap-8">
        <div className="min-w-0">
          <ControlRow label={t('calcGuestPrice')} sub={t('calcClubSets')}>
            <div className="flex items-center gap-2">
              <StepButton
                icon="minus"
                label={t('wizardDecrease')}
                onClick={() => setPriceMinor((value) => Math.max(100, value - 50))}
                disabled={priceMinor <= 100}
              />
              <span className="min-w-[74px] rounded-lg bg-drop-surface2 px-3 py-1.5 text-center text-[13px] font-semibold text-drop-text tabular">
                {money(priceMinor)}
              </span>
              <StepButton
                icon="plus"
                label={t('wizardIncrease')}
                onClick={() => setPriceMinor((value) => Math.min(5000, value + 50))}
                disabled={priceMinor >= 5000}
              />
            </div>
          </ControlRow>

          <div className="flex flex-wrap gap-1 pb-4">
            {PRICE_PRESETS.map((preset) => {
              const active = priceMinor === preset;
              return (
                <button
                  key={preset}
                  type="button"
                  onClick={() => setPriceMinor(preset)}
                  aria-pressed={active}
                  className={`h-8 min-w-[46px] rounded-lg px-2 text-[11px] tabular transition-colors duration-200 ${
                    active
                      ? 'bg-drop-accent/10 text-drop-accent ring-1 ring-drop-accent/40'
                      : 'bg-drop-surface2 text-drop-muted hover:text-drop-text'
                  }`}
                >
                  {euro(preset)}
                </button>
              );
            })}
          </div>

          <ControlRow label={t('calcScans')} sub={t('calcScansHint')}>
            <div className="flex items-center gap-2">
              <StepButton
                icon="minus"
                label={t('calcLessScans')}
                onClick={() => setScans((value) => Math.max(10, value - 10))}
                disabled={scans <= 10}
              />
              <span className="min-w-[74px] rounded-lg bg-drop-surface2 px-3 py-1.5 text-center text-[13px] font-semibold text-drop-text tabular">
                {scans}
              </span>
              <StepButton
                icon="plus"
                label={t('calcMoreScans')}
                onClick={() => setScans((value) => Math.min(1000, value + 10))}
                disabled={scans >= 1000}
              />
            </div>
          </ControlRow>

          <ControlRow label={t('calcNights')} sub={t('calcNightsHint')}>
            <div className="flex items-center gap-2">
              <StepButton
                icon="minus"
                label={t('calcFewerNights')}
                onClick={() => setNights((value) => Math.max(1, value - 1))}
                disabled={nights <= 1}
              />
              <span className="min-w-[74px] rounded-lg bg-drop-surface2 px-3 py-1.5 text-center text-[13px] font-semibold text-drop-text tabular">
                {nights}
              </span>
              <StepButton
                icon="plus"
                label={t('calcMoreNights')}
                onClick={() => setNights((value) => Math.min(31, value + 1))}
                disabled={nights >= 31}
              />
            </div>
          </ControlRow>

          <div className="mt-5 grid grid-cols-2 gap-x-6 gap-y-4 border-t border-drop-line pt-4">
            {[
              { label: t('calcMonthlyRequests'), value: int(requestsPerMonth) },
              { label: t('calcPerRequest'), value: money(clubPerRequest) },
              { label: t('calcPerNight'), value: money(clubPerNight) },
              { label: t('calcTurnover'), value: money(guestTurnover) },
            ].map((stat) => (
              <div key={stat.label}>
                <p className="text-[11px] leading-snug text-drop-muted">{stat.label}</p>
                <p className="mt-1 text-[15px] font-semibold text-drop-text tabular">{stat.value}</p>
              </div>
            ))}
          </div>
        </div>

        <div className="flex min-w-0 flex-col">
          <div>
            <p className="text-[11px] text-drop-muted">{t('calcMonthly')}</p>
            <p className="mt-1.5 font-display text-[38px] leading-none text-drop-text tabular sm:text-[42px]">
              {money(clubPerMonth)}
            </p>
            <p className="mt-2 text-[11px] text-drop-muted tabular">
              {int(scans)} {t('calcScansUnit')} × {money(clubPerRequest)} × {nights} {t('calcNightsUnit')}
            </p>
          </div>

          <div className="mt-6 border-t border-drop-line pt-4">
            <div className="flex flex-wrap items-baseline justify-between gap-2">
              <p className="text-[12px] font-medium text-drop-text">{t('calcChartTitle')}</p>
              <p className="text-[11px] text-drop-muted tabular">
                {t('calcAverage')} {money(chartMean)} {t('calcNight')}
              </p>
            </div>
            <div className="mt-2">
              <TrendChart
                points={points}
                mean={chartMean}
                height={206}
                valueFormat={(value) => int(value / 100)}
                axisFormat={(value) => int(value / 100)}
                tooltipFormat={money}
                ariaLabel={chartLabel}
              />
            </div>
          </div>
        </div>
      </div>

      <p className="flex items-start gap-2 border-t border-drop-line pt-4 text-[11px] leading-relaxed text-drop-muted">
        <Icon name="info" size={13} className="mt-0.5 shrink-0" />
        {t('calcNote')}
      </p>
    </div>
  );
}
