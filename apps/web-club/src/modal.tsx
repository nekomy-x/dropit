/**
 * Modal — the one dialog for the DROP IT panels.
 *
 * Built on the native `<dialog>` element, so the browser provides what a
 * hand-rolled overlay usually gets wrong: focus containment, Escape, the
 * backdrop that swallows clicks on the page behind, and focus returning to the
 * control that opened it. The component only owns the look (a `panel` surface,
 * one title row, a scrolling body and a footer for actions).
 *
 * Rules this component enforces:
 *   - the title is the dialog's accessible name (`aria-labelledby`);
 *   - the close control is a real labelled button, not an unexplained icon;
 *   - the body scrolls, the header and footer do not, so the save button is
 *     always in reach on a short screen;
 *   - Escape and the backdrop both close, but a click inside never does.
 */
import { useEffect, useId, useRef, type ReactNode } from 'react';
import Icon from './components/Icon';

export interface ModalProps {
  title: string;
  /** One line under the title: what this dialog is for. */
  description?: string;
  onClose: () => void;
  closeLabel: string;
  children: ReactNode;
  /** Right-aligned actions; the primary action goes last. */
  footer?: ReactNode;
  size?: 'default' | 'wide';
}

export function Modal({ title, description, onClose, closeLabel, children, footer, size = 'default' }: ModalProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog || dialog.open) return undefined;
    const previous = document.activeElement;
    dialog.showModal();
    return () => {
      dialog.close();
      if (previous instanceof HTMLElement) previous.focus({ preventScroll: true });
    };
  }, []);

  return (
    <dialog
      ref={ref}
      className={`panel-modal ${size === 'wide' ? 'panel-modal-wide' : ''}`.trim()}
      aria-labelledby={titleId}
      aria-describedby={description ? descriptionId : undefined}
      // `cancel` fires on Escape; closing through React keeps one source of truth.
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="panel-modal-surface">
        <header className="panel-modal-head">
          <div className="panel-modal-heading">
            <h2 id={titleId} className="panel-modal-title">{title}</h2>
            {description ? <p id={descriptionId} className="panel-modal-description">{description}</p> : null}
          </div>
          <button type="button" className="panel-icon-button" onClick={onClose} aria-label={closeLabel}>
            <Icon name="lucide:x" size={16} />
          </button>
        </header>
        <div className="panel-modal-body">{children}</div>
        {footer ? <footer className="panel-modal-foot">{footer}</footer> : null}
      </div>
    </dialog>
  );
}
