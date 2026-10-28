/**
 * Menu — the one dropdown implementation for the DROP IT panels.
 *
 * Every menu (the account menu, and any future one) is this component, so the
 * keyboard contract, the closing rules and the focus return are identical
 * everywhere:
 *
 *   Enter / Space / ArrowDown on the trigger   opens, focus goes to item 1
 *   ArrowUp on the trigger                     opens, focus goes to the last item
 *   ArrowDown / ArrowUp inside the panel       moves the highlight
 *   Home / End                                 first / last item
 *   Enter / Space on an item                   runs it, closes, returns focus
 *   Escape                                     closes, returns focus
 *   Tab                                        closes and lets focus move on
 *   pointer down outside                       closes
 *
 * The panel is `role="menu"`; items are real buttons with `menuitem` or
 * `menuitemradio` roles, so screen readers announce the checked state of the
 * language and theme choices without extra live regions.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from 'react';
import Icon from './components/Icon';

export type MenuPlacement = 'bottom-start' | 'bottom-end' | 'top-start' | 'top-end';

interface MenuContextValue {
  close: () => void;
}

const MenuContext = createContext<MenuContextValue | null>(null);

export interface MenuTriggerProps {
  ref: RefObject<HTMLButtonElement>;
  type: 'button';
  'aria-haspopup': 'menu';
  'aria-expanded': boolean;
  'aria-controls': string;
  onClick: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => void;
}

interface MenuProps {
  /** Accessible name of the menu itself. */
  label: string;
  placement?: MenuPlacement;
  /** Receives everything the trigger button needs; the caller owns the look. */
  trigger: (props: MenuTriggerProps) => ReactNode;
  children: ReactNode;
  /** Extra class on the root, so a caller can position the menu in its shell. */
  className?: string;
}

const ITEM_SELECTOR = '[data-menu-item]:not([disabled])';

export function Menu({ label, placement = 'bottom-start', trigger, children, className = '' }: MenuProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const menuId = useId();

  const close = useCallback(() => {
    setOpen(false);
    triggerRef.current?.focus({ preventScroll: true });
  }, []);

  const items = useCallback(
    () => Array.from(panelRef.current?.querySelectorAll<HTMLButtonElement>(ITEM_SELECTOR) ?? []),
    [],
  );

  const focusItem = (index: number) => {
    const nodes = items();
    if (nodes.length === 0) return;
    const bounded = ((index % nodes.length) + nodes.length) % nodes.length;
    nodes[bounded]?.focus({ preventScroll: true });
  };

  // Opening always moves focus into the panel: a menu that opens under the
  // cursor but leaves focus on the trigger is invisible to keyboard users.
  useEffect(() => {
    if (!open) return;
    const frame = requestAnimationFrame(() => focusItem(0));
    return () => cancelAnimationFrame(frame);
  }, [open]);

  useEffect(() => {
    if (!open) return undefined;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', onPointerDown);
    return () => document.removeEventListener('pointerdown', onPointerDown);
  }, [open]);

  const onPanelKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const nodes = items();
    const current = nodes.indexOf(document.activeElement as HTMLButtonElement);
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        focusItem(current + 1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        focusItem(current < 0 ? nodes.length - 1 : current - 1);
        break;
      case 'Home':
        event.preventDefault();
        focusItem(0);
        break;
      case 'End':
        event.preventDefault();
        focusItem(nodes.length - 1);
        break;
      case 'Escape':
        event.preventDefault();
        event.stopPropagation();
        close();
        break;
      case 'Tab':
        setOpen(false);
        break;
      default:
        break;
    }
  };

  const openAt = (edge: 'first' | 'last') => {
    setOpen(true);
    requestAnimationFrame(() => {
      const nodes = items();
      if (nodes.length === 0) return;
      nodes[edge === 'first' ? 0 : nodes.length - 1]?.focus({ preventScroll: true });
    });
  };

  const triggerProps: MenuTriggerProps = {
    ref: triggerRef,
    type: 'button',
    'aria-haspopup': 'menu',
    'aria-expanded': open,
    'aria-controls': menuId,
    onClick: () => (open ? setOpen(false) : openAt('first')),
    onKeyDown: (event) => {
      if (event.key === 'ArrowDown' || event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        openAt('first');
      } else if (event.key === 'ArrowUp') {
        event.preventDefault();
        openAt('last');
      } else if (event.key === 'Escape' && open) {
        event.preventDefault();
        close();
      }
    },
  };

  return (
    <MenuContext.Provider value={{ close }}>
      <div ref={rootRef} className={`panel-menu ${className}`.trim()}>
        {trigger(triggerProps)}
        {open ? (
          <div
            ref={panelRef}
            id={menuId}
            role="menu"
            aria-label={label}
            className={`panel-menu-panel panel-menu-panel--${placement}`}
            onKeyDown={onPanelKeyDown}
          >
            {children}
          </div>
        ) : null}
      </div>
    </MenuContext.Provider>
  );
}

interface MenuItemProps {
  label: string;
  onSelect: () => void;
  icon?: `lucide:${string}`;
  /** Second line; keep it to one short clause. */
  description?: string;
  role?: 'menuitem' | 'menuitemradio';
  checked?: boolean;
  /** `danger` is reserved for signing out. */
  tone?: 'default' | 'danger';
  /** Renders before the icon: the language flag or a status dot. */
  marker?: ReactNode;
}

export function MenuItem({
  label,
  onSelect,
  icon,
  description,
  role = 'menuitem',
  checked,
  tone = 'default',
  marker,
}: MenuItemProps) {
  const context = useContext(MenuContext);
  return (
    <button
      type="button"
      role={role}
      // `aria-checked` only exists on menu item radios; passing it on a plain
      // menuitem is an ARIA violation, so it is conditional by role.
      {...(role === 'menuitemradio' ? { 'aria-checked': Boolean(checked) } : {})}
      data-menu-item
      data-tone={tone}
      className={`panel-menu-item ${checked ? 'is-checked' : ''}`.trim()}
      onClick={() => {
        onSelect();
        context?.close();
      }}
    >
      <span className="panel-menu-item-icon" aria-hidden="true">
        {marker ?? (icon ? <Icon name={icon} size={16} /> : null)}
      </span>
      <span className="panel-menu-item-copy">
        <span className="panel-menu-item-label">{label}</span>
        {description ? <span className="panel-menu-item-description">{description}</span> : null}
      </span>
      {role === 'menuitemradio' || checked ? (
        <span className="panel-menu-item-check" aria-hidden="true">
          {checked ? <Icon name="lucide:check" size={15} /> : null}
        </span>
      ) : null}
    </button>
  );
}

export function MenuGroupLabel({ children }: { children: ReactNode }) {
  return <p className="panel-menu-group-label">{children}</p>;
}

export function MenuSeparator() {
  // A separator inside a menu is presentational; `role="separator"` on a div is
  // valid and announced, but the hairline already carries the meaning visually.
  return <div className="panel-menu-separator" role="separator" />;
}
