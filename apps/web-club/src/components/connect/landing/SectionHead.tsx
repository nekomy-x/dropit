import type { ReactNode } from 'react';
import Reveal from '../Reveal';

/** Eyebrow + headline on the left, muted note block on the right. */
export default function SectionHead({
  eyebrow,
  headline,
  note,
}: {
  eyebrow: string;
  headline: string;
  note?: ReactNode;
}) {
  return (
    <Reveal>
      <div className="flex flex-col justify-between gap-6 lg:flex-row lg:items-end">
        <div className="max-w-[640px]">
          <p className="connect-eyebrow">{eyebrow}</p>
          <h2 className="connect-display mt-4 text-[30px] leading-[1.06] sm:text-[38px] lg:text-[42px]">
            {headline}
          </h2>
        </div>
        {note && (
          <div className="connect-section-note max-w-[320px] text-[12px] leading-relaxed text-drop-muted lg:text-right">
            {note}
          </div>
        )}
      </div>
    </Reveal>
  );
}
