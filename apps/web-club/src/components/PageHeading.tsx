import type { ReactNode } from 'react';

interface PageHeadingProps {
  title: string;
  /**
   * The one line that names *what* the page is when the title alone cannot.
   * Optional and rare on purpose: the head is normally the page's name, and a
   * sentence under every title would turn nine working screens into marketing.
   * It is used where the subject itself needs a qualifier (the developer surface
   * names its API version and its three parts), and it renders as quiet 13px
   * text, not as copy.
   */
  subtitle?: string;
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
export default function PageHeading({ title, subtitle, action }: PageHeadingProps) {
  return (
    <header className="mb-6 flex items-end justify-between gap-5 max-sm:mb-5 max-sm:flex-col max-sm:items-stretch max-sm:gap-3">
      <div className="min-w-0">
        <h1 className="text-[length:var(--fs-28)] font-semibold leading-[1.15] tracking-[-0.01em] text-[color:var(--c-text)]">
          {title}
        </h1>
        {subtitle ? (
          <p className="mt-1 text-[length:var(--fs-13)] leading-[1.4] text-[color:var(--c-text-muted)]">{subtitle}</p>
        ) : null}
      </div>
      {action ? (
        <div className="flex flex-none flex-wrap items-center gap-2 max-sm:w-full">{action}</div>
      ) : null}
    </header>
  );
}
