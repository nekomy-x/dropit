/**
 * A 5-second clock for the board's freshness line.
 *
 * 01-contract.md §7 forbids a self-ticking "snapshot" counter: the label is
 * derived from `lastFetchedAt` (set only after a successful request) and this
 * timer only re-renders that derivation, at most every 5 s. When nothing has
 * ever been fetched the board says so in words instead of counting.
 */
import { useEffect, useState } from 'react';

export function useBoardClock(intervalMs = 5000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}
