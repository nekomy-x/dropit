import { useCallback, useState } from 'react';
import {
  ApiRequestError,
  generateSessionQr,
  createSession,
  getDjs,
  getPricing,
  type Dj,
  type Page,
  type PricingPolicy,
  type QrGenerateResponse,
  type Session,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

interface QrState {
  qr: QrGenerateResponse | null;
  generating: boolean;
  error: string | null;
}

/**
 * Sessions page: open an evening session, then generate (rotate) its QR code.
 * QR rendering itself is a placeholder — a real encoder module arrives with
 * the F7 design pass; the placeholder reserves the exact spot for it.
 */
export default function SessionsPage() {
  const fetchDjs = useCallback(() => getDjs(), []);
  const fetchPricing = useCallback(() => getPricing(), []);
  const djs = useAsyncData<Page<Dj>>(fetchDjs);
  const pricing = useAsyncData<Page<PricingPolicy>>(fetchPricing);

  const [session, setSession] = useState<Session | null>(null);
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  const [qrState, setQrState] = useState<QrState>({
    qr: null,
    generating: false,
    error: null,
  });

  const activeDjs = djs.data?.items.filter((d) => d.status === 'active') ?? [];
  const currentPolicy =
    pricing.data?.items.find((p) => p.validTo === null) ??
    pricing.data?.items[0] ??
    null;

  const openSession = async () => {
    setCreateError(null);
    setCreating(true);
    try {
      const created = await createSession(
        {
          djId: activeDjs[0]?.id ?? null,
          pricingPolicyId: currentPolicy?.id ?? '',
          timezone: 'Europe/Madrid',
        },
        crypto.randomUUID(),
      );
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

  return (
    <div>
      <h1 className="page-title">Sessions</h1>
      <p className="page-subtitle">
        Open an evening session and generate the QR guests will scan.
      </p>

      {/* Reference data: DJs + current pricing policy */}
      <div className="mt-6">
        {djs.loading || pricing.loading ? (
          <Loading label="Loading session options…" />
        ) : null}
        {djs.error ? (
          <ErrorState
            message={djs.error.message}
            code={djs.error.code}
            onRetry={djs.reload}
          />
        ) : null}
        {pricing.error ? (
          <ErrorState
            message={pricing.error.message}
            code={pricing.error.code}
            onRetry={pricing.reload}
          />
        ) : null}
      </div>

      {!djs.loading && !pricing.loading && !djs.error && !pricing.error ? (
        <div className="mt-6 grid gap-6 lg:grid-cols-2">
          <section className="card">
            <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
              Tonight's session
            </h2>

            <dl className="mt-4 space-y-3 text-sm">
              <div className="flex justify-between gap-4">
                <dt className="text-neutral-400">DJ on shift</dt>
                <dd className="font-medium text-neutral-200">
                  {activeDjs[0]?.displayName ?? 'No active DJ invited yet'}
                </dd>
              </div>
              <div className="flex justify-between gap-4">
                <dt className="text-neutral-400">Price per request</dt>
                <dd className="font-medium text-neutral-200">
                  {currentPolicy
                    ? `${(currentPolicy.priceMinor / 100).toFixed(2)} EUR`
                    : 'No pricing policy yet'}
                </dd>
              </div>
            </dl>

            {session ? (
              <p className="mt-4 rounded-md bg-neutral-800 px-3 py-2 font-mono text-xs text-neutral-300">
                session {session.id} · {session.status}
              </p>
            ) : null}

            {createError ? (
              <p className="mt-3 text-sm text-red-300" role="alert">
                {createError}
              </p>
            ) : null}

            <button
              type="button"
              className="btn-primary mt-4"
              onClick={openSession}
              disabled={
                creating || activeDjs.length === 0 || currentPolicy === null
              }
            >
              {creating ? 'Opening…' : 'Open session'}
            </button>
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
                  Guests who scan this will see:{' '}
                  <span className="font-medium text-white">
                    {qrState.qr.guestPreview.clubName}
                  </span>{' '}
                  — DJ {qrState.qr.guestPreview.djName ?? '—'} ·{' '}
                  {(qrState.qr.guestPreview.priceMinor / 100).toFixed(2)}{' '}
                  {qrState.qr.guestPreview.currency} per request
                </p>
                <p className="font-mono text-xs text-neutral-500">
                  expires {new Date(qrState.qr.expiresAt).toLocaleString()}
                </p>
              </div>
            ) : (
              <p className="mt-4 text-sm text-neutral-400">
                Generate a QR after opening a session. The code rotates per
                session and can be deactivated at any time.
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
              disabled={!session || qrState.generating}
            >
              {qrState.generating ? 'Generating…' : 'Generate QR code'}
            </button>
          </section>
        </div>
      ) : null}
    </div>
  );
}
