/**
 * Minimal focus trap for the dashboard dialogs.
 *
 * A dialog that lets Tab wander into the page behind it is not a dialog for
 * keyboard and screen-reader users. This hook keeps focus inside the container
 * while it is mounted and returns focus to the element that opened it, which is
 * the behaviour every other dialog on the panel is expected to match later.
 */
import { useEffect, useRef } from 'react';

const FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

export function useFocusTrap<T extends HTMLElement>(active: boolean) {
  const containerRef = useRef<T | null>(null);

  useEffect(() => {
    if (!active) return;
    const container = containerRef.current;
    if (!container) return;

    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const focusable = () => Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE))
      .filter((node) => node.offsetParent !== null || node === document.activeElement);

    const first = focusable()[0];
    (first ?? container).focus({ preventScroll: true });

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return;
      const nodes = focusable();
      if (nodes.length === 0) {
        event.preventDefault();
        return;
      }
      const head = nodes[0];
      const tail = nodes[nodes.length - 1];
      if (!head || !tail) return;
      if (event.shiftKey && document.activeElement === head) {
        event.preventDefault();
        tail.focus();
      } else if (!event.shiftKey && document.activeElement === tail) {
        event.preventDefault();
        head.focus();
      }
    };

    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      previous?.focus({ preventScroll: true });
    };
  }, [active]);

  return containerRef;
}
