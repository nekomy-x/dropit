import { Link } from 'react-router-dom';
import Reveal from '../Reveal';
import Icon from '../Icons';
import TriangleField from './TriangleField';
import { useLocale } from '../../../connect/locale';
import { CLUB_JOIN_CTA } from '../../../connect/cta';

export default function FinalCta() {
  const { t } = useLocale();
  return (
    <section className="connect-final-cta relative isolate overflow-hidden border-t border-drop-line">
      {/* Tiled stage: a tilted field of small triangles that reacts to the beat and the pointer. */}
      <div aria-hidden="true" className="connect-final-cta-stage pointer-events-none absolute inset-0 -z-10">
        <TriangleField />
      </div>

      <div className="relative mx-auto flex w-full max-w-[1440px] flex-col items-center px-6 py-20 text-center lg:px-12 lg:py-24">
        <Reveal>
          <p className="connect-eyebrow">{t('finalEyebrow')}</p>
        </Reveal>

        <Reveal delay={70}>
          <h2 className="connect-display mx-auto mt-5 max-w-[760px] text-[32px] leading-[1.06] text-white sm:text-[42px] lg:text-[50px] lg:leading-[54px]">
            {t('finalTitle')}
          </h2>
        </Reveal>

        <Reveal delay={120}>
          <p className="mx-auto mt-5 max-w-[520px] text-[15px] leading-[26px] text-[#C2CBC5]">
            {t('finalCopy')}
          </p>
        </Reveal>

        <Reveal delay={170} className="w-full sm:w-auto">
          {/* Two actions of equal weight and equal metrics: same height, same
              text size, one trailing glyph each. Stacked below sm so the pair
              stays a tidy column instead of two ragged widths. */}
          <div className="mt-9 flex w-full flex-col items-stretch justify-center gap-3 sm:w-auto sm:flex-row sm:items-center">
            <Link to={CLUB_JOIN_CTA} className="connect-btn-primary group w-full sm:w-auto">
              {t('finalCta')}
              <Icon
                name="arrowRight"
                size={15}
                strokeWidth={2}
                className="transition-transform duration-200 group-hover:translate-x-1"
              />
            </Link>
            <a href="#panel" className="connect-btn-ghost gap-2.5 w-full sm:w-auto">
              {t('demo')}
              <Icon name="layers" size={15} strokeWidth={1.9} />
            </a>
          </div>
        </Reveal>

        <Reveal delay={210}>
          {/* Sits on the lit tile field, so it takes a contrast-safe colour
              instead of the dim grey used on flat backgrounds. */}
          <p className="mt-6 font-mono text-[10px] uppercase tracking-[1.2px] text-[#9BA5A0]">
            {t('freeNote')}
          </p>
        </Reveal>
      </div>
    </section>
  );
}
