import { useEffect } from 'react';
import { Link } from 'react-router-dom';
import BrandLogo from '../components/BrandLogo';
import Icon, { type IconName } from '../components/connect/Icons';
import PublicBar from '../components/connect/PublicBar';
import { useLocale } from '../connect/locale';
import { DEFAULT_PRICE_MINOR, clubShareMinor } from '../components/connect/landing/scenario';
import './hub.css';

interface HubStat {
  value: string;
  label: string;
}

interface HubBullet {
  icon: IconName;
  text: string;
}

interface HubSideProps {
  /** Anchors the heading for the link's accessible name. */
  id: string;
  to: string;
  image: string;
  imageAlt: string;
  side: string;
  title: string;
  description: string;
  bullets: HubBullet[];
  stats: HubStat[];
  cta: string;
  /** Guest = accent fill with ink text; club = ink fill with accent text. */
  ctaClassName: string;
  /** Hairline between the two halves, drawn on the left half only. */
  edgeClassName: string;
  photoClassName: string;
}

/**
 * One half of the gate: a single link whose whole area is a photograph.
 *
 * The copy sits on top of the photo, so the link owns the pointer and the
 * keyboard focus; the CTA is a span, never a nested control. The colour switch
 * between the two halves is pure CSS (`:has()` in hub.css) — this component
 * carries no state.
 */
function HubSide({
  id,
  to,
  image,
  imageAlt,
  side,
  title,
  description,
  bullets,
  stats,
  cta,
  ctaClassName,
  edgeClassName,
  photoClassName,
}: HubSideProps) {
  return (
    <Link
      to={to}
      aria-labelledby={`${id}-title`}
      className={`hub-side ${edgeClassName}`}
    >
      <img
        className={`hub-photo ${photoClassName}`}
        src={image}
        alt={imageAlt}
        loading="eager"
        decoding="async"
      />
      <span className="hub-scrim" aria-hidden="true" />

      <div className="hub-copy gap-8 px-5 pb-7 pt-8 sm:px-8 sm:pb-8 sm:pt-10 min-[900px]:px-10 min-[900px]:pb-8 min-[900px]:pt-12 min-[1440px]:px-14 min-[1440px]:pb-10">
        <div className="flex flex-col gap-5">
          <p className="flex items-center gap-3.5">
            <span className="font-mono text-[11px] uppercase leading-none tracking-[.16em] text-drop-accent">
              {side}
            </span>
            <span className="h-px flex-1 bg-white/25" aria-hidden="true" />
          </p>

          <h2
            id={`${id}-title`}
            className="hub-side-title connect-display text-drop-text"
          >
            {title}
          </h2>

          <p className="max-w-[460px] text-[15px] leading-[1.55] text-drop-muted">{description}</p>

          <ul className="flex flex-col gap-2.5">
            {bullets.map((bullet) => (
              <li key={bullet.text} className="flex items-start gap-2.5 text-[14px] leading-[1.45] text-drop-text">
                <Icon
                  name={bullet.icon}
                  size={15}
                  strokeWidth={1.7}
                  className="mt-0.5 shrink-0 text-drop-accent"
                />
                <span>{bullet.text}</span>
              </li>
            ))}
          </ul>
        </div>

        <div className="flex flex-col gap-5">
          <span
            className={`inline-flex min-h-11 items-center gap-2 self-start rounded-field px-5 text-[13px] font-semibold ${ctaClassName}`}
          >
            {cta}
            <Icon name="arrowRight" size={15} strokeWidth={2} />
          </span>

          <dl className="grid grid-cols-1 gap-x-8 gap-y-4 border-t border-white/20 pt-4 min-[420px]:grid-cols-2">
            {stats.map((stat) => (
              <div key={stat.label}>
                <dt className="font-display text-[22px] leading-none text-drop-text tabular">{stat.value}</dt>
                <dd className="mt-1.5 font-mono text-[11px] uppercase leading-[1.35] tracking-[.08em] text-drop-muted">
                  {stat.label}
                </dd>
              </div>
            ))}
          </dl>
        </div>
      </div>
    </Link>
  );
}

/**
 * The domain root: the two-sided gate between a guest and a club.
 *
 * Implementation contract: `docs/ui/hub-download-contract.md`; design frame:
 * `DROP IT — Hub · Choose side` in `docs/ui/reference/dropit-design.pen`.
 * The frame's two halves are photo-backed here, and its second CTA per half is
 * intentionally omitted: one door per side, one action per door.
 */
export default function HubPage() {
  const { t, locale } = useLocale();
  const title = `${t('gateTitle')} · DROP IT`;

  useEffect(() => {
    const previousTitle = document.title;
    document.title = title;
    return () => { document.title = previousTitle; };
  }, [title]);

  // Money never lives in a translated string: the amounts come from the same
  // sample scenario the club landing renders, formatted for the active locale.
  const money = (minor: number) =>
    new Intl.NumberFormat(locale, {
      style: 'currency',
      currency: 'EUR',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(minor / 100);
  // The two "no charge" stats are counts of charges, not prices: plain `0`
  // reads as "nothing to pay", while "0,00 €" would read as a zero price.
  const zero = new Intl.NumberFormat(locale).format(0);

  const guestStats: HubStat[] = [
    { value: money(DEFAULT_PRICE_MINOR), label: t('hubGuestStatPrice') },
    { value: zero, label: t('hubGuestStatFree') },
  ];
  const clubStats: HubStat[] = [
    { value: money(clubShareMinor(DEFAULT_PRICE_MINOR)), label: t('hubClubStatShare') },
    { value: zero, label: t('hubClubStatSetup') },
  ];

  const guestBullets: HubBullet[] = [
    { icon: 'scanQrCode', text: t('hubGuestBullet1') },
    { icon: 'music', text: t('hubGuestBullet2') },
    { icon: 'live', text: t('hubGuestBullet3') },
  ];
  const clubBullets: HubBullet[] = [
    { icon: 'coins', text: t('hubClubBullet1') },
    { icon: 'listMusic', text: t('hubClubBullet2') },
    { icon: 'chart', text: t('hubClubBullet3') },
  ];

  return (
    <div className="connect-root hub-root flex min-h-screen flex-col bg-drop-bg text-drop-text">
      <PublicBar />

      <main className="hub-gate">
        <section className="hub-intro">
          <div className="mx-auto flex w-full max-w-[1440px] flex-col gap-3 px-5 pb-5 pt-7 sm:px-8 lg:flex-row lg:items-end lg:justify-between lg:gap-12 lg:px-12 lg:pb-7 lg:pt-9">
            <div className="min-w-0">
              <p className="connect-eyebrow">{t('gateEyebrow')}</p>
              <h1 className="connect-display mt-2 text-[28px] leading-[1.05] sm:text-[32px] lg:text-[38px]">
                {t('gateTitle')}
              </h1>
            </div>
            <p className="max-w-[400px] text-[13px] leading-[1.5] text-drop-muted lg:pb-1.5 lg:text-right">
              {t('gateNote')}
            </p>
          </div>
        </section>

        <div className="hub-split">
          <HubSide
            id="hub-guest"
            to="/download"
            image="/hero-club.jpg"
            imageAlt={t('hubGuestImageAlt')}
            side={t('hubSideGuest')}
            title={t('hubGuestTitle')}
            description={t('gateGuestDesc')}
            bullets={guestBullets}
            stats={guestStats}
            cta={t('gateGuestCta')}
            ctaClassName="bg-drop-accent text-drop-ink"
            edgeClassName="border-b border-drop-line min-[900px]:border-b-0 min-[900px]:border-r"
            photoClassName="object-center"
          />

          <HubSide
            id="hub-club"
            to="/connect"
            image="/phone-hero.jpg"
            imageAlt={t('hubClubImageAlt')}
            side={t('hubSideClub')}
            title={t('hubClubTitle')}
            description={t('gateClubDesc')}
            bullets={clubBullets}
            stats={clubStats}
            cta={t('gateClubCta')}
            ctaClassName="bg-drop-ink text-drop-accent ring-1 ring-inset ring-drop-accent/45"
            edgeClassName=""
            photoClassName="hub-photo--club"
          />
        </div>
      </main>

      <footer className="border-t border-drop-line bg-drop-bg">
        <div className="mx-auto flex w-full max-w-[1440px] flex-col gap-x-8 gap-y-2 px-5 py-3 sm:px-8 lg:flex-row lg:items-center lg:justify-between lg:px-12">
          <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1">
            <BrandLogo size="small" />
            <p className="font-mono text-[11px] uppercase tracking-[1.2px] text-drop-muted">
              {t('copyright')}
            </p>
          </div>
          <nav className="flex flex-wrap items-center gap-x-5">
            <Link
              to="/download"
              className="inline-flex min-h-11 items-center text-[12px] text-drop-muted transition-colors hover:text-drop-accent focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50"
            >
              {t('hubFooterGuests')}
            </Link>
            <Link
              to="/connect"
              className="inline-flex min-h-11 items-center text-[12px] text-drop-muted transition-colors hover:text-drop-accent focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50"
            >
              {t('hubFooterClubs')}
            </Link>
            <a
              href="mailto:hello@dropit.music"
              className="inline-flex min-h-11 items-center text-[12px] text-drop-muted transition-colors hover:text-drop-accent focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50"
            >
              {t('hubFooterSupport')}
            </a>
          </nav>
        </div>
      </footer>
    </div>
  );
}
