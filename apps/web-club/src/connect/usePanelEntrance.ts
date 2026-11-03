import { useLayoutEffect, useRef } from 'react';

/** One entrance when enough of the preview is visible, independent of scroll speed. */
export function usePanelEntrance<T extends HTMLElement>() {
  const ref = useRef<T | null>(null);

  useLayoutEffect(() => {
    const stage = ref.current;
    const screen = stage?.querySelector<HTMLElement>('.connect-panel-window');
    if (!stage || !screen) return;

    const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
    if (motion.matches || !window.IntersectionObserver || !screen.animate) return;

    const mobile = window.matchMedia('(max-width: 767px)').matches;
    const animations: Animation[] = [];
    let observer: IntersectionObserver | undefined;
    let started = false;
    let disposed = false;

    const settle = () => {
      observer?.disconnect();
      animations.forEach((animation) => animation.cancel());
      stage.dataset.panelMotion = 'settled';
    };

    const prepare = (element: HTMLElement, from: Keyframe, duration: number, delay = 0) => {
      const animation = element.animate(
        [from, { transform: 'none', opacity: 1 }],
        { duration, delay, easing: 'cubic-bezier(0.16, 1, 0.3, 1)', fill: 'both' },
      );
      // Prepare before paint, keeping all copy visible while awaiting the viewport.
      animation.pause();
      animation.currentTime = 0;
      animations.push(animation);
    };

    prepare(screen, {
      transform: mobile
        ? 'translate3d(0, 20px, 0) scale(0.985)'
        : 'perspective(1800px) translate3d(0, 40px, 0) rotateX(7deg) scale(0.965)',
      opacity: 0.8,
    }, mobile ? 460 : 640);

    const reveal = (selector: string, delay: number, stagger = 0) => {
      stage.querySelectorAll<HTMLElement>(selector).forEach((element, index) => {
        prepare(element, { transform: 'translate3d(0, 12px, 0)', opacity: 0.72 },
          mobile ? 300 : 360, (mobile ? delay * 0.6 : delay) + index * stagger);
      });
    };
    reveal('.panel-rail', 60);
    reveal('.panel-topbar, .panel-page-head', 90, 30);
    reveal('.stat-card', 130, mobile ? 20 : 40);
    reveal('.panel-grid-hero > .panel-card', 210, mobile ? 20 : 40);

    // Subscribe before a possible cancellation so finished promises never reject unhandled.
    void Promise.allSettled(animations.map((animation) => animation.finished)).then(() => {
      if (!disposed) settle();
    });
    stage.dataset.panelMotion = 'ready';

    const visibleHeight = Math.min(144, window.innerHeight * 0.18);
    const threshold = Math.min(0.5, visibleHeight / Math.max(1, stage.offsetHeight));
    observer = new IntersectionObserver((entries) => {
      if (started) return;
      const entry = entries[entries.length - 1];
      if (!entry?.isIntersecting || entry.intersectionRatio < threshold) return;
      started = true;
      observer?.disconnect();
      stage.dataset.panelMotion = 'running';
      animations.forEach((animation) => animation.play());
    }, { threshold, rootMargin: '-72px 0px -24px 0px' });
    observer.observe(stage);

    const onMotionChange = () => {
      if (motion.matches) settle();
    };
    motion.addEventListener('change', onMotionChange);
    return () => {
      disposed = true;
      observer?.disconnect();
      motion.removeEventListener('change', onMotionChange);
      animations.forEach((animation) => animation.cancel());
      delete stage.dataset.panelMotion;
    };
  }, []);

  return ref;
}
