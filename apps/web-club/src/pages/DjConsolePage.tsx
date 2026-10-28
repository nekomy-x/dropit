/**
 * DJ console — the page inside the DJ shell.
 *
 * The shell (DjLayout) owns the rail, the top bar and the theme toggle; this
 * page owns the page head, the intake state and the three views. Everything is
 * built from the shared kit so both themes work.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import Icon from '../components/Icon';
import { useLocale } from '../locale';
import { useDjConsoleData } from '../dj-console/useDjConsoleData';
import { useCommandPalette } from '../features/dj-console/powerFeatures';
import { DjQueueView, DjRoomControlView, DjSessionDetailsView, type DjConsoleTab } from './DJConsoleViews';
import './dj-console.css';

const TITLE_KEYS: Record<DjConsoleTab, string> = {
  queue: 'dj.queueTitle',
  room: 'dj.roomTitle',
  session: 'dj.sessionTitle',
};

const ROOM_MODES = [
  { value: 'Warm-up', labelKey: 'dj.modeWarmUp' },
  { value: 'Peak time', labelKey: 'dj.modePeakTime' },
  { value: 'Closing soon', labelKey: 'dj.modeClosingSoon' },
] as const;

export default function DjConsolePage() {
  const { t } = useLocale();
  const [notice, setNotice] = useState<string | null>(null);
  const showNotice = useCallback((message: string) => setNotice(message), []);
  const { dj, loading, error, live, reload, sort } = useDjConsoleData(showNotice, t);
  const [activeTab, setActiveTab] = useState<DjConsoleTab>(() => {
    const hash = window.location.hash.slice(1);
    return hash === 'room' ? 'room' : hash === 'session' || hash === 'history' ? 'session' : 'queue';
  });
  const [roomMode, setRoomMode] = useState<'Warm-up' | 'Peak time' | 'Closing soon'>('Peak time');
  const roomPaused = dj.state.session?.status === 'paused_acceptance';
  const session = dj.state.session;

  useEffect(() => {
    if (!notice) return undefined;
    const timer = window.setTimeout(() => setNotice(null), 3000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  useEffect(() => {
    const sync = () => {
      const hash = window.location.hash.slice(1);
      if (hash === 'room') setActiveTab('room');
      else if (hash === 'session' || hash === 'history') setActiveTab('session');
      else if (hash === 'queue' || hash === 'requests') setActiveTab('queue');
    };
    window.addEventListener('hashchange', sync);
    return () => window.removeEventListener('hashchange', sync);
  }, []);

  const setTab = useCallback((tab: DjConsoleTab) => {
    setActiveTab(tab);
    window.history.pushState(null, '', `#${tab}`);
    window.dispatchEvent(new Event('hashchange'));
  }, []);

  const commands = useMemo(() => [
    { id: 'queue', label: t('dj.cmdQueue'), keywords: [t('dj.cmdQueueKeywords')], shortcut: 'Q', onSelect: () => setTab('queue') },
    { id: 'room', label: t('dj.cmdRoom'), keywords: [t('dj.cmdRoomKeywords')], shortcut: 'R', onSelect: () => setTab('room') },
    { id: 'session', label: t('dj.cmdSession'), keywords: [t('dj.cmdSessionKeywords')], shortcut: 'S', onSelect: () => setTab('session') },
    { id: 'pause', label: roomPaused ? t('dj.cmdResume') : t('dj.cmdPause'), keywords: [t('dj.cmdPauseKeywords')], onSelect: dj.toggleAcceptance },
    { id: 'reset', label: t('dj.cmdReset'), keywords: [t('dj.cmdResetKeywords')], onSelect: dj.reset },
  ], [dj, roomPaused, setTab, t]);
  const palette = useCommandPalette(commands);

  const heading = TITLE_KEYS[activeTab];
  const statusLabel = error ? t('dj.apiUnavailable') : loading ? t('dj.syncing') : live ? t('dj.apiReady') : t('dj.demoData');
  /* The reading is the chip: a word with a tone. The old 7px dot was a second
     copy of the same fact, and it read as noise next to the words. */
  const statusTone = error ? 'chip-error' : loading ? 'chip-neutral' : live ? 'chip-accent' : 'chip-warn';

  return (
    <div className="dj-console">
      <header className="panel-page-head">
        <div style={{ minWidth: 0 }}>
          <h1 className="panel-page-title">{t(heading)}</h1>
        </div>
        <div className="panel-page-actions">
          <span className={`chip ${statusTone}`}>{statusLabel}</span>
          <button type="button" className="btn btn-secondary" onClick={palette.open}>
            <Icon name="lucide:search" size={15} />
            {t('dj.findAction')}
            <kbd className="dj-kbd" aria-hidden="true">⌘K</kbd>
          </button>
          {live ? (
            <button type="button" className="btn btn-secondary" onClick={() => reload()} disabled={loading}>
              <Icon name="lucide:refresh-cw" size={15} />
              {t('common.refresh')}
            </button>
          ) : null}
        </div>
      </header>

      <section className="dj-intake-bar" aria-label={live ? t('dj.liveRequestIntake') : t('dj.demoRoomControls')}>
        <div className="dj-intake-copy">
          <span className="dj-intake-text">
            <strong>
              {!session ? t('dj.noActiveShiftAssigned') : roomPaused ? t('dj.requestsPaused') : live ? t('dj.takingRequests') : t('dj.demoRequestsOpen')}
            </strong>
            <span>
              {session
                ? t('dj.sessionId', { id: session.id })
                : live ? t('dj.assignYourself') : t('dj.previewOnlyChanges')}
            </span>
          </span>
        </div>
        <div className="dj-intake-actions">
          <button
            type="button"
            className={roomPaused ? 'btn btn-primary' : 'btn btn-secondary'}
            disabled={loading}
            onClick={() => dj.toggleAcceptance()}
          >
            {roomPaused ? t('dj.resumeIntake') : t('dj.pauseIntake')}
          </button>
          {!live ? (
            <div className="seg" role="group" aria-label={t('dj.setRoomMode')}>
              {ROOM_MODES.map((mode) => (
                <button
                  type="button"
                  key={mode.value}
                  className={`seg-item ${roomMode === mode.value ? 'is-active' : ''}`}
                  aria-pressed={roomMode === mode.value}
                  onClick={() => setRoomMode(mode.value)}
                >
                  {t(mode.labelKey)}
                </button>
              ))}
            </div>
          ) : null}
        </div>
      </section>

      {error ? (
        <div className="panel-error" role="alert" style={{ marginTop: 16 }}>
          <div>
            <strong>{t('dj.loadErrorTitle')}</strong>
            <span className="dj-error-copy">{error}</span>
          </div>
          <button type="button" className="btn btn-secondary btn-sm" onClick={() => reload()}>{t('common.tryAgain')}</button>
        </div>
      ) : null}

      {loading ? (
        <div className="panel-note" role="status" style={{ marginTop: 16 }}>
          <Icon name="lucide:loader-circle" size={15} />
          <span>{t('dj.loadingConsole')}</span>
        </div>
      ) : null}

      <div className="panel-section">
        {activeTab === 'queue' ? (
          <div id="dj-queue-panel"><DjQueueView dj={dj} onNotice={showNotice} live={live} loading={loading} sort={sort} onSortChange={(nextSort) => reload(nextSort)} /></div>
        ) : activeTab === 'room' ? (
          <div id="dj-room-panel"><DjRoomControlView dj={dj} onNotice={showNotice} live={live} /></div>
        ) : (
          <div id="dj-session-panel"><DjSessionDetailsView dj={dj} onNotice={showNotice} live={live} /></div>
        )}
      </div>

      {notice ? (
        <div className="dj-toast" role="status" aria-live="polite">
          <span>{notice}</span>
          <button type="button" onClick={() => setNotice(null)} aria-label={t('dj.closeNotification')}><Icon name="lucide:x" size={14} /></button>
        </div>
      ) : null}

      {palette.isOpen ? (
        <div className="dj-palette-backdrop" onMouseDown={palette.close}>
          <div className="dj-palette" onMouseDown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-label={t('dj.commandPalette')}>
            <div className="dj-palette-search">
              <Icon name="lucide:search" size={17} />
              <input
                autoFocus
                value={palette.query}
                onChange={(event) => palette.setQuery(event.target.value)}
                placeholder={t('dj.searchCommandsPlaceholder')}
                aria-label={t('dj.searchCommands')}
                name="command-search"
                autoComplete="off"
              />
              <kbd className="dj-kbd">ESC</kbd>
            </div>
            <div className="dj-palette-list">
              {palette.filteredCommands.map((command) => (
                <button type="button" key={command.id} className="dj-palette-item" onClick={() => palette.run(command)}>
                  <span>{command.label}</span>
                  {command.shortcut ? <kbd className="dj-palette-kbd">{command.shortcut}</kbd> : <Icon name="lucide:arrow-right" size={14} />}
                </button>
              ))}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
