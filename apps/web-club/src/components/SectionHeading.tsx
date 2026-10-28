import type { ReactNode } from 'react';

interface SectionHeadingProps {
  title: string;
  detail?: string;
  action?: ReactNode;
}

/**
 * Small heading for a group inside a panel: 15/600 title, 12px detail, quiet
 * action slot. No rule and no uppercase — `Panel` already owns the panel-level
 * heading, this one is for a second level inside the body.
 */
export default function SectionHeading({ title, detail, action }: SectionHeadingProps) {
  return (
    <div className="flex items-end justify-between gap-4">
      <div className="min-w-0">
        <h2 className="text-[length:var(--fs-15)] font-semibold leading-[1.3] text-[color:var(--c-text)]">{title}</h2>
        {detail ? (
          <p className="mt-1 text-[length:var(--fs-12)] leading-[1.45] text-[color:var(--c-text-muted)]">{detail}</p>
        ) : null}
      </div>
      {action ? <div className="flex flex-none items-center gap-2">{action}</div> : null}
    </div>
  );
}
