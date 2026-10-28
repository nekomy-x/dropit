/**
 * NavDrawer — the mobile navigation layer for the DROP IT panels.
 *
 * Why it exists
 * -------------
 * Below 1024px the rail is gone. It used to be replaced by a fixed bottom bar
 * of five destinations, which cost the product twice: the bar permanently
 * occupied the bottom edge (where the phone puts its own gestures and where
 * the deployment strip already sits), and five slots could not name the real
 * navigation — the club panel has seven destinations and the admin panel has
 * seventeen. Half the product was unreachable on a phone, and the half that
 * remained had to be abbreviated to "Room", "Price", "Income".
 *
 * The drawer replaces that bar. One labelled button in the top bar opens a
 * panel that carries the SAME groups the rail carries, with the same labels,
 * the same order and the same live badges. Nothing is abbreviated, nothing is
 * dropped, and the bottom edge of the screen stays free.
 *
 * Contract
 * --------
 *   - The trigger is `NavDrawerButton`: a real button with an accessible name,
 *     `aria-expanded`, `aria-controls`. It is the only way in from the top bar.
 *   - The panel is a modal surface built on the native `<dialog>`, so focus
 *     containment, Escape, the backdrop and focus return are the browser's job
 *     and not a reimplementation (`packages/ui/modal.tsx` makes the same call).
 *   - Escape closes and returns focus to the trigger; a tap on the backdrop
 *     closes; a tap inside never does.
 *   - Choosing a destination closes the drawer. That is the only navigation
 *     action the panel performs; it does not know about routing, so both
 *     `NavLink` and `button` destinations work.
 *   - The body scroll is locked while open, and the previous overflow is
 *     restored on close and on unmount (`dialog[open]` already blocks the page,
 *     but iOS still rubber-bands the body behind it).
 *   - Under `prefers-reduced-motion` the panel appears without a transition.
 *
 * The panel is a full-height edge sheet below 1024px and never renders at all
 * at desktop widths: the rail is present there, and two ways to reach the same
 * seventeen destinations would be one too many.
 */
import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from 'react';
import Icon from './components/Icon';

export interface NavDrawerProps {
  /** Accessible name of the panel: "Main navigation", "Admin navigation". */
  label: string;
  /** Label of the trigger button; also its title on hover. */
  triggerLabel: string;
  /** Everything inside the sheet after the title row. */
  children: ReactNode;
  /** Optional line under the title: the workspace this navigation belongs to. */
  subtitle?: string;
  /** Rendered opposite the title, e.g. the theme switch. */
  actions?: ReactNode;
  /** Extra class on the trigger, so an app can position it in its top bar. */
  className?: string;
}

export function NavDrawer({ label, triggerLabel, children, subtitle, actions, className = '' }: NavDrawerProps) {
  const [open, setOpen] = useState(false);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const titleId = useId();

  const close = useCallback(() => setOpen(false), []);

  // `showModal()`/`close()` are imperative on the element; React only decides
  // when the dialog exists. Keeping them in one effect means the element and
  // the state can never disagree (a dialog left open after an unmount traps
  // focus in a detached node).
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!open || !dialog) return undefined;
    const previous = document.activeElement;
    const { body } = document;
    const previousOverflow = body.style.overflow;
    const previousPadding = body.style.paddingRight;
    // Reserve the scrollbar so locking the body does not shift the page.
    const scrollbar = window.innerWidth - document.documentElement.clientWidth;
    body.style.overflow = 'hidden';
    if (scrollbar > 0) body.style.paddingRight = `${scrollbar}px`;
    if (!dialog.open) dialog.showModal();
    return () => {
      body.style.overflow = previousOverflow;
      body.style.paddingRight = previousPadding;
      if (dialog.open) dialog.close();
      if (previous instanceof HTMLElement) previous.focus({ preventScroll: true });
    };
  }, [open]);

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className={`panel-icon-button panel-nav-trigger ${className}`.trim()}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={triggerLabel}
        title={triggerLabel}
        onClick={() => setOpen(true)}
      >
        <Icon name="lucide:menu" size={18} />
      </button>

      {open ? (
        <dialog
          ref={dialogRef}
          className="panel-nav-drawer"
          aria-labelledby={titleId}
          // `cancel` fires on Escape and on a device "back" gesture; closing
          // through React keeps one source of truth for the open state.
          onCancel={(event) => {
            event.preventDefault();
            close();
          }}
          onClick={(event) => {
            // A tap on the dialog element itself is a tap on the backdrop: the
            // sheet is a child, so a tap inside has a different target.
            if (event.target === event.currentTarget) close();
          }}
        >
          <div className="panel-nav-sheet">
            <header className="panel-nav-sheet-head">
              <div className="panel-nav-sheet-heading">
                <h2 id={titleId} className="panel-nav-sheet-title">{label}</h2>
                {subtitle ? <p className="panel-nav-sheet-subtitle">{subtitle}</p> : null}
              </div>
              {actions ? <div className="panel-nav-sheet-actions">{actions}</div> : null}
              <button
                type="button"
                className="panel-icon-button"
                onClick={close}
                aria-label={label}
                title={label}
              >
                <Icon name="lucide:x" size={18} />
              </button>
            </header>
            {/* `onClick` captures the choice made in the list without the drawer
                owning a router; any element with `data-nav-close` closes it. */}
            <div className="panel-nav-sheet-body" onClick={(event) => {
              if ((event.target as HTMLElement).closest('[data-nav-close]')) close();
            }}>
              {children}
            </div>
          </div>
        </dialog>
      ) : null}
    </>
  );
}
