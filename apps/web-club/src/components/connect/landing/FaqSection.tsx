import { useState } from 'react';
import Reveal from '../Reveal';
import Icon from '../Icons';
import SectionHead from './SectionHead';
import { useLocale } from '../../../connect/locale';

const FAQ_KEYS = [
  ['faqQ1', 'faqA1'], ['faqQ2', 'faqA2'], ['faqQ3', 'faqA3'],
  ['faqQ4', 'faqA4'], ['faqQ5', 'faqA5'],
] as const;

export default function FaqSection() {
  const [open, setOpen] = useState<number | null>(0);
  const { t } = useLocale();

  return (
    <section id="faq" className="border-t border-drop-line bg-drop-bg">
      <div className="mx-auto w-full max-w-[1440px] px-6 py-16 lg:px-12 lg:py-24">
        <div className="grid gap-12 lg:grid-cols-[minmax(0,420px)_minmax(0,1fr)]">
          <Reveal>
            <div className="lg:sticky lg:top-24">
              <SectionHead eyebrow={t('faqEyebrow')} headline={t('faqTitle')} />
              <div className="mt-8 border-t border-drop-line pt-6">
                <p className="text-[12px] leading-relaxed text-drop-muted">
                  {t('faqNote')}
                </p>
                <a
                  href="mailto:clubs@dropit.music"
                  className="mt-3 inline-flex items-center gap-2 text-[13px] text-drop-accent transition-colors hover:text-drop-text"
                >
                  <Icon name="mail" size={14} />
                  clubs@dropit.music
                </a>
              </div>
            </div>
          </Reveal>

          <div className="border-t border-drop-line">
            {FAQ_KEYS.map(([question, answer], index) => {
              const isOpen = open === index;
              return (
                  <Reveal key={question} delay={index * 50}>
                  <div className="border-b border-drop-line">
                    <button
                      type="button"
                      onClick={() => setOpen(isOpen ? null : index)}
                      aria-expanded={isOpen}
                      className="group flex w-full items-center justify-between gap-6 py-5 text-left"
                    >
                      <span
                        className={`text-[14px] transition-colors duration-200 ${
                          isOpen ? 'text-drop-text' : 'text-drop-muted group-hover:text-drop-text'
                        }`}
                      >
                        {t(question)}
                      </span>
                      <span
                        className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full border transition-all duration-300 ${
                          isOpen
                            ? 'rotate-45 border-drop-accent text-drop-accent'
                            : 'border-drop-line text-drop-muted group-hover:border-drop-accent/40'
                        }`}
                      >
                        <Icon name="plus" size={13} strokeWidth={2} />
                      </span>
                    </button>

                    <div
                      className={`grid overflow-hidden transition-[grid-template-rows,opacity] duration-300 ease-out ${
                        isOpen ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'
                      }`}
                    >
                      <div className="min-h-0">
                        <p className="max-w-[620px] pb-5 pr-10 text-[13px] leading-relaxed text-drop-muted">
                          {t(answer)}
                        </p>
                      </div>
                    </div>
                  </div>
                </Reveal>
              );
            })}
          </div>
        </div>
      </div>
    </section>
  );
}
