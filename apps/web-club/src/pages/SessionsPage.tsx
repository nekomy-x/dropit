import { useState } from 'react';
import {
  ApiRequestError,
  generateSessionQr,
  createSession,
  closeSession,
  type QrGenerateResponse,
  type SessionResponse,
} from '../api/client';

interface QrState {
  qr: QrGenerateResponse | null;
  generating: boolean;
  error: string | null;
}

/**
 * Sessions page — the LIVE club flow (F2 backend):
 *   POST /v1/club/sessions {timezone, dj_id?} → SessionResponse (201)
 *   POST /v1/club/sessions/:id/qr/generate → {session_id, qr_token, expires_at}
 *   PATCH /v1/club/sessions/:id/close → SessionResponse
 * QR rendering itself is a placeholder — the encoder module arrives with
 * the design pass; the raw qr_token is shown once (server stores sha256).
 * DJs/pricing reference data (`GET /v1/club/djs`, `/club/pricing`) is not
 * implemented on the backend yet, so DJ/price are optional free inputs.
 */
const DEFAULT_TIMEZONE = 'Europe/Madrid';

export default function SessionsPage() {
  const [djId, setDjId] = useState('');
  const [timezone, setTimezone] = useState(DEFAULT_TIMEZONE);

  const [session, setSession] = useState<SessionResponse | null>(null);
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  const [closing, setClosing] = useState(false);
  const [closeError, setCloseError] = useState<string | null>(null);

  const [qrState, setQrState] = useState<QrState>({
    qr: null,
    generating: false,
    error: null,
  });

  const openSession = async () => {
    setCreateError(null);
    setCloseError(null);
    setCreating(true);
    try {
      const created = await createSession({
        timezone: timezone.trim() || DEFAULT_TIMEZONE,
        // dj_id optional: unassigned until a DJ is attached (F2 contract).
        dj_id: djId.trim() === '' ? undefined : djId.trim(),
      });
      setSession(created);
      setQrState({ qr: null, generating: false, error: null });
    } catch (err) {
      setCreateError(
        err instanceof ApiRequestError ? err.message : 'Could not open a session',
      );
    } finally {
      setCreating(false);
    }
  };

  const generateQr = async () => {
    if (!session) return;
    setQrState((prev) => ({ ...prev, generating: true, error: null }));
    try {
      const qr = await generateSessionQr(session.id);
      setQrState({ qr, generating: false, error: null });
    } catch (err) {
      setQrState((prev) => ({
        ...prev,
        generating: false,
        error:
          err instanceof ApiRequestError ? err.message : 'QR generation failed',
      }));
    }
  };

  const closeShift = async () => {
    if (!session) return;
    setCloseError(null);
    setClosing(true);
    try {
      const closed = await closeSession(session.id);
      setSession(closed);
      setQrState({ qr: null, generating: false, error: null });
    } catch (err) {
      setCloseError(
        err instanceof ApiRequestError ? err.message : 'Could not close the session',
      );
    } finally {
      setClosing(false);
    }
  };

  const sessionClosed = session?.status === 'closed';

  return (
    <div>
      <h1 className="page-title">Sessions</h1>
      <p className="page-subtitle">
        Open an evening session, generate the QR guests will scan, and close
        the shift at the end of the night.
      </p>

      <div className="mt-6 grid gap-6 lg:grid-cols-2">
        <section className="card">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
            Tonight's session
          </h2>

          <div className="mt-4 space-y-4">
            <div>
              <label htmlFor="session-timezone" className="field-label">
                Timezone (IANA)
              </label>
              <input
                id="session-timezone"
                type="text"
                required
                placeholder={DEFAULT_TIMEZONE}
                className="field-input"
                value={timezone}
                onChange={(e) => setTimezone(e.target.value)}
                disabled={session !== null}
              />
              <p className="mt-1 text-xs text-neutral-500">
                Planned auto-close: next 07:00 in this zone (DST-safe).
              </p>
            </div>
            <div>
              <label htmlFor="session-dj" className="field-label">
                DJ on shift (optional UUID)
              </label>
              <input
                id="session-dj"
                type="text"
                placeholder="not assigned"
                className="field-input"
                value={djId}
                onChange={(e) => setDjId(e.target.value)}
                disabled={session !== null}
              />
              <p className="mt-1 text-xs text-neutral-500">
                Leave empty to open the shift without a DJ; the DJ list
                endpoint lands with the backend reference data (F3+).
              </p>
            </div>
          </div>

          {session ? (
            <p className="mt-4 rounded-md bg-neutral-800 px-3 py-2 font-mono text-xs text-neutral-300">
              session {session.id} · {session.status}
              {session.dj_id ? ` · dj ${session.dj_id}` : ''}
            </p>
          ) : null}

          {createError ? (
            <p className="mt-3 text-sm text-red-300" role="alert">
              {createError}
            </p>
          ) : null}

          {session ? (
            <button
              type="button"
              className="btn-secondary mt-4"
              onClick={closeShift}
              disabled={closing || sessionClosed}
            >
              {closing
                ? 'Closing…'
                : sessionClosed
                  ? 'Session closed'
                  : 'Close session'}
            </button>
          ) : (
            <button
              type="button"
              className="btn-primary mt-4"
              onClick={openSession}
              disabled={creating || timezone.trim() === ''}
            >
              {creating ? 'Opening…' : 'Open session'}
            </button>
          )}

          {closeError ? (
            <p className="mt-3 text-sm text-red-300" role="alert">
              {closeError}
            </p>
          ) : null}
        </section>

        <section className="card">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
            QR code
          </h2>

          {qrState.qr ? (
            <div className="mt-4 space-y-3">
              {/* QR placeholder: real encoder module lands with the design pass. */}
              <div
                className="flex aspect-square w-56 items-center justify-center rounded-lg border-2 border-dashed border-neutral-700 bg-neutral-950 text-center text-xs text-neutral-500"
                aria-label="QR code placeholder"
              >
                QR image will appear here
              </div>
              <p className="text-sm text-neutral-300">
                Show the token to guests (rendered as a QR image once the
                encoder module lands):
              </p>
              <p className="break-all rounded-md bg-neutral-800 px-3 py-2 font-mono text-xs text-neutral-300">
                {qrState.qr.qr_token}
              </p>
              <p className="font-mono text-xs text-neutral-500">
                expires {new Date(qrState.qr.expires_at).toLocaleString()}
              </p>
              <p className="text-xs text-neutral-500">
                The token is shown once — the server stores only its hash.
                Regenerating rotates it; the old token stops working.
              </p>
            </div>
          ) : (
            <p className="mt-4 text-sm text-neutral-400">
              {session && !sessionClosed
                ? 'Generate a QR for the open session. The code rotates per session and old tokens are rejected.'
                : 'Generate a QR after opening a session.'}
            </p>
          )}

          {qrState.error ? (
            <p className="mt-3 text-sm text-red-300" role="alert">
              {qrState.error}
            </p>
          ) : null}

          <button
            type="button"
            className="btn-secondary mt-4"
            onClick={generateQr}
            disabled={!session || sessionClosed || qrState.generating}
          >
            {qrState.qr ? 'Rotate QR code' : 'Generate QR code'}
            {qrState.generating ? '…' : ''}
          </button>
        </section>
      </div>
    </div>
  );
}
