import { useCallback, useState, type FormEvent } from 'react';
import {
  ApiRequestError,
  getDjs,
  inviteDj,
  NotImplementedStubError,
  type Dj,
  type DjInviteRequest,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

interface InviteState {
  submitting: boolean;
  error: string | null;
  success: boolean;
}

/** Waiting-for-backend notice: GET /v1/club/djs is not implemented yet. */
function WaitingForBackend() {
  return (
    <p
      className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
      role="status"
    >
      DJ roster is waiting for the backend — <code>GET /v1/club/djs</code> and{' '}
      <code>POST /v1/club/djs/invite</code> are not implemented yet (F3+).
    </p>
  );
}

/**
 * DJs page — reference-data CRUD is a "backend not implemented yet" surface:
 * both client methods reject locally with a 501 stub until the backend ships
 * the club DJ endpoints.
 */
export default function DjsPage() {
  const fetchDjs = useCallback(() => getDjs(), []);
  const { data, loading, error } = useAsyncData<{ items: Dj[] }>(fetchDjs);

  const [invite, setInvite] = useState<DjInviteRequest>({
    email: '',
    displayName: '',
  });
  const [inviteState, setInviteState] = useState<InviteState>({
    submitting: false,
    error: null,
    success: false,
  });

  const onInvite = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setInviteState({ submitting: true, error: null, success: false });
    try {
      await inviteDj(invite);
      setInvite({ email: '', displayName: '' });
      setInviteState({ submitting: false, error: null, success: true });
    } catch (err) {
      setInviteState({
        submitting: false,
        error:
          err instanceof ApiRequestError || err instanceof NotImplementedStubError
            ? err.message
            : 'Invite failed',
        success: false,
      });
    }
  };

  return (
    <div>
      <h1 className="page-title">DJs</h1>
      <p className="page-subtitle">
        Invite DJs to your club. They sign in with email + password and 2FA.
      </p>

      <div className="mt-6 grid gap-6 lg:grid-cols-3">
        <section className="card lg:col-span-2">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
            Your DJs
          </h2>
          <div className="mt-4">
            {loading ? <Loading label="Loading DJs…" /> : null}
            {error ? (
              error.notImplemented ? (
                <WaitingForBackend />
              ) : (
                <ErrorState message={error.message} code={error.code} />
              )
            ) : null}
            {data && data.items.length === 0 ? (
              <p className="text-sm text-neutral-400">No DJs yet.</p>
            ) : null}
          </div>
        </section>

        <section className="card">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
            Invite a DJ
          </h2>
          <form onSubmit={onInvite} className="mt-4 space-y-4" noValidate>
            <div>
              <label htmlFor="dj-name" className="field-label">
                Display name
              </label>
              <input
                id="dj-name"
                type="text"
                required
                className="field-input"
                value={invite.displayName}
                onChange={(e) =>
                  setInvite({ ...invite, displayName: e.target.value })
                }
              />
            </div>
            <div>
              <label htmlFor="dj-email" className="field-label">
                Email
              </label>
              <input
                id="dj-email"
                type="email"
                required
                className="field-input"
                value={invite.email}
                onChange={(e) => setInvite({ ...invite, email: e.target.value })}
              />
            </div>

            {inviteState.error ? (
              <p className="text-sm text-red-300" role="alert">
                {inviteState.error}
              </p>
            ) : null}

            <button
              type="submit"
              className="btn-primary w-full"
              disabled={
                inviteState.submitting ||
                invite.email === '' ||
                invite.displayName === ''
              }
            >
              {inviteState.submitting ? 'Inviting…' : 'Send invitation'}
            </button>

            <p className="text-xs text-neutral-500">
              Invitations are a backend-not-implemented stub — the request is
              rejected locally (501) until the endpoint ships.
            </p>
          </form>
        </section>
      </div>
    </div>
  );
}
