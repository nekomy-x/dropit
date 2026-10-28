/**
 * EmptyState — text plus one action, no illustration (§4.9).
 */
import type { ReactNode } from 'react';

export interface EmptyStateProps {
  title: string;
  detail: string;
  /** The single next action. */
  action?: ReactNode;
  className?: string;
}

export default function EmptyState({ title, detail, action, className }: EmptyStateProps) {
  return (
    <div className={['club-state', className].filter(Boolean).join(' ')} data-empty-state="">
      <p className="club-state-title">{title}</p>
      <p className="club-state-detail">{detail}</p>
      {action}
    </div>
  );
}
