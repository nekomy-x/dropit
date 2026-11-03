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
import RequestsWorkspace from '../components/dj-console/RequestsWorkspace';
import { DjRoomControlView, DjSessionDetailsView, type DjConsoleTab } from './DJConsoleViews';
import './dj-console.css';

const TITLE_KEYS: Record<DjConsoleTab, string> = {
  requests: 'dj.queueTitle',
  room: 'dj.roomTitle',
  session: 'dj.sessionTitle',
};

/**
 * `#requests` is the canonical deep link; `#queue` is what the console used
 * before the surface was rebuilt, and old links (a bookmark, a club's own
 * shortcut) must keep landing on the same screen.
 */
function tabFromHash(hash: string): DjConsoleTab {
  const value = hash.replace(/^#/, '');
  if (value === 'room') return 'room';
  if (value === 'session' || value === 'history') return 'session';
  return 'requests';
}

export default function DjConsolePage() {
  const { t } = useLocale();
  const [notice, setNotice] = useState<string | null>(null);
  const showNotice = useCallback((message: string) => setNotice(message), []);
  const { dj, loading, error, live, reload, sort, status } = useDjConsoleData(showNotice, t);
  const [activeTab, setActiveTab] = useState<DjConsoleTab>(() => tabFromHash(window.location.hash));
  const roomPaused = dj.state.session?.status === 'paused_acceptance';

  useEffect(() => {
    if (!notice) return undefined;
    const timer = window.setTimeout(() => setNotice(null), 3000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  useEffect(() => {
    const sync = () => setActiveTab(tabFromHash(window.location.hash));
    window.addEventListener('hashchange', sync);
    return () => window.removeEventListener('hashchange', sync);
  }, []);

  const setTab = useCallback((tab: DjConsoleTab) => {
    setActiveTab(tab);
    window.history.pushState(null, '', `#${tab}`);
    window.dispatchEvent(new Event('hashchange'));
  }, []);

  const commands = useMemo(() => [
    { id: 'requests', label: t('dj.cmdQueue'), keywords: [t('dj.cmdQueueKeywords')], shortcut: 'Q', onSelect: () => setTab('requests') },
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
          {/* The requests tab carries the same reading in its sticky strip, where
              it stays visible while the queue scrolls; two copies of one fact on
              one screen is the noise the console keeps removing. */}
          {activeTab === 'requests' ? null : <span className={`chip ${statusTone}`}>{statusLabel}</span>}
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
        {activeTab === 'requests' ? (
          <div id="dj-queue-panel">
            <RequestsWorkspace
              dj={dj}
              onNotice={showNotice}
              live={live}
              loading={loading}
              sort={sort}
              onSortChange={(nextSort) => reload(nextSort)}
              status={status}
            />
          </div>
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
