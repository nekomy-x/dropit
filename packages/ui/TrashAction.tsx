/**
 * TrashAction — the one destructive row control in the panels.
 *
 * A text button that says "Remove" is the loudest thing in a row it is not
 * about. A destructive action is rare and always confirmed, so it reads as a
 * bin at rest and states itself on approach:
 *
 *   rest             the bin is shut; the word beside it is quiet, so the row's
 *                    own data stays the first thing read;
 *   hover / focus    the lid lifts and the control takes the error tone — the
 *                    row now clearly offers one destructive action;
 *   open (expanded)  the lid stays up while the inline confirmation is on
 *                    screen, so the open panel is visibly tied to this control;
 *   press            the lid settles part-way down: direct feedback, not play.
 *
 * The word is always rendered. It is never revealed by hover alone: a control
 * whose label depends on a pointer is unusable by touch and by screen readers,
 * and here it is also the accessible name. Only the drawing moves.
 *
 * Why this is not `<Icon name="lucide:trash-2" />`: lucide ships the bin as one
 * flat set of paths with no lid group, and a lid that cannot move on its own
 * cannot open. The glyph below is the same lucide geometry (24 viewBox,
 * `1.75` stroke, round joins) split into `lid` and `can`, which is the whole
 * reason it lives here instead of in `Icon.tsx`.
 *
 * Motion contract (DESIGN.md §Motion): `transform` and colour only, `--dur-base`
 * / `--ease-out` from the token layer, and every transform is declared inside
 * `@media (prefers-reduced-motion: no-preference)` in `panel.css`. With reduced
 * motion the shut bin and the word are the finished state — nothing runs, and
 * `document.getAnimations()` stays empty.
 *
 * Confirmation is the caller's job: this control opens the confirm state and
 * never deletes on its own (DESIGN.md §Data honesty).
 */
import type { MouseEvent } from 'react';

export interface TrashActionProps {
  /** The visible word, e.g. the translated "Remove". Also part of the accessible name. */
  label: string;
  /** What exactly is being removed — pointer tooltip and assistive-tech subject. */
  detail?: string;
  /**
   * The click handler receives the event because the callers that own an inline
   * confirmation need the trigger element to restore focus to when it closes.
   */
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  /**
   * A blocked action stays visible but explains itself: a control that vanishes
   * teaches nothing about why the action is unavailable.
   */
  disabled?: boolean;
  /** Why the action is blocked, used as the native tooltip. */
  disabledReason?: string;
  /** Sets `aria-expanded` when the control opens an inline confirmation. */
  expanded?: boolean;
  className?: string;
}

/**
 * The bin, in the shared icon language, with the lid as its own hinge.
 * Path geometry is lucide's `trash`; only the grouping is ours.
 */
function TrashGlyph({ size }: { size: number }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <g className="trash-action-can">
        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
        <path d="M10 11v6" />
        <path d="M14 11v6" />
      </g>
      <g className="trash-action-lid">
        <path d="M3 6h18" />
        <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      </g>
    </svg>
  );
}

export default function TrashAction({
  label,
  detail,
  onClick,
  disabled = false,
  disabledReason,
  expanded,
  className,
}: TrashActionProps) {
  // One name in both states: the word first, then the subject, so a screen
  // reader says "Remove · Nora Vex" without the row having to be re-read.
  const name = detail ? `${label} · ${detail}` : label;

  return (
    <button
      type="button"
      className={['trash-action', className].filter(Boolean).join(' ')}
      onClick={onClick}
      disabled={disabled}
      aria-expanded={expanded}
      aria-label={name}
      /* A tooltip on an enabled control would fight the lid animation that just
         answered the hover, and it would repeat the word already on screen. The
         one case that needs words is a blocked action: its reason has nowhere
         else to live. */
      title={disabled ? disabledReason ?? name : undefined}
    >
      <span className="trash-action-glyph">
        <TrashGlyph size={16} />
      </span>
      <span className="trash-action-label" aria-hidden="true">
        {label}
      </span>
    </button>
  );
}
