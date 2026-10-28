import type { ReactNode } from 'react';

interface PageHeadingProps {
  title: string;
  action?: ReactNode;
}

/**
 * The admin page head, in the shared panel language.
 *
 * One composition for every admin route: the page title and the actions that
 * belong to the whole page on the right. No eyebrow above the title and no
 * explanation under it — the head names the page, and state belongs to the
 * panel that owns it. Layout, type and spacing come from `panel-page-head` in
 * the shared kit, so this component adds no styling of its own.
 */
export default function PageHeading({ title, action }: PageHeadingProps) {
  return (
    <header className="panel-page-head">
      <div style={{ minWidth: 0 }}>
        <h1 className="panel-page-title">{title}</h1>
      </div>
      {action ? <div className="panel-page-actions">{action}</div> : null}
    </header>
  );
}
