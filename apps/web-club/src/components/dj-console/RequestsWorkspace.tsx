/**
 * Requests workspace — the DJ's processing surface.
 *
 * One tab, one job: decide what happens to every incoming request. A row must
 * answer four questions without opening anything else — who asked, how many
 * guests asked, how long it has been waiting, and what it sounds like — and
 * then offer exactly the actions that settle it.
 *
 * The music block is part of that: the catalog's 30-second preview plays in
 * place (one shared <audio> element, so starting a second preview stops the
 * first), and the four service links open that service's **search** results.
 * The backend exposes no streaming ids, so nothing here may claim a catalog
 * match; the labels say "find", and the group says "open in".
 *
 * Copy lives in `messages/dj-requests.ts` (this surface) and the shared `dj.*`
 * vocabulary in `messages/dj-console.ts` / the shell catalog. Live facts keep
 * their meaning: the strip says `live`, `timer` or `offline` for the channel
 * that is actually keeping the list current, and demo data is labelled demo.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import Icon from '../Icon';
import { useLocale, type Translate } from '../../locale';
import type {
  DjConsoleMockApi,
  DjConsoleRequest,
  DjLiveStatus,
  DjRequestStatus,
} from '../../dj-console/types';
import { musicLinksFor, trackClipboardText } from '../../dj-console/musicLinks';
import '../../pages/dj-requests.css';

export interface RequestsWorkspaceProps {
  dj: DjConsoleMockApi;
  live: boolean;
  loading: boolean;
  sort: 'new' | 'hot';
  onSortChange: (sort: 'new' | 'hot') => void;
  status: DjLiveStatus;
  onNotice: (message: string) => void;
}

type StatusFilter = 'all' | 'pending' | 'accepted';
type CopyKind = 'track' | 'isrc';

const FILTERS: readonly StatusFilter[] = ['all', 'pending', 'accepted'];

const FILTER_KEY: Record<StatusFilter, string> = {
  all: 'common.all',
  pending: 'dj.filterAwaiting',
  accepted: 'dj.filterQueued',
};

const STATUS_KEY: Record<DjRequestStatus, string> = {
  pending: 'dj.statusPending',
  accepted: 'dj.statusAccepted',
  played: 'dj.statusPlayed',
  rejected: 'dj.statusRejected',
};

const STATUS_TONE: Record<DjRequestStatus, string> = {
  pending: 'chip chip-warn',
  accepted: 'chip chip-accent',
  played: 'chip chip-neutral',
  rejected: 'chip chip-error',
};

/** Russian needs one/few/many, so the count goes through Intl.PluralRules and
 *  the same four keys the rest of the console uses. */
const PLURAL_KEY: Record<string, string> = {
  one: 'dj.requestsForTrackOne',
  few: 'dj.requestsForTrackFew',
  many: 'dj.requestsForTrackMany',
  other: 'dj.requestsForTrackOther',
};

const REJECT_REASONS = [
  'dj.rejectReasonStyle',
  'dj.rejectReasonAlreadyPlayed',
  'dj.rejectReasonLater',
] as const;

/** How long a copy control keeps its "copied" label before it returns. */
const COPIED_MS = 1200;
/** A reading younger than this is "just now" rather than "3 s ago". */
const JUST_NOW_MS = 5000;

function requestCountLabel(count: number, locale: string, t: Translate): string {
  const category = new Intl.PluralRules(locale).select(count);
  return t(PLURAL_KEY[category] ?? 'dj.requestsForTrackOther', { count });
}

function arrivalClock(iso: string, locale: string, unavailable: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return unavailable;
  return new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' }).format(date);
}

function money(minor: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    style: 'currency',
    currency: 'EUR',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(minor / 100);
}

export default function RequestsWorkspace({
  dj,
  live,
  loading,
  sort,
  onSortChange,
  status,
  onNotice,
}: RequestsWorkspaceProps) {
  const { t, locale } = useLocale();
  const unavailable = t('common.unavailable');

  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<StatusFilter>('all');
  const [playingId, setPlayingId] = useState<string | null>(null);
  const [copied, setCopied] = useState<{ id: string; kind: CopyKind } | null>(null);
  const [reasonFor, setReasonFor] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const rowRefs = useRef(new Map<string, HTMLLIElement>());
  const copyTimer = useRef<number | undefined>(undefined);

  const requests = dj.queue;
  const session = dj.state.session;
  const paused = session?.status === 'paused_acceptance';
  const lastSyncAt = status.lastSyncAt;

  /* Freshness ticks once a second while the tab is visible. A hidden tab is not
     "watching" the queue, so the clock stops with it instead of aging in the
     background and jumping when the DJ comes back. */
  useEffect(() => {
    if (lastSyncAt === null) return undefined;
    let timer: number | undefined;
    const tick = () => setNow(Date.now());
    const stop = () => {
      if (timer !== undefined) {
        window.clearInterval(timer);
        timer = undefined;
      }
    };
    const start = () => {
      stop();
      tick();
      timer = window.setInterval(tick, 1000);
    };
    if (document.visibilityState === 'visible') start();
    const onVisibility = () => {
      if (document.visibilityState === 'visible') start();
      else stop();
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      stop();
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [lastSyncAt]);

  /* One audio element for the whole surface: leaving the tab must not leave a
     preview playing, and the copy label timer dies with the component. */
  useEffect(() => () => {
    const audio = audioRef.current;
    if (audio) {
      audio.pause();
      audio.removeAttribute('src');
    }
    if (copyTimer.current !== undefined) window.clearTimeout(copyTimer.current);
  }, []);

  /* A request that leaves the queue (accepted into another filter, declined,
     refreshed away) must not keep playing invisibly. */
  useEffect(() => {
    if (!playingId) return;
    if (requests.some((item) => item.id === playingId)) return;
    const audio = audioRef.current;
    if (audio) {
      audio.pause();
      audio.removeAttribute('src');
    }
    setPlayingId(null);
  }, [playingId, requests]);

  const normalizedQuery = query.trim().toLowerCase();

  const visible = useMemo(() => {
    const filtered = requests.filter((item) => {
      if (filter !== 'all' && item.status !== filter) return false;
      if (!normalizedQuery) return true;
      return `${item.title} ${item.artist}`.toLowerCase().includes(normalizedQuery);
    });
    /* "Newest" keeps the order the adapter returned (the API is queried with
       that sort); "most wanted" re-orders by the merge count the row shows. */
    if (sort === 'hot') {
      return filtered
        .slice()
        .sort((a, b) => b.sameTrackCount - a.sameTrackCount || Date.parse(b.createdAt) - Date.parse(a.createdAt));
    }
    return filtered;
  }, [requests, filter, normalizedQuery, sort]);

  const waiting = requests.reduce((total, item) => total + (item.status === 'pending' ? 1 : 0), 0);
  const queued = requests.reduce((total, item) => total + (item.status === 'accepted' ? 1 : 0), 0);

  const connectionChip = !live
    ? { tone: 'chip chip-neutral', label: t('dj.demoData') }
    : status.connection === 'live'
      ? { tone: 'chip chip-accent', label: t('common.live') }
      : status.connection === 'timer'
        ? { tone: 'chip chip-warn', label: t('dj.liveTimer') }
        : { tone: 'chip chip-error', label: t('dj.liveStale') };

  /* No reading yet means no sentence: the chip alone already says which channel
     is keeping the list current, and demo mode has no server to be fresh
     against. */
  const freshness = lastSyncAt === null
    ? null
    : now - lastSyncAt < JUST_NOW_MS
      ? t('dj.updatedJustNow')
      : t('dj.updatedSecondsAgo', { seconds: Math.floor((now - lastSyncAt) / 1000) });

  const accept = (id: string) => {
    dj.action(id, 'accept');
    setReasonFor(null);
    onNotice(t('dj.noticeAccepted'));
  };

  const reject = (id: string, reason?: string) => {
    dj.action(id, 'reject', reason);
    setReasonFor(null);
    onNotice(t('dj.noticeDeclined'));
  };

  const markPlayed = (id: string) => {
    dj.action(id, 'played');
    setReasonFor(null);
    onNotice(t('dj.noticePlayed'));
  };

  const toggleIntake = () => {
    if (!session) return;
    dj.toggleAcceptance();
    onNotice(t(paused ? 'dj.noticeIntakeResumed' : 'dj.noticeIntakePaused'));
  };

  const stopPreview = () => {
    const audio = audioRef.current;
    if (audio) {
      audio.pause();
      audio.removeAttribute('src');
      audio.load();
    }
    setPlayingId(null);
  };

  const togglePreview = (item: DjConsoleRequest) => {
    const audio = audioRef.current;
    if (!audio || !item.previewUrl) return;
    if (playingId === item.id) {
      stopPreview();
      return;
    }
    audio.pause();
    audio.src = item.previewUrl;
    setPlayingId(item.id);
    /* A preview that cannot load must say so: a play button that does nothing
       reads as a broken screen, and `common.error` is the honest word for it. */
    void audio.play().catch(() => {
      stopPreview();
      onNotice(t('common.error'));
    });
  };

  const copy = async (text: string, id: string, kind: CopyKind) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied({ id, kind });
      if (copyTimer.current !== undefined) window.clearTimeout(copyTimer.current);
      copyTimer.current = window.setTimeout(() => setCopied(null), COPIED_MS);
      onNotice(kind === 'track' ? t('dj.trackCopied') : t('dj.isrcCopied'));
    } catch {
      onNotice(t('dj.noticeClipboardDenied'));
    }
  };

  const moveActive = (delta: number) => {
    if (visible.length === 0) return;
    const index = visible.findIndex((item) => item.id === activeId);
    const nextIndex = index < 0 ? 0 : Math.min(visible.length - 1, Math.max(0, index + delta));
    const next = visible[nextIndex];
    if (!next) return;
    setActiveId(next.id);
    rowRefs.current.get(next.id)?.focus();
  };

  /* The list is the keyboard surface. ↑/↓ walk the rows, Enter settles the row
     the same way its primary button does, Backspace asks for a decline reason,
     P marks played, and `/` jumps to the search field from anywhere on the
     surface. Nothing fires while the DJ is typing. */
  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    const typing = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;

    if (event.key === '/' && !typing && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      searchRef.current?.focus();
      return;
    }
    if (typing) return;

    /* Escape closes the reason row wherever focus is inside it: a disclosure
       that opens inside a row must also be dismissible from inside that row. */
    if (event.key === 'Escape' && reasonFor) {
      setReasonFor(null);
      return;
    }

    const row = target.closest('[data-request-row]');
    if (!(row instanceof HTMLElement)) return;
    const item = visible.find((entry) => entry.id === row.dataset.requestId);
    if (!item) return;

    /* Settling keys only fire when the row itself holds focus: on a nested
       button, Enter and Space belong to that button. */
    const onRow = target === row || target.closest('button, a') === null;

    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        moveActive(1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        moveActive(-1);
        break;
      case 'Enter':
        if (!onRow) return;
        event.preventDefault();
        if (item.status === 'pending') accept(item.id);
        else markPlayed(item.id);
        break;
      case 'Backspace':
        if (!onRow || item.status !== 'pending') return;
        event.preventDefault();
        setReasonFor(item.id);
        break;
      case 'p':
      case 'P':
        if (!onRow || event.metaKey || event.ctrlKey || event.altKey) return;
        event.preventDefault();
        markPlayed(item.id);
        break;
      default:
        break;
    }
  };

  return (
    <div className="dj-req-workspace" onKeyDown={handleKeyDown}>
      {/* Live strip: what is keeping the list current, how much is waiting, and
          the one control that changes the intake. It sticks under the top bar
          so none of that scrolls away. */}
      <div className="dj-req-strip">
        <div className="dj-req-strip-main">
          <span className={connectionChip.tone}>{connectionChip.label}</span>
          {freshness ? <span className="dj-req-fresh">{freshness}</span> : null}
        </div>
        <div className="dj-req-strip-counts">
          <span className="dj-req-count">
            <span className="dj-req-count-label">{t('dj.waitingNow')}</span>
            <span className="dj-req-count-value">{waiting}</span>
          </span>
          <span className="dj-req-count">
            <span className="dj-req-count-label">{t('dj.queuedNow')}</span>
            <span className="dj-req-count-value">{queued}</span>
          </span>
        </div>
        <div className="dj-req-strip-side">
          {!session && !loading ? (
            <span className="dj-req-shift">
              <strong>{t('dj.noActiveShiftAssigned')}</strong>
              <span>{t('dj.assignYourself')}</span>
            </span>
          ) : null}
          <button
            type="button"
            className="btn btn-secondary btn-sm dj-req-intake"
            onClick={toggleIntake}
            disabled={!session}
            title={session ? undefined : t('dj.noActiveShiftAssigned')}
          >
            {paused ? t('dj.resumeIntake') : t('dj.pauseIntake')}
          </button>
        </div>
      </div>

      <div className="dj-req-toolbar">
        <label className="field dj-req-search">
          <span className="field-label">{t('dj.findTrack')}</span>
          <input
            ref={searchRef}
            className="field-input"
            name="dj-requests-search"
            autoComplete="off"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t('dj.searchQueuePlaceholder')}
          />
        </label>
        <div className="seg" role="group" aria-label={t('dj.filterByStatus')}>
          {FILTERS.map((value) => (
            <button
              key={value}
              type="button"
              className={`seg-item ${filter === value ? 'is-active' : ''}`.trim()}
              aria-pressed={filter === value}
              onClick={() => setFilter(value)}
            >
              {t(FILTER_KEY[value])}
            </button>
          ))}
        </div>
        <div className="seg" role="group" aria-label={t('dj.sortQueue')}>
          <button
            type="button"
            className={`seg-item ${sort === 'new' ? 'is-active' : ''}`.trim()}
            aria-pressed={sort === 'new'}
            onClick={() => {
              if (sort !== 'new') onSortChange('new');
            }}
          >
            {t('dj.sortNewest')}
          </button>
          <button
            type="button"
            className={`seg-item ${sort === 'hot' ? 'is-active' : ''}`.trim()}
            aria-pressed={sort === 'hot'}
            onClick={() => {
              if (sort !== 'hot') onSortChange('hot');
            }}
          >
            {t('dj.sortMostWanted')}
          </button>
        </div>
      </div>

      {visible.length > 0 ? <p className="dj-req-hint">{t('dj.keyboardHint')}</p> : null}

      {visible.length > 0 ? (
        <ul className="dj-req-list" aria-label={t('dj.guestRequests')}>
          {visible.map((item) => {
            const isNew = status.newIds.has(item.id);
            const reasonsOpen = reasonFor === item.id;
            const isPlaying = playingId === item.id;
            const copiedKind = copied && copied.id === item.id ? copied.kind : null;
            const trackText = trackClipboardText(item.artist, item.title);
            const isrc = item.isrc;
            const meta = [
              item.guestName,
              item.artist,
              item.style ?? t('dj.openFormat'),
              requestCountLabel(item.sameTrackCount, locale, t),
              arrivalClock(item.createdAt, locale, unavailable),
            ]
              .filter((part): part is string => Boolean(part))
              .join(' · ');

            return (
              <li
                key={item.id}
                ref={(node) => {
                  if (node) rowRefs.current.set(item.id, node);
                  else rowRefs.current.delete(item.id);
                }}
                className="dj-req-row"
                data-request-row=""
                data-request-id={item.id}
                data-active={activeId === item.id ? 'true' : undefined}
                tabIndex={-1}
                onFocus={() => setActiveId(item.id)}
              >
                {item.coverUrl ? (
                  <img className="dj-req-cover" src={item.coverUrl} alt="" width={40} height={40} loading="lazy" />
                ) : (
                  <span className="list-thumb dj-req-thumb" aria-hidden="true">
                    {item.title.trim().slice(0, 1).toUpperCase()}
                  </span>
                )}

                <div className="dj-req-main">
                  <div className="dj-req-titleline">
                    <span className="dj-req-title">{item.title}</span>
                    {item.vip ? <span className="chip chip-accent">{t('dj.vip')}</span> : null}
                    <span className={STATUS_TONE[item.status]}>{t(STATUS_KEY[item.status])}</span>
                  </div>
                  <div className="dj-req-sub">
                    <span className="dj-req-meta">{meta}</span>
                    {typeof item.tipMinor === 'number' && item.tipMinor > 0 ? (
                      <span className="chip chip-warn" title={t('dj.tipsShort')}>{money(item.tipMinor, locale)}</span>
                    ) : null}
                    {isNew ? <span className="chip chip-neutral">{t('dj.newBadge')}</span> : null}
                  </div>
                  {item.note ? <p className="dj-req-note" title={item.note}>{item.note}</p> : null}

                  <div className="dj-req-music">
                    <span className="dj-req-preview-slot" role="group" aria-label={t('dj.previewStreamLabel')}>
                      {item.previewUrl ? (
                        <button
                          type="button"
                          className="dj-req-preview"
                          aria-pressed={isPlaying}
                          onClick={() => togglePreview(item)}
                        >
                          <Icon name={isPlaying ? 'lucide:pause' : 'lucide:play'} size={13} />
                          {isPlaying ? t('dj.stopPreview') : t('dj.playPreview')}
                        </button>
                      ) : (
                        <span className="dj-req-preview is-off" title={t('dj.previewUnavailable')}>
                          {t('dj.previewUnavailable')}
                        </span>
                      )}
                    </span>

                    <span className="dj-req-music-group">
                      <span className="dj-req-music-label">{t('dj.musicSources')}</span>
                      {musicLinksFor(item.artist, item.title).map((link) => (
                        <a
                          key={link.id}
                          className="dj-req-service"
                          href={link.url}
                          target="_blank"
                          rel="noopener noreferrer"
                          aria-label={t('dj.findInService', { track: trackText, service: link.name })}
                        >
                          {link.name}
                        </a>
                      ))}
                    </span>

                    <span className="dj-req-music-group">
                      <button
                        type="button"
                        className={`dj-req-copy ${copiedKind === 'track' ? 'is-done' : ''}`.trim()}
                        onClick={() => void copy(trackText, item.id, 'track')}
                      >
                        {copiedKind === 'track' ? t('dj.trackCopied') : t('dj.copyTrack')}
                      </button>
                      {isrc ? (
                        <button
                          type="button"
                          className={`dj-req-copy ${copiedKind === 'isrc' ? 'is-done' : ''}`.trim()}
                          title={isrc}
                          onClick={() => void copy(isrc, item.id, 'isrc')}
                        >
                          {copiedKind === 'isrc' ? t('dj.isrcCopied') : t('dj.isrcCopy')}
                        </button>
                      ) : null}
                    </span>
                  </div>
                </div>

                <div className="dj-req-actions">
                  {item.status === 'pending' ? (
                    <>
                      <button
                        type="button"
                        className="btn btn-secondary btn-sm"
                        aria-expanded={reasonsOpen}
                        onClick={() => setReasonFor(reasonsOpen ? null : item.id)}
                      >
                        {t('dj.decline')}
                      </button>
                      <button type="button" className="btn btn-primary btn-sm" onClick={() => accept(item.id)}>
                        {t('dj.accept')}
                      </button>
                    </>
                  ) : (
                    <button type="button" className="btn btn-primary btn-sm" onClick={() => markPlayed(item.id)}>
                      <Icon name="lucide:play" size={13} />
                      {t('dj.markPlayed')}
                    </button>
                  )}
                </div>

                {reasonsOpen ? (
                  <div className="dj-req-reasons">
                    <span className="dj-req-reasons-label">{t('dj.rejectPrompt')}</span>
                    {REJECT_REASONS.map((key) => (
                      <button
                        key={key}
                        type="button"
                        className="btn btn-secondary btn-sm"
                        onClick={() => reject(item.id, t(key))}
                      >
                        {t(key)}
                      </button>
                    ))}
                    <button type="button" className="btn btn-quiet btn-sm" onClick={() => reject(item.id)}>
                      {t('dj.decline')}
                    </button>
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : loading && requests.length === 0 ? (
        <div className="panel-empty" role="status">
          <strong>{t('dj.loadingQueue')}</strong>
          <span>{t('dj.loadingQueueBody')}</span>
        </div>
      ) : requests.length === 0 ? (
        <div className="panel-empty">
          <strong>{t('dj.emptyQueueBody')}</strong>
        </div>
      ) : (
        <div className="panel-empty">
          <strong>{t('dj.emptyFilterTitle')}</strong>
          <span>{t('dj.emptyFilterBody')}</span>
        </div>
      )}

      {/* One element, owned by the surface: starting a preview stops the last
          one, and the row's mark is driven by the same state. */}
      <audio
        ref={audioRef}
        className="dj-req-audio"
        preload="none"
        aria-hidden="true"
        onEnded={stopPreview}
        onError={() => {
          stopPreview();
          onNotice(t('common.error'));
        }}
      />
    </div>
  );
}
