import { useEffect, useRef, useState } from 'react';

/** True when the user asked the OS to reduce motion. */
export function prefersReducedMotion(): boolean {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/**
 * Reveals an element the first time it scrolls into view.
 *
 * Returns a ref to attach and a boolean to drive `opacity`/`translate`
 * classes. Elements above the fold are revealed immediately so the page
 * never looks empty on load.
 */
export function useReveal<T extends HTMLElement>(rootMargin = '-12% 0px -8% 0px') {
  const ref = useRef<T | null>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    if (prefersReducedMotion() || typeof IntersectionObserver === 'undefined') {
      setVisible(true);
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setVisible(true);
            observer.disconnect();
          }
        }
      },
      { rootMargin, threshold: 0.12 },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [rootMargin]);

  return { ref, visible };
}

/**
 * Animated number for the revenue calculator / KPI counters.
 * Eases towards `target` with requestAnimationFrame; snaps instantly when
 * motion is reduced.
 */
export function useCountUp(target: number, duration = 520): number {
  const [value, setValue] = useState(target);
  const fromRef = useRef(target);
  const frameRef = useRef<number | null>(null);

  useEffect(() => {
    if (prefersReducedMotion()) {
      fromRef.current = target;
      setValue(target);
      return;
    }

    const from = fromRef.current;
    if (from === target) return;
    const start = performance.now();

    const tick = (now: number) => {
      const t = Math.min(1, (now - start) / duration);
      // easeOutCubic — fast start, soft landing.
      const eased = 1 - Math.pow(1 - t, 3);
      setValue(from + (target - from) * eased);
      if (t < 1) {
        frameRef.current = requestAnimationFrame(tick);
      } else {
        fromRef.current = target;
        setValue(target);
      }
    };

    frameRef.current = requestAnimationFrame(tick);
    return () => {
      if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
      fromRef.current = target;
    };
  }, [target, duration]);

  return value;
}

/** Window scroll offset — used for the sticky nav backdrop and hero parallax. */
export function useScrollY(): number {
  const [y, setY] = useState(0);

  useEffect(() => {
    let frame = 0;
    const onScroll = () => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        setY(window.scrollY);
        frame = 0;
      });
    };
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      window.removeEventListener('scroll', onScroll);
      if (frame) cancelAnimationFrame(frame);
    };
  }, []);

  return y;
}
