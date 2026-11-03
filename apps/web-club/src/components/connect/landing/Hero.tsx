import { Link } from 'react-router-dom';
import Icon from '../Icons';
import { useLocale } from '../../../connect/locale';
import { CLUB_JOIN_CTA } from '../../../connect/cta';
/* The phone scale for the whole landing (type floor, thumb targets, hero
   height). Loaded here because the hero is the page's first section; every
   rule inside is scoped to `.connect-sprint-page` and `max-width: 767px`. */
import './connect-phone.css';

const STATS = ['0 €', '0.30 €'];
const AVATARS = ['#3A4038', '#5A5F58', '#7C8278', '#00FF87'];

const HOW_IT_WORKS = [
  { number: '01', title: 'stepQr', desc: 'stepQrCopy' },
  { number: '02', title: 'stepPay', desc: 'stepPayCopy' },
  { number: '03', title: 'stepEarn', desc: 'stepEarnCopy' },
] as const;

export default function Hero() {
  const { t } = useLocale();
  return (
    <section id="hero" className="connect-hero sprint-hero relative overflow-hidden">
      <div className="sprint-hero-photo" aria-hidden="true" />
      <div className="sprint-hero-content">
        <p className="sprint-hero-eyebrow">{t('eyebrow')}</p>
        <h1>{t('heroTitle')}</h1>
        <p className="sprint-hero-copy">{t('heroCopy')}</p>
        <div className="sprint-hero-actions">
          <Link to={CLUB_JOIN_CTA} className="connect-btn-primary">
            {t('heroCta')}
            <Icon name="arrowRight" size={15} strokeWidth={2} />
          </Link>
          <a href="#revenue" className="sprint-hero-secondary">{t('heroSecondary')}</a>
        </div>
      </div>
    </section>
  );
}

/** Original setup, service-fee, venue-proof and three-step copy, below the product preview. */
export function HeroDetails() {
  const { t } = useLocale();
  return (
    <section className="sprint-hero-details" aria-label="Как работает DROP IT">
      <div className="sprint-proof-row">
        {[...STATS, t('firstNightValue')].map((value, index) => (
          <div className="sprint-proof-item" key={index}>
            <span className="sprint-proof-value">{value}</span>
            <span className="sprint-proof-label">{t((['setup', 'fee', 'firstNight'] as const)[index]!)}</span>
          </div>
        ))}
        <div className="sprint-venue-proof">
          <span className="sprint-avatars" aria-hidden="true">
            {AVATARS.map((color, index) => <span key={index} style={{ backgroundColor: color }} />)}
            <span className="sprint-avatar-count">+38</span>
          </span>
          <span>{t('places')}</span>
        </div>
      </div>
      <div className="sprint-how-row">
        {HOW_IT_WORKS.map((step) => (
          <article className="sprint-how-item" key={step.number}>
            <span className="sprint-how-number">{step.number}</span>
            <div>
              <h2>{t(step.title)}</h2>
              <p>{t(step.desc)}</p>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
