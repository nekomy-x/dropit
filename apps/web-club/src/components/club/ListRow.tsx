/**
 * ListRow — one row in a list (§4.9).
 *
 * Minimum 44px tall, separated by a 1px hairline at 6% white (never a border
 * around the row), hover uses the `raised` surface. Rows that are links or
 * buttons keep the same rhythm and gain a real hit target.
 */
import type { ReactNode } from 'react';

export interface ListRowProps {
  title: ReactNode;
  detail?: ReactNode;
  /** Right-aligned figures — tabular, mono. */
  meta?: ReactNode;
  /** Leading marker slot (avatar, index, dot). */
  leading?: ReactNode;
  /** Renders the row as a link when set. */
  href?: string;
  onClick?: () => void;
  className?: string;
}

export default function ListRow({ title, detail, meta, leading, href, onClick, className }: ListRowProps) {
  const classes = ['club-list-row', className].filter(Boolean).join(' ');
  const body = (
    <>
      {leading ? <span className="club-list-row-leading" aria-hidden="true">{leading}</span> : null}
      <span className="club-list-row-main">
        <span className="club-list-row-title">{title}</span>
        {detail ? <span className="club-list-row-detail">{detail}</span> : null}
      </span>
      {meta ? <span className="club-list-row-meta">{meta}</span> : null}
    </>
  );

  if (href) {
    return (
      <li>
        <a className={classes} href={href}>{body}</a>
      </li>
    );
  }
  if (onClick) {
    return (
      <li>
        <button type="button" className={classes} onClick={onClick} data-control="button">{body}</button>
      </li>
    );
  }
  return <li className={classes}>{body}</li>;
}
