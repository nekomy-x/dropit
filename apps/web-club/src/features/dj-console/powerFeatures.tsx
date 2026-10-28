import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type DragEvent, type KeyboardEvent } from 'react';

/** Minimal queue shape accepted by the power-feature hooks. */
export interface QueueRecord {
  id: string;
  status: string;
  title?: string;
  artist?: string;
  style?: string | null;
}

export type QueueAction = 'accept' | 'reject' | 'played';

export interface QueueFeedback {
  kind: 'success' | 'error';
  message: string;
}

export interface QueueDragDropOptions<T extends QueueRecord> {
  items: T[];
  onReorder?: (next: T[], previous: T[]) => void | Promise<void>;
  onReorderError?: (error: unknown) => void;
}

export interface QueueItemDragProps {
  draggable: true;
  role: 'listitem';
  tabIndex: 0;
  'aria-grabbed': boolean;
  'data-queue-id': string;
  onDragStart: (event: DragEvent<HTMLElement>) => void;
  onDragOver: (event: DragEvent<HTMLElement>) => void;
  onDrop: (event: DragEvent<HTMLElement>) => void;
  onDragEnd: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLElement>) => void;
}

export interface QueueDragDropResult<T extends QueueRecord> {
  items: T[];
  draggingId: string | null;
  dropTargetId: string | null;
  isReordering: boolean;
  getItemProps: (item: T, index: number) => QueueItemDragProps;
}

/**
 * Native drag/drop queue behavior with an optimistic reorder and rollback.
 * The hook deliberately does not assume an ordering endpoint; provide one via
 * onReorder when the backend supports persistence.
 */
export function useQueueDragDrop<T extends QueueRecord>({
  items: sourceItems,
  onReorder,
  onReorderError,
}: QueueDragDropOptions<T>): QueueDragDropResult<T> {
  const [items, setItems] = useState(sourceItems);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dropTargetId, setDropTargetId] = useState<string | null>(null);
  const [isReordering, setIsReordering] = useState(false);
  const itemsRef = useRef(items);
  const sourceRef = useRef(sourceItems);

  useEffect(() => {
    itemsRef.current = items;
  }, [items]);

  useEffect(() => {
    // Polling can replace the source list. Keep a local optimistic order while
    // a drag or persistence request is in flight, then accept the fresh data.
    if (sourceRef.current !== sourceItems && draggingId === null && !isReordering) {
      setItems(sourceItems);
      sourceRef.current = sourceItems;
    }
  }, [sourceItems, draggingId, isReordering]);

  const reorder = useCallback((sourceId: string, targetId: string) => {
    if (sourceId === targetId || isReordering) return;
    const previous = itemsRef.current;
    const fromIndex = previous.findIndex((item) => item.id === sourceId);
    const toIndex = previous.findIndex((item) => item.id === targetId);
    if (fromIndex < 0 || toIndex < 0) return;

    const next = [...previous];
    const [moved] = next.splice(fromIndex, 1);
    if (!moved) return;
    next.splice(toIndex, 0, moved);
    itemsRef.current = next;
    setItems(next);
    setIsReordering(true);

    void Promise.resolve(onReorder?.(next, previous))
      .catch((error: unknown) => {
        itemsRef.current = previous;
        setItems(previous);
        onReorderError?.(error);
      })
      .finally(() => setIsReordering(false));
  }, [isReordering, onReorder, onReorderError]);

  const getItemProps = useCallback((item: T, index: number): QueueItemDragProps => ({
    draggable: true,
    role: 'listitem',
    tabIndex: 0,
    'aria-grabbed': draggingId === item.id,
    'data-queue-id': item.id,
    onDragStart: (event) => {
      if (isReordering) {
        event.preventDefault();
        return;
      }
      event.dataTransfer.effectAllowed = 'move';
      event.dataTransfer.setData('text/plain', item.id);
      setDraggingId(item.id);
    },
    onDragOver: (event) => {
      event.preventDefault();
      if (draggingId !== item.id) {
        event.dataTransfer.dropEffect = 'move';
        setDropTargetId(item.id);
      }
    },
    onDrop: (event) => {
      event.preventDefault();
      const sourceId = event.dataTransfer.getData('text/plain') || draggingId;
      if (sourceId) reorder(sourceId, item.id);
      setDraggingId(null);
      setDropTargetId(null);
    },
    onDragEnd: () => {
      setDraggingId(null);
      setDropTargetId(null);
    },
    onKeyDown: (event) => {
      if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
      event.preventDefault();
      const destination = event.key === 'ArrowUp' ? items[index - 1] : items[index + 1];
      if (destination) reorder(item.id, destination.id);
    },
  }), [draggingId, isReordering, items, reorder]);

  return { items, draggingId, dropTargetId, isReordering, getItemProps };
}

export interface QueueFilters<T extends QueueRecord> {
  query: string;
  status: string;
  style: string;
  setQuery: (value: string) => void;
  setStatus: (value: string) => void;
  setStyle: (value: string) => void;
  toggleStatus: (value: string) => void;
  toggleStyle: (value: string) => void;
  clearFilters: () => void;
  hasFilters: boolean;
  filteredItems: T[];
}

/** Search and status/style filters shared by queue views and command actions. */
export function useQueueFilters<T extends QueueRecord>(items: T[]): QueueFilters<T> {
  const [query, setQuery] = useState('');
  const [status, setStatus] = useState('all');
  const [style, setStyle] = useState('all');

  const filteredItems = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return items.filter((item) => {
      const matchesQuery = needle.length === 0
        || `${item.title ?? ''} ${item.artist ?? ''} ${item.style ?? ''}`.toLocaleLowerCase().includes(needle);
      const matchesStatus = status === 'all' || item.status === status;
      const matchesStyle = style === 'all' || item.style === style;
      return matchesQuery && matchesStatus && matchesStyle;
    });
  }, [items, query, status, style]);

  const clearFilters = useCallback(() => {
    setQuery('');
    setStatus('all');
    setStyle('all');
  }, []);
  const toggleStatus = useCallback((value: string) => setStatus((current) => current === value ? 'all' : value), []);
  const toggleStyle = useCallback((value: string) => setStyle((current) => current === value ? 'all' : value), []);

  return {
    query,
    status,
    style,
    setQuery,
    setStatus,
    setStyle,
    toggleStatus,
    toggleStyle,
    clearFilters,
    hasFilters: query.trim().length > 0 || status !== 'all' || style !== 'all',
    filteredItems,
  };
}

export interface PaletteCommand {
  id: string;
  label: string;
  keywords?: string[];
  shortcut?: string;
  onSelect: () => void | Promise<void>;
}

export interface CommandPaletteResult {
  isOpen: boolean;
  query: string;
  setQuery: (value: string) => void;
  open: () => void;
  close: () => void;
  toggle: () => void;
  run: (command: PaletteCommand) => void;
  filteredCommands: PaletteCommand[];
}

/** Global Cmd/Ctrl+K command palette state. Attach the returned handlers to a view. */
export function useCommandPalette(commands: PaletteCommand[]): CommandPaletteResult {
  const [isOpen, setIsOpen] = useState(false);
  const [query, setQuery] = useState('');

  const open = useCallback(() => {
    setQuery('');
    setIsOpen(true);
  }, []);
  const close = useCallback(() => {
    setIsOpen(false);
    setQuery('');
  }, []);
  const toggle = useCallback(() => {
    setIsOpen((value) => !value);
    setQuery('');
  }, []);

  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setIsOpen(true);
        setQuery('');
      } else if (event.key === 'Escape') {
        setIsOpen(false);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const filteredCommands = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!needle) return commands;
    return commands.filter((command) => [command.label, ...(command.keywords ?? [])]
      .join(' ').toLocaleLowerCase().includes(needle));
  }, [commands, query]);

  const run = useCallback((command: PaletteCommand) => {
    close();
    void command.onSelect();
  }, [close]);

  return { isOpen, query, setQuery, open, close, toggle, run, filteredCommands };
}

export interface OptimisticQueueActions {
  runAction: (itemId: string, action: QueueAction, reason?: string) => Promise<void>;
  pendingAction: { id: string; type: QueueAction } | null;
  feedback: QueueFeedback | null;
  clearFeedback: () => void;
}

const optimisticStatus: Record<QueueAction, string> = {
  accept: 'accepted',
  reject: 'rejected',
  played: 'played',
};

/** Immediately updates a request, restores it on failure, and exposes feedback text. */
export function useOptimisticQueueActions<T extends QueueRecord>(
  items: T[],
  setItems: (next: T[] | ((current: T[]) => T[])) => void,
  mutate: (itemId: string, action: QueueAction, reason?: string) => Promise<unknown>,
): OptimisticQueueActions {
  const [pendingAction, setPendingAction] = useState<{ id: string; type: QueueAction } | null>(null);
  const [feedback, setFeedback] = useState<QueueFeedback | null>(null);
  const itemsRef = useRef(items);
  useEffect(() => { itemsRef.current = items; }, [items]);

  const runAction = useCallback(async (itemId: string, action: QueueAction, reason?: string) => {
    if (pendingAction) return;
    const previous = itemsRef.current;
    const item = previous.find((candidate) => candidate.id === itemId);
    if (!item) return;
    setFeedback(null);
    setPendingAction({ id: itemId, type: action });
    const optimistic = previous.map((candidate) => candidate.id === itemId
      ? { ...candidate, status: optimisticStatus[action] }
      : candidate);
    itemsRef.current = optimistic;
    setItems(optimistic);
    try {
      await mutate(itemId, action, reason);
      setFeedback({ kind: 'success', message: `${item.title ?? 'Request'} ${action === 'played' ? 'marked played' : `${action}ed`}.` });
    } catch (error) {
      itemsRef.current = previous;
      setItems(previous);
      setFeedback({ kind: 'error', message: error instanceof Error ? error.message : 'Could not update request.' });
    } finally {
      setPendingAction(null);
    }
  }, [mutate, pendingAction, setItems]);

  const clearFeedback = useCallback(() => setFeedback(null), []);
  return { runAction, pendingAction, feedback, clearFeedback };
}

export interface TelemetryEvent {
  name: string;
  value?: number;
  metadata?: Record<string, string | number | boolean | null>;
  timestamp: number;
}

export interface DjTelemetry {
  events: TelemetryEvent[];
  latest: TelemetryEvent | null;
  track: (name: string, metadata?: TelemetryEvent['metadata'], value?: number) => void;
}

/** Local telemetry buffer; plug in a sink at the call site without coupling UI to analytics. */
export function useDjTelemetry(sink?: (event: TelemetryEvent) => void, maxEvents = 100): DjTelemetry {
  const [events, setEvents] = useState<TelemetryEvent[]>([]);
  const sinkRef = useRef(sink);
  useEffect(() => { sinkRef.current = sink; }, [sink]);

  const track = useCallback((name: string, metadata?: TelemetryEvent['metadata'], value?: number) => {
    const event: TelemetryEvent = { name, metadata, value, timestamp: Date.now() };
    setEvents((current) => [...current, event].slice(-Math.max(1, maxEvents)));
    sinkRef.current?.(event);
  }, [maxEvents]);

  return { events, latest: events.at(-1) ?? null, track };
}

export function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    const media = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setReduced(media.matches);
    update();
    media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  }, []);
  return reduced;
}

export interface WaveformOptions {
  barCount?: number;
  active?: boolean;
  amplitude?: number;
}

/** Lightweight visualizer state. Bars freeze to a calm static shape for reduced motion. */
export function useWaveformAnimation({ barCount = 24, active = true, amplitude = 1 }: WaveformOptions = {}) {
  const reducedMotion = usePrefersReducedMotion();
  const safeCount = Math.max(4, Math.floor(barCount));
  const staticBars = useMemo(() => Array.from({ length: safeCount }, (_, index) => {
    const center = (safeCount - 1) / 2;
    return Math.max(12, Math.round(34 + (1 - Math.abs(index - center) / center) * 40));
  }), [safeCount]);
  const [bars, setBars] = useState(staticBars);

  useEffect(() => {
    if (!active || reducedMotion) {
      setBars(staticBars);
      return undefined;
    }
    const update = () => setBars(staticBars.map((base) => Math.max(8, Math.min(100, Math.round(base * (1 - amplitude * 0.28 + Math.random() * amplitude * 0.56))))));
    update();
    const timer = window.setInterval(update, 180);
    return () => window.clearInterval(timer);
  }, [active, amplitude, reducedMotion, staticBars]);

  return { bars, reducedMotion, isAnimating: active && !reducedMotion };
}

export interface WaveformBarsProps extends WaveformOptions {
  label?: string;
  className?: string;
}

/** Presentational waveform that keeps useful state available without animation. */
export function WaveformBars({ label = 'Audio waveform', className = '', ...options }: WaveformBarsProps) {
  const { bars, isAnimating } = useWaveformAnimation(options);
  return (
    <span className={`dj-power-waveform ${isAnimating ? 'dj-power-waveform-animated' : ''} ${className}`.trim()} role="img" aria-label={label}>
      {bars.map((height, index) => <i key={`${index}-${height}`} style={{ '--wave-height': `${height}%`, '--wave-index': index } as CSSProperties} aria-hidden="true" />)}
    </span>
  );
}

export function isInteractiveTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && ['INPUT', 'TEXTAREA', 'SELECT', 'BUTTON', 'A'].includes(target.tagName);
}
