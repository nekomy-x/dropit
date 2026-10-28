import Reveal from '../Reveal';
import Icon from '../Icons';
import SectionHead from './SectionHead';
import { useLocale } from '../../../connect/locale';

const ROWS = [
  {
    index: '01',
    title: 'benefit1', desc: 'benefit1Copy', tag: 'benefit1Tag',
  },
  {
    index: '02',
    title: 'benefit2', desc: 'benefit2Copy', tag: 'benefit2Tag',
  },
  {
    index: '03',
    title: 'benefit3', desc: 'benefit3Copy', tag: 'benefit3Tag',
  },
  {
    index: '04',
    title: 'benefit4', desc: 'benefit4Copy', tag: 'benefit4Tag',
  },
];

export default function Benefits() {
  const { t } = useLocale();
  return (
    <section id="benefits" className="border-t border-drop-line bg-drop-bg">
      <div className="mx-auto w-full max-w-[1440px] px-6 pb-14 pt-16 lg:px-12 lg:pb-16 lg:pt-16">
        <SectionHead
          eyebrow={t('benefitsEyebrow')}
          headline={t('benefitsTitle')}
          note={t('benefitsNote')}
        />

        <div className="mt-12 border-t border-drop-line">
          {ROWS.map((row, i) => (
            <Reveal key={row.index} delay={i * 70}>
              <div className="group relative border-b border-drop-line">
                {/* Mint wipe that grows from the left on hover. */}
                <span
                  aria-hidden="true"
                   className="pointer-events-none absolute inset-y-0 left-0 w-0 bg-[linear-gradient(90deg,#00FF8708,transparent)] transition-[width] duration-500 ease-out group-hover:w-full"
                />
                <div className="relative flex flex-col gap-4 py-8 lg:flex-row lg:items-center lg:gap-8">
                  <span className="font-mono text-[11px] text-drop-accent lg:w-10">{row.index}</span>
                  <div className="flex-1 lg:max-w-[340px]">
                    <h3 className="text-[17px] font-semibold text-drop-text">{t(row.title as 'benefit1' | 'benefit2' | 'benefit3' | 'benefit4')}</h3>
                  </div>
                  <p className="flex-1 text-[13px] leading-relaxed text-drop-muted">{t(row.desc as 'benefit1Copy' | 'benefit2Copy' | 'benefit3Copy' | 'benefit4Copy')}</p>
                  <div className="flex items-center gap-5">
                    <span className="rounded-full border border-drop-line px-3 py-1.5 font-mono text-[9px] uppercase tracking-[1.2px] text-drop-muted transition-colors duration-300 group-hover:border-drop-accent/50 group-hover:text-drop-accent">
                      {t(row.tag as 'benefit1Tag' | 'benefit2Tag' | 'benefit3Tag' | 'benefit4Tag')}
                    </span>
                    <span className="text-drop-dim transition-all duration-300 group-hover:translate-x-1 group-hover:text-drop-accent">
                      <Icon name="arrowUpRight" size={16} />
                    </span>
                  </div>
                </div>
              </div>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
