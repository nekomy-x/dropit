import Reveal from '../Reveal';
import Icon, { type IconName } from '../Icons';
import SectionHead from './SectionHead';
import CalculatorCard from './CalculatorCard';
import { DonutSplit } from './charts';
import { useLocale } from '../../../connect/locale';
import { DEFAULT_PRICE_MINOR, SERVICE_FEE_MINOR, clubShareMinor } from './scenario';

const BULLETS: { icon: IconName; key: 'revenueBullet1' | 'revenueBullet2' | 'revenueBullet3' | 'revenueBullet4' }[] = [
  { icon: 'wallet', key: 'revenueBullet1' },
  { icon: 'spark', key: 'revenueBullet2' },
  { icon: 'chart', key: 'revenueBullet3' },
  { icon: 'calendar', key: 'revenueBullet4' },
];

/** One payment split: the club's share against the flat DROP IT fee. */
function FeeSplit() {
  const { t, locale } = useLocale();
  const clubMinor = clubShareMinor(DEFAULT_PRICE_MINOR);
  const clubShare = Math.round((clubMinor / DEFAULT_PRICE_MINOR) * 100);
  const money = (minor: number) =>
    new Intl.NumberFormat(locale, {
      style: 'currency',
      currency: 'EUR',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(minor / 100);

  return (
    <div className="connect-fee-split mt-10 rounded-lg border border-drop-line bg-drop-surface p-4">
      <div className="flex items-baseline justify-between gap-3">
        <p className="text-[11px] text-drop-muted">{t('feeSplitTitle')}</p>
        <p className="text-[13px] font-medium text-drop-text tabular">{money(DEFAULT_PRICE_MINOR)}</p>
      </div>

      <div className="connect-fee-split-body mt-4 flex items-center gap-5">
        <DonutSplit
          segments={[
            { label: t('calcPerRequest'), value: clubMinor, tone: 'accent' },
            { label: t('payoutServiceFee'), value: SERVICE_FEE_MINOR, tone: 'muted' },
          ]}
          size={96}
          thickness={10}
          centerValue={`${clubShare}%`}
          ariaLabel={`${t('feeSplitTitle')}: ${t('calcPerRequest')} ${money(clubMinor)}, ${t(
            'payoutServiceFee',
          )} ${money(SERVICE_FEE_MINOR)}`}
        />

        <dl className="flex min-w-0 flex-1 flex-col gap-2.5">
          <div className="flex items-baseline gap-2">
            <span
              className="h-1.5 w-1.5 shrink-0 rounded-full bg-drop-accent"
              aria-hidden="true"
            />
            <dt className="text-[11px] leading-snug text-drop-muted">{t('calcPerRequest')}</dt>
            <dd className="ml-auto whitespace-nowrap text-[11px] font-medium leading-snug text-drop-text tabular">
              {money(clubMinor)}
            </dd>
          </div>
          <div className="flex items-baseline gap-2">
            <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-drop-muted" aria-hidden="true" />
            <dt className="text-[11px] leading-snug text-drop-muted">{t('payoutServiceFee')}</dt>
            <dd className="ml-auto whitespace-nowrap text-[11px] font-medium leading-snug text-drop-text tabular">
              {money(SERVICE_FEE_MINOR)}
            </dd>
          </div>
        </dl>
      </div>
    </div>
  );
}

export default function RevenueSection() {
  const { t } = useLocale();
  return (
    <section id="revenue" className="border-t border-drop-line bg-drop-rail">
      <div className="mx-auto w-full max-w-[1440px] px-6 py-16 lg:px-12 lg:py-24">
        <SectionHead
          eyebrow={t('revenueEyebrow')}
          headline={t('revenueTitle')}
          note={t('revenueNote')}
        />

        <div className="mt-14 grid grid-cols-1 gap-8 xl:grid-cols-[minmax(0,380px)_minmax(0,1fr)]">
          <Reveal>
            <div className="max-w-[560px]">
              <p className="text-[14px] leading-relaxed text-drop-muted">
                {t('revenueBody')}
              </p>

              <ul className="mt-8 flex flex-col gap-5">
                {BULLETS.map((bullet) => (
                  <li key={bullet.key} className="flex items-start gap-3">
                    <span className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-lg border border-drop-line text-drop-accent">
                      <Icon name={bullet.icon} size={13} />
                    </span>
                    <span className="text-[13px] leading-relaxed text-drop-muted">{t(bullet.key)}</span>
                  </li>
                ))}
              </ul>

              <FeeSplit />
            </div>
          </Reveal>

          <Reveal delay={90}>
            <CalculatorCard />
          </Reveal>
        </div>
      </div>
    </section>
  );
}
