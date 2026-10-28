import type { ReactNode } from 'react';

interface PageHeadingProps {
  title: string;
  action?: ReactNode;
}

/**
 * Page heading for the operational club pages.
 *
 * Contract (01-contract.md §1, §4.4): 28px title and the page actions. The head
 * is the page's name, not a summary: no eyebrow above it and no explanation
 * under it. Whatever the page needs to say about its state belongs to the
 * panels below, where it is attached to the thing it describes.
 */
export default function PageHeading({ title, action }: PageHeadingProps) {
  return (
    <header className="mb-6 flex items-end justify-between gap-5 max-sm:mb-5 max-sm:flex-col max-sm:items-stretch max-sm:gap-3">
      <div className="min-w-0">
        <h1 className="text-[length:var(--fs-28)] font-semibold leading-[1.15] tracking-[-0.01em] text-[color:var(--c-text)]">
          {title}
        </h1>
      </div>
      {action ? (
        <div className="flex flex-none flex-wrap items-center gap-2 max-sm:w-full">{action}</div>
      ) : null}
    </header>
  );
}
