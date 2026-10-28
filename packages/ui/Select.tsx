/**
 * Select — the one combobox/dropdown control of the panel kit.
 *
 * Why this exists and not a native `<select>`: the native popup is drawn by the
 * OS, so its surface, row height and density can never match the product, and
 * its height is not controllable at all. Every dropdown across the panels
 * renders here so the input, the height of the open layer and its behaviour on
 * any screen come from one place.
 *
 * Geometry contract
 * -----------------
 * * The trigger is exactly the field: the same 1px rule under the value and the
 *   same height as `.field-input` (38px desktop, 44px touch), so a select sits
 *   on one baseline with a text input in a form grid or a toolbar.
 * * The list opens **directly under the trigger** — the menu is `position: fixed`,
 *   measured against the trigger rect, so it is never clipped by a scrolling
 *   table or an overflow container. When the viewport has no room below the
 *   field, the layer flips above it; when even that is short, the menu caps its
 *   own height and scrolls inside.
 * * Width is the trigger width at minimum (a select never opens narrower than
 *   its own field), capped at 460px and always inside the viewport. A short
 *   toolbar field (168px) therefore opens a 180px list — wide enough for the
 *   longest label — instead of an 80px column of ellipses.
 *
 * Accessibility
 * -------------
 * A button/combobox + listbox popup (the ARIA "select-only combobox"): focus
 * stays on the trigger, options are exposed through `aria-activedescendant`,
 * arrow keys move, Enter selects, Escape/Tab closes. Never a menu/`role="menu"`:
 * this is picking a value, not performing an action.
 *
 * Note on the import paths: this file is byte-copied into
 * `apps/<app>/src/components/ui/`, so `../Icon` resolves to that app's own
 * copy of the shared icon surface (same convention as StatCard.tsx).
 */
import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from 'react';
import { createPortal } from 'react-dom';
import Icon from '../Icon';

export interface SelectOption {
  /** Stored value. Requested from the caller, never derived from a label. */
  value: string;
  label: string;
  /** Secondary line: an email under a DJ name, a count under a club. */
  description?: string;
  disabled?: boolean;
}

export type SelectSize = 'md' | 'sm';

export interface SelectProps {
  options: readonly SelectOption[];
  value: string;
  onChange: (value: string) => void;
  /** Visible label rendered above the field (`.field-label`). */
  label?: string;
  /** Id of an external label element, when the label lives in page markup. */
  labelledBy?: string;
  ariaLabel?: string;
  /** Shown while `value` matches no option (empty lists, loading states). */
  placeholder?: string;
  id?: string;
  size?: SelectSize;
  disabled?: boolean;
  invalid?: boolean;
  /** Extra class on the root. */
  className?: string;
  /**
   * Extra class on the floating list. The list is portalled to <body>, so a
   * caller that re-skins the control for its own palette (the public Connect
   * flow) must pass its menu class here — an ancestor selector cannot reach it.
   */
  menuClassName?: string;
  /** Force the side; default flips up when the space below is short. */
  menuPlacement?: 'auto' | 'below' | 'above';
}

const GAP = 6;                 /* trigger → menu */
const VIEWPORT_MARGIN = 8;
const MIN_MENU_HEIGHT = 120;
const MAX_MENU_HEIGHT = 340;
const MIN_MENU_WIDTH = 180;
const MAX_MENU_WIDTH = 460;
const ROW_ESTIMATE = 44;

type Side = 'below' | 'above';

interface Rect {
  top: number;
  bottom: number;
  left: number;
  width: number;
}

const firstEnabled = (options: readonly SelectOption[]) => {
  const index = options.findIndex((option) => !option.disabled);
  return index < 0 ? 0 : index;
};

const lastEnabled = (options: readonly SelectOption[]) => {
  for (let index = options.length - 1; index >= 0; index -= 1) {
    if (!options[index]?.disabled) return index;
  }
  return 0;
};

export default function Select({
  options,
  value,
  onChange,
  label,
  labelledBy,
  ariaLabel,
  placeholder,
  id,
  size = 'md',
  disabled = false,
  invalid = false,
  className = '',
  menuClassName = '',
  menuPlacement = 'auto',
}: SelectProps) {
  const rootId = useId();
  const triggerId = id ?? rootId;
  const menuId = `${triggerId}-menu`;
  const labelId = `${triggerId}-label`;
  const optionDomId = useCallback((index: number) => `${triggerId}-opt-${index}`, [triggerId]);

  const [portalTarget, setPortalTarget] = useState<HTMLElement | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLUListElement>(null);

  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const [rect, setRect] = useState<Rect | null>(null);
  const [menuHeight, setMenuHeight] = useState<number | null>(null);
  const typeahead = useRef({ buffer: '', timer: 0 });

  const selectedIndex = useMemo(() => {
    const index = options.findIndex((option) => option.value === value);
    return index < 0 ? -1 : index;
  }, [options, value]);
  const selected = selectedIndex >= 0 ? options[selectedIndex] : undefined;

  /* --- geometry -------------------------------------------------------- */
  const readRect = useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;
    const box = trigger.getBoundingClientRect();
    setRect({ top: box.top, bottom: box.bottom, left: box.left, width: box.width });
  }, []);

  const geometry = useMemo(() => {
    if (!rect) return null;
    const viewportW = window.innerWidth;
    const viewportH = window.innerHeight;
    const spaceBelow = viewportH - rect.bottom - GAP - VIEWPORT_MARGIN;
    const spaceAbove = rect.top - GAP - VIEWPORT_MARGIN;
    let side: Side = 'below';
    if (menuPlacement === 'above') side = 'above';
    else if (menuPlacement === 'auto' && spaceBelow < MIN_MENU_HEIGHT && spaceAbove > spaceBelow) side = 'above';

    const available = Math.max(0, side === 'below' ? spaceBelow : spaceAbove);
    const cap = Math.min(MAX_MENU_HEIGHT, Math.round(viewportH * 0.55));
    const maxHeight = Math.max(MIN_MENU_HEIGHT, Math.min(cap, available));

    const width = Math.max(
      Math.min(Math.max(rect.width, MIN_MENU_WIDTH), MAX_MENU_WIDTH, viewportW - VIEWPORT_MARGIN * 2),
      MIN_MENU_WIDTH,
    );
    const left = Math.min(Math.max(VIEWPORT_MARGIN, rect.left), Math.max(VIEWPORT_MARGIN, viewportW - VIEWPORT_MARGIN - width));

    const estimated = Math.min(options.length * ROW_ESTIMATE + 12, maxHeight);
    const height = Math.min(menuHeight ?? estimated, maxHeight);
    const top = side === 'below' ? rect.bottom + GAP : Math.max(VIEWPORT_MARGIN, rect.top - GAP - height);
    return { side, top, left, width, maxHeight };
  }, [rect, menuHeight, menuPlacement, options.length]);

  const openMenu = useCallback(() => {
    // A native <dialog> renders in the top layer: a menu portalled to <body>
    // would paint under it, so the portal follows the trigger into the dialog.
    const dialog = triggerRef.current?.closest('dialog');
    setPortalTarget(dialog instanceof HTMLElement ? dialog : null);
    readRect();
    setMenuHeight(null);
    setOpen(true);
  }, [readRect]);

  useLayoutEffect(() => {
    if (!open) return undefined;
    const menu = menuRef.current;
    if (!menu) return undefined;
    const update = () => setMenuHeight(menu.getBoundingClientRect().height);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(menu);
    return () => observer.disconnect();
  }, [open, geometry?.maxHeight]);

  useLayoutEffect(() => {
    if (!open) return undefined;
    const follow = () => {
      const trigger = triggerRef.current;
      if (!trigger) return;
      const box = trigger.getBoundingClientRect();
      if (box.bottom < 0 || box.top > window.innerHeight) {
        setOpen(false);
        return;
      }
      setRect({ top: box.top, bottom: box.bottom, left: box.left, width: box.width });
    };
    window.addEventListener('scroll', follow, true);
    window.addEventListener('resize', follow);
    return () => {
      window.removeEventListener('scroll', follow, true);
      window.removeEventListener('resize', follow);
    };
  }, [open]);

  /* --- open/close behaviour -------------------------------------------- */
  useEffect(() => {
    if (!open) return undefined;
    const onPointerDown = (event: PointerEvent) => {
      const node = event.target;
      if (!(node instanceof Node)) return;
      if (rootRef.current?.contains(node) || menuRef.current?.contains(node)) return;
      setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
      triggerRef.current?.focus();
    };
    document.addEventListener('pointerdown', onPointerDown, true);
    document.addEventListener('keydown', onKeyDown, true);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true);
      document.removeEventListener('keydown', onKeyDown, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    setActiveIndex(selectedIndex >= 0 && !options[selectedIndex]?.disabled ? selectedIndex : firstEnabled(options));
  }, [open, options, selectedIndex]);

  /* --- selection -------------------------------------------------------- */
  const choose = useCallback(
    (index: number) => {
      const option = options[index];
      if (!option || option.disabled) return;
      onChange(option.value);
      setOpen(false);
      triggerRef.current?.focus();
    },
    [onChange, options],
  );

  const move = useCallback(
    (direction: 1 | -1) => {
      setActiveIndex((current) => {
        let next = current;
        for (let step = 0; step < options.length; step += 1) {
          next = (next + direction + options.length) % options.length;
          if (!options[next]?.disabled) return next;
        }
        return current;
      });
    },
    [options],
  );

  const onTriggerKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    const { key } = event;
    if (key === 'ArrowDown' || key === 'ArrowUp') {
      event.preventDefault();
      if (!open) {
        openMenu();
        return;
      }
      move(key === 'ArrowDown' ? 1 : -1);
      return;
    }
    if (key === 'Home' && open) {
      event.preventDefault();
      setActiveIndex(firstEnabled(options));
      return;
    }
    if (key === 'End' && open) {
      event.preventDefault();
      setActiveIndex(lastEnabled(options));
      return;
    }
    if ((key === 'Enter' || key === ' ') && open) {
      event.preventDefault();
      choose(activeIndex);
      return;
    }
    if (key === 'Tab') {
      setOpen(false);
      return;
    }
    if (key.length === 1 && !event.altKey && !event.ctrlKey && !event.metaKey) {
      if (!open) openMenu();
      const buffer = typeahead.current.buffer + key.toLowerCase();
      typeahead.current.buffer = buffer;
      window.clearTimeout(typeahead.current.timer);
      typeahead.current.timer = window.setTimeout(() => { typeahead.current.buffer = ''; }, 450);
      for (let offset = 0; offset < options.length; offset += 1) {
        const index = (activeIndex + offset) % options.length;
        const option = options[index];
        if (option && !option.disabled && option.label.toLowerCase().startsWith(buffer)) {
          setActiveIndex(index);
          return;
        }
      }
    }
  };

  useLayoutEffect(() => {
    if (!open) return;
    menuRef.current?.querySelector<HTMLElement>(`[id="${optionDomId(activeIndex)}"]`)?.scrollIntoView({ block: 'nearest' });
  }, [open, activeIndex, optionDomId]);

  const rootClass = [
    'field-select',
    size === 'sm' ? 'field-select--sm' : '',
    invalid ? 'field-select--invalid' : '',
    className,
  ].filter(Boolean).join(' ');

  return (
    <div ref={rootRef} className={rootClass}>
      {label ? (
        <label htmlFor={triggerId} id={labelId} className="field-label">
          {label}
        </label>
      ) : null}

      <button
        ref={triggerRef}
        id={triggerId}
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-labelledby={labelledBy}
        aria-label={ariaLabel}
        aria-invalid={invalid || undefined}
        aria-activedescendant={open && options[activeIndex] ? optionDomId(activeIndex) : undefined}
        disabled={disabled}
        onClick={() => { if (open) setOpen(false); else openMenu(); }}
        onKeyDown={onTriggerKeyDown}
        className="field-select__trigger"
      >
        <span className={`field-select__value${selected ? '' : ' is-placeholder'}`}>
          {selected ? selected.label : placeholder ?? 'Select'}
        </span>
        <Icon name="lucide:chevron-down" size={15} className="field-select__chevron" />
      </button>

      {open && geometry
        ? createPortal(
            <ul
              ref={menuRef}
              id={menuId}
              role="listbox"
              aria-labelledby={labelledBy ?? (label ? labelId : undefined)}
              aria-label={ariaLabel}
              className={`field-select__menu${menuClassName ? ` ${menuClassName}` : ''}`}
              data-side={geometry.side}
              style={{
                position: 'fixed',
                top: geometry.top,
                left: geometry.left,
                width: geometry.width,
                maxHeight: geometry.maxHeight,
              }}
            >
              {options.length === 0 ? (
                <li className="field-select__empty" role="presentation">{placeholder ?? 'No options'}</li>
              ) : (
                options.map((option, index) => {
                  const isActive = index === activeIndex;
                  const isSelected = index === selectedIndex;
                  return (
                    <li
                      key={`${option.value}-${index}`}
                      id={optionDomId(index)}
                      role="option"
                      aria-selected={isSelected}
                      aria-disabled={option.disabled || undefined}
                      onMouseMove={() => { if (!isActive) setActiveIndex(index); }}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => choose(index)}
                      className={[
                        'field-select__option',
                        isActive ? 'is-active' : '',
                        isSelected ? 'is-selected' : '',
                      ].filter(Boolean).join(' ')}
                    >
                      <span className="field-select__option-copy">
                        <span className="field-select__option-label">{option.label}</span>
                        {option.description ? <span className="field-select__option-desc">{option.description}</span> : null}
                      </span>
                      {isSelected ? (
                        <span className="field-select__option-check" aria-hidden="true">
                          <Icon name="lucide:check" size={15} />
                        </span>
                      ) : null}
                    </li>
                  );
                })
              )}
            </ul>,
            portalTarget ?? document.body,
          )
        : null}
    </div>
  );
}
