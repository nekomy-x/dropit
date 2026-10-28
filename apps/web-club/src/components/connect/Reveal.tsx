import type { ReactNode } from 'react';
import { useReveal } from '../../connect/useMotion';

interface RevealProps {
  children: ReactNode;
  /** Stagger, ms — keep the sequence short (≤ 240 ms) so it never feels slow. */
  delay?: number;
  className?: string;
}

/**
 * Fades + lifts its children the first time they enter the viewport.
 * Uses transform/opacity only, so it stays on the compositor.
 */
export default function Reveal({ children, delay = 0, className = '' }: RevealProps) {
  const { ref, visible } = useReveal<HTMLDivElement>();

  return (
    <div
      ref={ref}
      data-reveal
      className={`transition-[opacity,transform] duration-[620ms] ease-[cubic-bezier(0.22,1,0.36,1)] ${
        visible ? 'translate-y-0 opacity-100' : 'translate-y-[18px] opacity-0'
      } ${className}`}
      style={{ transitionDelay: visible ? `${delay}ms` : '0ms' }}
    >
      {children}
    </div>
  );
}
