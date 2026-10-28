/**
 * Toast — transient confirmation, feedback or a real undo.
 *
 * Contract (01-contract.md §3, §5.3):
 *  - lives at least 8000ms (`duration` cannot drop below that);
 *  - the timer pauses on hover AND on focus anywhere inside the toast;
 *  - exactly ONE `role="status"` in the whole toast system, and NO `aria-live`
 *    on the container above it, so a screen reader gets one announcement per
 *    toast instead of a nested double announcement;
 *  - the close button is a real 36x36 hit target (44x44 on touch widths);
 *  - "Undo" is only rendered when the caller passes a rollback handler —
 *    a toast must never promise a rollback that does not exist.
 *
 * Compose with the exported `useToasts()` hook; the region renders nothing
 * when the queue is empty, so it costs no layout.
 */
import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { CircleAlert, CircleCheck, Info, TriangleAlert, X } from 'lucide-react';
import { useLocale } from '../../locale';

export type ToastTone = 'live' | 'warn' | 'error' | 'neutral';

export interface ToastInput {
  title: string;
  message?: string;
  tone?: ToastTone;
  /** ms; values under 8000 are raised to 8000. */
  duration?: number;
  /** Rendered as an explicit rollback — only pass when the action is reversible. */
  onUndo?: () => void;
  undoLabel?: string;
}

interface ToastRecord extends ToastInput {
  id: string;
  tone: ToastTone;
  duration: number;
}

const MIN_DURATION = 8000;

// Monotonic id source. Deliberately not Math.random(): the panel must not carry
// the pattern anywhere near data that the board renders as telemetry.
let toastSeq = 0;
const nextToastId = () => `toast-${++toastSeq}`;

const TONE_ICON = {
  live: CircleCheck,
  warn: TriangleAlert,
  error: CircleAlert,
  neutral: Info,
} as const;

export function useToasts() {
  const [toasts, setToasts] = useState<ToastRecord[]>([]);

  const dismiss = useCallback((id: string) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const push = useCallback((input: ToastInput) => {
    const id = nextToastId();
    const record: ToastRecord = {
      ...input,
      id,
      tone: input.tone ?? 'neutral',
      duration: Math.max(MIN_DURATION, input.duration ?? MIN_DURATION),
    };
    setToasts((current) => [...current, record]);
    return id;
  }, []);

  const clear = useCallback(() => setToasts([]), []);

  return useMemo(() => ({ toasts, push, dismiss, clear }), [toasts, push, dismiss, clear]);
}

export interface ToastRegionProps {
  toasts: ToastRecord[];
  onDismiss: (id: string) => void;
}

function ToastItem({ toast, onDismiss, announce }: { toast: ToastRecord; onDismiss: (id: string) => void; announce: boolean }) {
  const { t } = useLocale();
  const [paused, setPaused] = useState(false);
  const remaining = useRef(toast.duration);
  const startedAt = useRef<number>(0);
  // Where focus came from, so closing a focused toast does not drop the user on
  // <body> (audited defect: "focus falls to <body> after the toast closes").
  const returnFocusTo = useRef<HTMLElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const titleId = useId();
  const Icon = TONE_ICON[toast.tone];

  // Move focus out BEFORE the node unmounts. Doing it in the unmount cleanup is
  // too late: React has already removed the button, so focus has fallen to
  // <body> (this was the audited defect).
  const dismissNow = useCallback(() => {
    const root = rootRef.current;
    const active = document.activeElement;
    if (root && active instanceof HTMLElement && root.contains(active)) {
      const target = returnFocusTo.current;
      if (target && document.contains(target)) {
        target.focus({ preventScroll: true });
      } else {
        // Nothing remembered: hand focus to the control that opened the flow,
        // falling back to the page's main landmark rather than <body>.
        const fallback = document.querySelector<HTMLElement>('.command-trigger, [data-control="button"]');
        fallback?.focus({ preventScroll: true });
      }
    }
    onDismiss(toast.id);
  }, [onDismiss, toast.id]);

  // If the toast is removed while focus is inside it (auto-dismiss), the browser
  // falls back to <body>; blur first so the page keeps a sane active element.
  useEffect(() => () => {
    const root = rootRef.current;
    const active = document.activeElement;
    if (root && active instanceof HTMLElement && root.contains(active)) {
      const target = returnFocusTo.current;
      if (target && document.contains(target)) target.focus({ preventScroll: true });
    }
  }, []);

  useEffect(() => {
    if (paused) {
      remaining.current -= Date.now() - startedAt.current;
      return undefined;
    }
    startedAt.current = Date.now();
    const timer = window.setTimeout(dismissNow, Math.max(0, remaining.current));
    return () => window.clearTimeout(timer);
  }, [paused, dismissNow]);

  return (
    <div
      ref={rootRef}
      className={`club-toast club-toast-${toast.tone}`}
      // Exactly one live-region in the region: the newest toast announces.
      role={announce ? 'status' : 'group'}
      aria-labelledby={titleId}
      data-toast=""
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocusCapture={(event) => {
        setPaused(true);
        const from = event.relatedTarget;
        if (from instanceof HTMLElement && !event.currentTarget.contains(from)) returnFocusTo.current = from;
      }}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setPaused(false);
      }}
    >
      <Icon className="club-toast-icon" size={16} aria-hidden="true" />
      <div className="club-toast-copy">
        <p className="club-toast-title" id={titleId}>{toast.title}</p>
        {toast.message ? <p className="club-toast-message">{toast.message}</p> : null}
      </div>
      <div className="club-toast-actions">
        {toast.onUndo ? (
          <button
            type="button"
            className="club-toast-action"
            data-control="button"
            onClick={() => { toast.onUndo?.(); dismissNow(); }}
          >
            {toast.undoLabel ?? t('toast.undo')}
          </button>
        ) : null}
        <button
          type="button"
          className="club-toast-close"
          data-control="icon"
          aria-label={t('toast.dismiss', { title: toast.title })}
          onClick={dismissNow}
        >
          <X size={15} aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}

export default function ToastRegion({ toasts, onDismiss }: ToastRegionProps) {
  if (toasts.length === 0) return null;
  const newestId = toasts[toasts.length - 1]?.id;
  return (
    <div className="club-toast-region" data-toast-region="">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} onDismiss={onDismiss} announce={toast.id === newestId} />
      ))}
    </div>
  );
}
