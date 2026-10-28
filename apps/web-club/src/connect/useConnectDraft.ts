import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  DEFAULT_DRAFT,
  type ClubStepData,
  type ConnectDraft,
  type QrStepData,
  type VenueStepData,
} from './types';

const STORAGE_KEY = 'dropit.connect.draft.v1';

function readStoredDraft(): ConnectDraft {
  if (typeof window === 'undefined') return DEFAULT_DRAFT;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_DRAFT;
    const parsed = JSON.parse(raw) as Partial<ConnectDraft>;
    // Merge defensively: a draft written by an older build must never crash
    // the wizard, so every nested object falls back to the defaults.
    return {
      club: { ...DEFAULT_DRAFT.club, ...(parsed.club ?? {}) },
      venue: { ...DEFAULT_DRAFT.venue, ...(parsed.venue ?? {}) },
      qr: { ...DEFAULT_DRAFT.qr, ...(parsed.qr ?? {}) },
      savedAt: parsed.savedAt ?? null,
    };
  } catch {
    return DEFAULT_DRAFT;
  }
}

/**
 * Draft state for the Connect wizard.
 *
 * - loads the saved draft once (localStorage)
 * - exposes per-step patchers that keep the rest of the draft intact
 * - auto-saves 600 ms after the last change ("Черновик сохраняется
 *   автоматически" in the design footer) and reports the save time
 */
export function useConnectDraft() {
  const [draft, setDraft] = useState<ConnectDraft>(readStoredDraft);
  const [savedAt, setSavedAt] = useState<string | null>(draft.savedAt);
  const [saving, setSaving] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    setSaving(true);
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      const stamp = new Date().toISOString();
      const next: ConnectDraft = { ...draft, savedAt: stamp };
      try {
        window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      } catch {
        // Storage can be full or disabled (private mode) — the wizard still
        // works, it just loses the draft on reload.
      }
      setSavedAt(stamp);
      setSaving(false);
    }, 600);

    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [draft]);

  const patchClub = useCallback((patch: Partial<ClubStepData>) => {
    setDraft((prev) => ({ ...prev, club: { ...prev.club, ...patch } }));
  }, []);

  const patchVenue = useCallback((patch: Partial<VenueStepData>) => {
    setDraft((prev) => ({ ...prev, venue: { ...prev.venue, ...patch } }));
  }, []);

  const patchQr = useCallback((patch: Partial<QrStepData>) => {
    setDraft((prev) => ({ ...prev, qr: { ...prev.qr, ...patch } }));
  }, []);

  const reset = useCallback(() => {
    try {
      window.localStorage.removeItem(STORAGE_KEY);
    } catch {
      // ignore — see above
    }
    setDraft(DEFAULT_DRAFT);
    setSavedAt(null);
  }, []);

  /** "сохранено 14:32" — short local time for the footer hint. */
  const savedLabel = useMemo(() => {
    if (!savedAt) return null;
    return new Date(savedAt).toLocaleTimeString('ru-RU', {
      hour: '2-digit',
      minute: '2-digit',
    });
  }, [savedAt]);

  return { draft, patchClub, patchVenue, patchQr, reset, saving, savedLabel };
}
