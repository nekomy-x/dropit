/**
 * Panel — the only large surface in the club panel.
 *
 * Contract (01-contract.md §2): stamps `data-surface="panel"`. No border, no
 * blurred shadow, radius comes from `--r-surface` (16px). Pages must not add
 * their own surface wrapper around it.
 */
import { useId, type ReactNode } from 'react';

export interface PanelProps {
  title?: ReactNode;
  detail?: ReactNode;
  /** Rendered on the right of the header — a link, a Chip, a quiet button. */
  action?: ReactNode;
  /** Visual grouping only; the DOM keeps a plain <section>. */
  as?: 'section' | 'article' | 'div';
  className?: string;
  bodyClassName?: string;
  children?: ReactNode;
  /** Extra data attributes for page-level hooks without breaking the contract. */
  id?: string;
  'aria-labelledby'?: string;
}

export default function Panel({
  title,
  detail,
  action,
  as: Tag = 'section',
  className,
  bodyClassName,
  children,
  id,
  ...rest
}: PanelProps) {
  const generatedId = useId();
  const headingId = title ? `${id ?? generatedId}-title` : undefined;
  const classes = ['club-panel', className].filter(Boolean).join(' ');
  return (
    <Tag className={classes} data-surface="panel" id={id} aria-labelledby={headingId} {...rest}>
      {title || action ? (
        <header className="club-panel-head">
          <div className="club-panel-head-copy">
            {title ? <h2 className="club-panel-title" id={headingId}>{title}</h2> : null}
            {detail ? <p className="club-panel-detail">{detail}</p> : null}
          </div>
          {action ? <div className="club-panel-actions">{action}</div> : null}
        </header>
      ) : null}
      {children ? <div className={bodyClassName}>{children}</div> : null}
    </Tag>
  );
}
