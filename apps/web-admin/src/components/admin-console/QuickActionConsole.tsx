import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import Icon from '../Icon';
import { useLocale } from '../../locale';

export interface QuickAction {
  id: string;
  label: string;
  description: string;
  run: () => void;
}
export interface QuickActionConsoleProps {
  actions?: QuickAction[];
  /** Convenience adapter for callers that dispatch actions by id. */
  onAction?: (actionId: string) => void;
}

/**
 * Quick action descriptors.
 *
 * The counts in the descriptions ("3 awaiting review", "1 critical event") are
 * the local preview's own copy, kept exactly as the previous console had them so
 * the panel does not start asserting a figure it cannot read from the store.
 */
const ACTION_DESCRIPTORS: ReadonlyArray<{ id: string; labelKey: string; descriptionKey: string }> = [
  { id: 'review', labelKey: 'console.actionReviewLabel', descriptionKey: 'console.actionReviewDescription' },
  { id: 'suspend', labelKey: 'console.actionSuspendLabel', descriptionKey: 'console.actionSuspendDescription' },
  { id: 'audit', labelKey: 'console.actionAuditLabel', descriptionKey: 'console.actionAuditDescription' },
  { id: 'role', labelKey: 'console.actionRoleLabel', descriptionKey: 'console.actionRoleDescription' },
];
const FOCUSABLE_SELECTOR = 'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

/**
 * Command palette for operator shortcuts (⌘K / Ctrl-K).
 *
 * Behaviour is unchanged: the shortcut toggles it, Arrow Up/Down move the
 * highlighted action, Enter runs it, Escape closes it, and focus returns to the
 * trigger. Presentation is the shared panel language — one dialog surface, a
 * search row, and list options whose active row is raised, not outlined.
 */
export default function QuickActionConsole({ actions: providedActions, onAction }: QuickActionConsoleProps) {
  const { t } = useLocale();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const actions = useMemo<QuickAction[]>(() => providedActions ?? (onAction ? ACTION_DESCRIPTORS.map((action) => ({ id: action.id, label: t(action.labelKey), description: t(action.descriptionKey), run: () => onAction(action.id) })) : []), [onAction, providedActions, t]);
  const filteredActions = useMemo(() => actions.filter((action) => `${action.label} ${action.description}`.toLowerCase().includes(query.toLowerCase())), [actions, query]);

  const restoreFocus = () => {
    const target = previousFocusRef.current ?? triggerRef.current;
    if (target?.isConnected) requestAnimationFrame(() => target.focus());
  };
  const closeConsole = () => {
    setOpen(false);
    restoreFocus();
  };
  const openConsole = () => {
    previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : triggerRef.current;
    setQuery('');
    setActiveIndex(0);
    setOpen(true);
  };

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        if (open) closeConsole(); else openConsole();
      }
    };
    window.addEventListener('keydown', handleShortcut);
    return () => window.removeEventListener('keydown', handleShortcut);
  });

  useEffect(() => {
    if (!open) return;
    requestAnimationFrame(() => inputRef.current?.focus());
    const handleDialogKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        closeConsole();
        return;
      }
      if (event.key !== 'Tab' || !dialogRef.current) return;
      const focusable = Array.from(dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    document.addEventListener('keydown', handleDialogKeyDown);
    return () => document.removeEventListener('keydown', handleDialogKeyDown);
  }, [open]);

  useEffect(() => { setActiveIndex(0); }, [query]);

  const execute = (action: QuickAction) => {
    action.run();
    closeConsole();
  };
  const handleInputKey = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setActiveIndex((index) => filteredActions.length === 0 ? 0 : Math.min(index + 1, filteredActions.length - 1));
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      setActiveIndex((index) => Math.max(index - 1, 0));
    }
    if (event.key === 'Enter' && filteredActions[activeIndex]) execute(filteredActions[activeIndex]);
  };

  return (
    <>
      <button ref={triggerRef} type="button" onClick={openConsole} className="btn btn-secondary" aria-haspopup="dialog" aria-expanded={open}>
        <Icon name="lucide:command" size={15} />
        <span>{t('console.quickActions')}</span>
        <kbd
          aria-hidden="true"
          style={{
            paddingLeft: 8,
            borderLeft: '1px solid var(--c-hairline-strong)',
            color: 'var(--c-text-muted-sm)',
            fontFamily: 'var(--font-mono)',
            fontSize: 'var(--fs-11)',
          }}
        >
          ⌘K
        </kbd>
      </button>

      {open && (
        <div className="admin-palette-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) closeConsole(); }}>
          <section ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="quick-action-heading" className="admin-palette">
            <h2 id="quick-action-heading" className="sr-only">{t('console.quickActionTitle')}</h2>
            <div className="admin-palette-search">
              <Icon name="lucide:search" size={16} />
              <input
                ref={inputRef}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                onKeyDown={handleInputKey}
                placeholder={t('console.searchActions')}
                aria-label={t('console.searchActionsLabel')}
              />
              <kbd>ESC</kbd>
            </div>
            <ul className="admin-palette-list">
              {filteredActions.map((action, index) => (
                <li key={action.id}>
                  <button
                    type="button"
                    onMouseEnter={() => setActiveIndex(index)}
                    onClick={() => execute(action)}
                    className={`admin-palette-option ${index === activeIndex ? 'is-active' : ''}`.trim()}
                  >
                    <span style={{ minWidth: 0 }}>
                      <span style={{ display: 'block', color: 'var(--c-text)', fontSize: 'var(--fs-13)' }}>{action.label}</span>
                      <small>{action.description}</small>
                    </span>
                    <span aria-hidden="true" style={{ color: 'var(--c-text-muted-sm)', fontSize: 'var(--fs-11)' }}>↵</span>
                  </button>
                </li>
              ))}
              {filteredActions.length === 0 && <li className="admin-palette-empty">{t('console.noActions')}</li>}
            </ul>
            <footer className="admin-palette-foot">
              {t('console.paletteKeys')}
            </footer>
          </section>
        </div>
      )}
    </>
  );
}
