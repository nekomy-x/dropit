import { Link } from 'react-router-dom';
import Reveal from '../Reveal';
import Icon from '../Icons';
import SectionHead from './SectionHead';
import { useLocale } from '../../../connect/locale';
import { CLUB_JOIN_CTA } from '../../../connect/cta';

const STEPS = [
  {
    number: '01',
    time: '~5 мин',
    title: 'apply', desc: 'applyCopy',
  },
  {
    number: '02',
    time: '10 минут',
    title: 'profile', desc: 'profileCopy',
  },
  {
    number: '03',
    time: '1 вечер',
    title: 'qrSetup', desc: 'qrSetupCopy',
  },
  {
    number: '04',
    time: 'первая ночь',
    title: 'firstSession', desc: 'firstSessionCopy',
  },
];

const STEP_TIMES = {
  en: ['~5 min', '10 minutes', '1 evening', 'first night'],
  es: ['~5 min', '10 minutos', '1 noche', 'primera noche'],
  fr: ['~5 min', '10 minutes', '1 soirée', 'première soirée'],
  de: ['~5 Min.', '10 Minuten', '1 Abend', 'erste Nacht'],
  pt: ['~5 min', '10 minutos', '1 noite', 'primeira noite'],
  ru: ['~5 мин', '10 минут', '1 вечер', 'первая ночь'],
} as const;

export default function StepsSection() {
  const { t, language } = useLocale();
  return (
    <section id="steps" className="border-t border-drop-line bg-drop-bg">
      <div className="mx-auto w-full max-w-[1440px] px-6 py-16 lg:px-12 lg:py-24">
        <SectionHead
          eyebrow={t('stepsEyebrow')}
          headline={t('stepsTitle')}
          note={t('stepsNote')}
        />

        <div className="mt-14 grid gap-px overflow-hidden rounded-card border border-drop-line bg-drop-line sm:grid-cols-2 lg:grid-cols-4">
          {STEPS.map((step, index) => (
            <Reveal key={step.number} delay={index * 80}>
              <div className="group relative flex h-full flex-col gap-4 bg-drop-bg p-6 transition-colors duration-300 hover:bg-drop-surface">
                {/* Rail dot + connector */}
                <div className="flex items-center gap-2">
                  <span className="h-1.5 w-1.5 rounded-full bg-drop-accent transition-transform duration-300 group-hover:scale-150" />
                  <span className="h-px flex-1 bg-drop-line" />
                </div>

                <div className="flex items-center gap-2.5">
                  <span className="font-mono text-[11px] text-drop-accent">{step.number}</span>
                  <span className="rounded-full border border-drop-line px-2 py-0.5 font-mono text-[9px] uppercase tracking-[1px] text-drop-dim">
                    {STEP_TIMES[language][index]}
                  </span>
                </div>

                <h3 className="text-[15px] font-semibold text-drop-text">{t(step.title as 'apply' | 'profile' | 'qrSetup' | 'firstSession')}</h3>
                <p className="text-[12px] leading-relaxed text-drop-muted">{t(step.desc as 'applyCopy' | 'profileCopy' | 'qrSetupCopy' | 'firstSessionCopy')}</p>
              </div>
            </Reveal>
          ))}
        </div>

        <Reveal delay={100}>
          <div className="mt-6 flex flex-col justify-between gap-4 rounded-card border border-drop-line bg-drop-surface px-6 py-5 sm:flex-row sm:items-center">
            <div>
              <p className="connect-mono-label">{t('totalTime')}</p>
              <p className="mt-1 text-[11px] text-drop-dim">{t('average')}</p>
            </div>
            <div className="flex items-center gap-6">
              <p className="font-display text-[30px] leading-none text-drop-text">{t('oneDay')}</p>
              <Link to={CLUB_JOIN_CTA} className="connect-btn-primary group h-11">
                {t('startSetup')}
                <Icon
                  name="arrowRight"
                  size={14}
                  strokeWidth={2}
                  className="transition-transform duration-200 group-hover:translate-x-1"
                />
              </Link>
            </div>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
