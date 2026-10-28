import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import Icon from './Icon';

export interface DropdownOption {
  value: string;
  label: string;
  description?: string;
  icon?: `lucide:${string}`;
  disabled?: boolean;
}

interface DropdownButtonProps {
  label: string;
  value: string;
  options: readonly DropdownOption[];
  onChange: (value: string) => void;
  className?: string;
  menuLabel?: string;
  align?: 'start' | 'end';
}

/**
 * Button-led selection menu for operator surfaces. The trigger keeps the
 * current choice visible, while the menu owns keyboard navigation and closing
 * behavior so callers only provide options and a value.
 */
export default function DropdownButton({
  label,
  value,
  options,
  onChange,
  className = '',
  menuLabel,
  align = 'start',
}: DropdownButtonProps) {
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(() => Math.max(0, options.findIndex((option) => option.value === value)));
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuId = useId();
  const selected = options.find((option) => option.value === value);

  useEffect(() => {
    if (!open) return undefined;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        setOpen(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open]);

  const choose = (option: DropdownOption) => {
    if (option.disabled) return;
    onChange(option.value);
    setOpen(false);
    triggerRef.current?.focus();
  };

  const move = (direction: 1 | -1) => {
    const enabled = options.map((option, index) => ({ option, index })).filter(({ option }) => !option.disabled);
    if (enabled.length === 0) return;
    const currentPosition = Math.max(0, enabled.findIndex(({ index }) => index === activeIndex));
    const next = enabled[(currentPosition + direction + enabled.length) % enabled.length];
    if (next) setActiveIndex(next.index);
  };

  const onTriggerKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === 'ArrowDown' || event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      setActiveIndex(Math.max(0, options.findIndex((option) => option.value === value)));
      setOpen(true);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setActiveIndex(Math.max(0, options.findIndex((option) => option.value === value)));
      setOpen(true);
    }
  };

  const onMenuKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      move(1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      move(-1);
    } else if (event.key === 'Home') {
      event.preventDefault();
      const first = options.findIndex((option) => !option.disabled);
      if (first >= 0) setActiveIndex(first);
    } else if (event.key === 'End') {
      event.preventDefault();
      let last = -1;
      for (let index = options.length - 1; index >= 0; index -= 1) {
        if (!options[index]?.disabled) {
          last = index;
          break;
        }
      }
      if (last >= 0) setActiveIndex(last);
    } else if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      const option = options[activeIndex];
      if (option) choose(option);
    }
  };

  return (
    <div ref={rootRef} className={`admin-dropdown ${open ? 'is-open' : ''} ${className}`}>
      <button
        ref={triggerRef}
        type="button"
        className="admin-dropdown__trigger"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={menuId}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={onTriggerKeyDown}
      >
        <span className="admin-dropdown__trigger-copy">
          <span className="admin-dropdown__label">{label}</span>
          <span className="admin-dropdown__value">
            {selected?.icon ? <Icon name={selected.icon} size={14} /> : null}
            {selected?.label ?? value}
          </span>
        </span>
        <Icon name="lucide:chevron-down" size={16} className="admin-dropdown__chevron" />
      </button>
      {open ? (
        <div
          id={menuId}
          className={`admin-dropdown__menu admin-dropdown__menu--${align}`}
          role="menu"
          aria-label={menuLabel ?? label}
          onKeyDown={onMenuKeyDown}
        >
          {options.map((option, index) => (
            <button
              key={option.value}
              type="button"
              role="menuitemradio"
              aria-checked={option.value === value}
              disabled={option.disabled}
              className={`admin-dropdown__option ${option.value === value ? 'is-selected' : ''} ${index === activeIndex ? 'is-active' : ''}`}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={() => choose(option)}
            >
              <span className="admin-dropdown__option-icon">
                {option.icon ? <Icon name={option.icon} size={15} /> : null}
                {option.value === value ? <Icon name="lucide:check" size={14} /> : null}
              </span>
              <span className="admin-dropdown__option-copy">
                <span>{option.label}</span>
                {option.description ? <small>{option.description}</small> : null}
              </span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
