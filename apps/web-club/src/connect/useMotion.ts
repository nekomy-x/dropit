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

/**
 * Scroll-linked unfold for the landing's panel preview.
 *
 * The ref goes on the *untransformed* stage; the hook writes `--u` (0 → 1) there
 * and the inner window reads it. Measuring the stage rather than the rotating
 * window matters: `getBoundingClientRect()` includes the transform, so reading
 * the window itself would feed its own tilt back into the progress.
 *
 * `from` and `to` are viewport fractions for the stage's top edge, which is
 * also the window's hinge. The window hangs from that edge, so it covers
 * `top … top + height` no matter what the tilt does; the unfold starts as soon
 * as the shrunken panel fits in the frame (`from`, 0.52 vh — its whole tipped
 * body is on screen) and is done when the stage top has climbed to `to`
 * (0.18 vh), where the board is read flat. Measuring the stage rather than the
 * window matters: `getBoundingClientRect()` on the transformed window includes
 * the projection, so reading it would feed its own tilt back into the progress.
 *
 * That is the whole entrance: the panel is tipped while it is still entering,
 * opens as it is read, and is flat before its content needs to be understood.
 *
 * The curve is a smoothstep rather than an ease-out: an ease-out finishes the
 * motion in the first half of the scroll and then nothing happens, which reads
 * as the animation being cut short. Smoothstep spends the motion evenly and
 * still lands softly at both ends.
 *
 * The value only ever increases, so the panel opens once as it scrolls in and
 * then stays flat while the page keeps moving; it never tips away again on the
 * way back up. With reduced motion, or with no JavaScript at all (the CSS
 * default is `--u: 1`), the preview is simply flat.
 */
export function useUnfold<T extends HTMLElement>(from = 0.52, to = 0.18) {
  const ref = useRef<T | null>(null);

  useEffect(() => {
    const node = ref.current;
    if (!node) return undefined;
    if (prefersReducedMotion()) {
      node.style.setProperty('--u', '1');
      node.dataset.unfold = 'flat';
      return undefined;
    }

    let frame = 0;
    // -1 so the first measurement always writes: the CSS default is `--u: 1`
    // (flat, for the no-JavaScript case), and a panel that is still below the
    // fold has to start tipped, not flat.
    let progress = -1;

    const measure = () => {
      frame = 0;
      const top = node.getBoundingClientRect().top;
      const vh = window.innerHeight || 800;
      const start = vh * from;
      const end = vh * to;
      const raw = Math.min(1, Math.max(0, (start - top) / Math.max(1, start - end)));
      // smoothstep(3t² - 2t³): soft at both ends, so the panel is still visibly
      // moving while it is being read and settles exactly at the end of travel.
      const eased = raw * raw * (3 - 2 * raw);
      if (eased <= progress) return;
      progress = eased;
      node.style.setProperty('--u', progress.toFixed(4));
      // `data-unfold="flat"` lets the stylesheet drop `will-change`: once the
      // panel is flat there is nothing left to animate, and a 1300×850 layer
      // should not stay alive for the rest of the page.
      if (progress > 0.999) node.dataset.unfold = 'flat';
    };

    const schedule = () => {
      if (frame) return;
      frame = requestAnimationFrame(measure);
    };

    measure();
    window.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', schedule);
    return () => {
      window.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', schedule);
      if (frame) cancelAnimationFrame(frame);
    };
  }, [from, to]);

  return ref;
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
