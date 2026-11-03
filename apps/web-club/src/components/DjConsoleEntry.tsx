/**
 * DjConsoleEntry — одна кнопка из панели клуба в консоль диджея.
 *
 * Зачем этот слой существует
 * ---------------------------
 * Консоль — поверхность роли `dj`: и маршрут (`RequireRole role="dj"`), и
 * каждый `/v1/dj/**` эндпоинт требуют DJ-токен. Клубный токен туда не
 * подходит, поэтому раньше переход по «Консоли диджея» в рельсе клуба
 * заканчивался экраном `/dj/login` — то есть вторым логином, которого у
 * сотрудника клуба нет.
 *
 * Теперь клуб сам выписывает себе DJ-сессию смены (`POST
 * /v1/club/dj-console/session`), а этот компонент делает это по факту
 * захода на `/dj`. Один клик в рельсе — и консоль открыта; если открыть
 * её нельзя, экран называет причину и страницу, которая её исправляет,
 * вместо пустого логина.
 *
 * Правила
 * -------
 *   - решение принимает сервер (`target` и `session` резолвят одного и
 *     того же диджея), а не панель по догадкам;
 *   - DJ, который вошёл сам, не проходит через handoff — у него уже есть
 *     DJ-сессия;
 *   - демо-сборка консоли локальная, без API, и handoff ей не нужен.
 */
import { useEffect, useState } from 'react';
import { Link, Navigate, Outlet, useLocation, useNavigate } from 'react-router-dom';
import BrandLogo from './BrandLogo';
import Icon from './Icon';
import {
  ApiRequestError,
  isAuthenticated,
  openDjConsole,
} from '../api/client';
import { IS_DEMO } from '../app-mode';
import { useLocale } from '../locale';
import './console-entry.css';

/** Причины отказа, для которых у экрана есть своё следующее действие. */
type BlockCode = 'no_open_shift' | 'no_djs' | 'dj_unavailable';

const BLOCK_COPY: Record<BlockCode, { title: string; body: string; to: string; action: string }> = {
  no_open_shift: {
    title: 'console.noShiftTitle',
    body: 'console.noShiftBody',
    to: '/sessions',
    action: 'console.openLiveRoom',
  },
  no_djs: {
    title: 'console.noDjsTitle',
    body: 'console.noDjsBody',
    to: '/djs',
    action: 'console.openRoster',
  },
  dj_unavailable: {
    title: 'console.djUnavailableTitle',
    body: 'console.djUnavailableBody',
    to: '/djs',
    action: 'console.openRoster',
  },
};

function isBlockCode(code: string): code is BlockCode {
  return code === 'no_open_shift' || code === 'no_djs' || code === 'dj_unavailable';
}

/** Активный диджей клуба, которого можно поставить за пульт. */
interface DjOption {
  id: string;
  displayName: string;
}

/**
 * `details.candidates` из 409 `dj_required` — активный состав клуба.
 *
 * Сервер присылает `[{id, display_name}]`; панель показывает те же имена,
 * что и в составе, поэтому переводятся только подписи кнопок, а не имена
 * людей.
 */
function readCandidates(err: ApiRequestError): DjOption[] {
  const raw = err.details?.candidates;
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((item) => {
    if (typeof item !== 'object' || item === null) return [];
    const record = item as Record<string, unknown>;
    const id = typeof record.id === 'string' ? record.id : null;
    const name = typeof record.display_name === 'string' ? record.display_name : null;
    return id && name ? [{ id, displayName: name }] : [];
  });
}

export default function DjConsoleEntry() {
  const location = useLocation();
  // Демо-сборка: консоль работает на локальных данных, DJ-сессии нет.
  if (IS_DEMO) return <Outlet />;
  if (isAuthenticated('dj')) return <Outlet />;
  if (!isAuthenticated('club')) {
    const from = `${location.pathname}${location.search}${location.hash}`;
    return <Navigate to="/dj/login" replace state={{ from }} />;
  }
  return <OpenConsole />;
}

type Phase =
  | { status: 'opening'; djName: string | null }
  | { status: 'ready' }
  | { status: 'failed'; statusCode: number; code: string; candidates: DjOption[] };

function OpenConsole() {
  const { t } = useLocale();
  const navigate = useNavigate();
  const [phase, setPhase] = useState<Phase>({ status: 'opening', djName: null });
  // `djId` is the person the operator picked for a shift that had no DJ; it is
  // part of the attempt so choosing somebody else re-runs the request.
  const [attempt, setAttempt] = useState<{ id: number; djId?: string; djName?: string }>({ id: 0 });

  useEffect(() => {
    let cancelled = false;
    setPhase({ status: 'opening', djName: attempt.djName ?? null });
    openDjConsole(attempt.djId)
      .then(() => {
        if (!cancelled) setPhase({ status: 'ready' });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        if (err instanceof ApiRequestError) {
          setPhase({
            status: 'failed',
            statusCode: err.status,
            code: err.code,
            candidates: readCandidates(err),
          });
        } else {
          setPhase({ status: 'failed', statusCode: 0, code: 'network', candidates: [] });
        }
      });
    return () => { cancelled = true; };
  }, [attempt]);

  // Готово — дальше рендерится настоящая консоль (DjLayout + страница).
  if (phase.status === 'ready') return <Outlet />;

  if (phase.status === 'opening') {
    return (
      <ConsoleScreen>
        <div className="panel-note" role="status">
          <Icon name="lucide:loader-circle" size={16} />
          <div>
            <strong>
              {phase.djName
                ? t('console.pickDjAction', { dj: phase.djName })
                : t('console.openingTitle')}
            </strong>
            <p className="console-entry-body">{t('console.openingBody')}</p>
          </div>
        </div>
      </ConsoleScreen>
    );
  }

  const blocked = isBlockCode(phase.code) ? BLOCK_COPY[phase.code] : null;
  // A shift with no DJ is not a dead end: the active roster is the choice, and
  // picking one assigns the shift and opens the console in the same request.
  const needsDjChoice = phase.code === 'dj_required';
  // 404/405 means the panel is newer than the server it talks to: the route
  // does not exist there yet. Saying "the API did not confirm" would blame a
  // failed action for what is really a version mismatch — and the fix is a
  // deploy, not a retry.
  const stale = phase.statusCode === 404 || phase.statusCode === 405;
  const title = needsDjChoice
    ? t('console.pickDjTitle')
    : blocked ? t(blocked.title) : stale ? t('console.staleTitle') : t('console.failedTitle');
  const body = needsDjChoice
    ? t('console.pickDjBody')
    : blocked ? t(blocked.body) : stale ? t('console.staleBody') : t('console.failedBody');

  return (
    <ConsoleScreen>
      <div className="panel-error" role="alert">
        <div>
          {/* The reason is the heading: "cannot open yet" says nothing a
              person can act on, while "No shift is open" is the next task. */}
          <strong>{title}</strong>
          <p className="console-entry-body">{body}</p>
          {phase.statusCode ? (
            <p className="console-entry-meta">{`HTTP ${phase.statusCode} • ${phase.code}`}</p>
          ) : null}
        </div>
      </div>
      <div className="console-entry-actions">
        {needsDjChoice ? (
          phase.status === 'failed' && phase.candidates.map((option) => (
            <button
              key={option.id}
              type="button"
              className="btn btn-primary"
              onClick={() => setAttempt((value) => ({
                id: value.id + 1,
                djId: option.id,
                djName: option.displayName,
              }))}
            >
              {t('console.pickDjAction', { dj: option.displayName })}
            </button>
          ))
        ) : blocked ? (
          <Link className="btn btn-primary" to={blocked.to}>{t(blocked.action)}</Link>
        ) : stale ? null : (
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => setAttempt((value) => ({ id: value.id + 1 }))}
          >
            {t('console.retry')}
          </button>
        )}
        <button
          type="button"
          className="btn btn-secondary"
          onClick={() => navigate('/dashboard')}
        >
          {t('console.backToClub')}
        </button>
      </div>
    </ConsoleScreen>
  );
}

/**
 * Каркас экрана входа: консоль ещё не открыта, поэтому это не DjLayout с
 * пустыми панелями — один блок по центру с тем же фоном, что у панели.
 */
function ConsoleScreen({ children }: { children: React.ReactNode }) {
  const { t } = useLocale();
  return (
    <div className="panel-shell club-app-shell">
      <div className="console-entry-page">
        {/* The way out exists even while the request is still in flight: a
            handoff that never settles must not trap the person on a spinner. */}
        <Link className="console-entry-brand" to="/dashboard">
          <BrandLogo size="small" />
          <span>{t('auth.clubPanel')}</span>
        </Link>
        <main className="console-entry">{children}</main>
      </div>
    </div>
  );
}