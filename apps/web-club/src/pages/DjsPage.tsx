import { useCallback, useState, type FormEvent } from 'react';
import {
  ApiRequestError,
  getDjs,
  inviteDj,
  type Dj,
  type DjInviteRequest,
  type Page,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

interface InviteState {
  submitting: boolean;
  error: string | null;
  success: boolean;
}

/** DJs page: list of invited DJs + invite form (email is sent by the backend). */
export default function DjsPage() {
  const fetchDjs = useCallback(() => getDjs(), []);
  const { data, loading, error, reload } = useAsyncData<Page<Dj>>(fetchDjs);

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
      reload();
    } catch (err) {
      setInviteState({
        submitting: false,
        error:
          err instanceof ApiRequestError ? err.message : 'Invite failed',
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
              <ErrorState
                message={error.message}
                code={error.code}
                onRetry={reload}
              />
            ) : null}
            {data ? (
              data.items.length === 0 ? (
                <p className="text-sm text-neutral-400">
                  No DJs yet — invite your first DJ using the form.
                </p>
              ) : (
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-neutral-800 text-left text-xs uppercase tracking-wider text-neutral-500">
                      <th className="py-2 pr-4 font-medium">Name</th>
                      <th className="py-2 pr-4 font-medium">Email</th>
                      <th className="py-2 pr-4 font-medium">Stripe</th>
                      <th className="py-2 font-medium">Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.items.map((dj) => (
                      <tr
                        key={dj.id}
                        className="border-b border-neutral-800/60"
                      >
                        <td className="py-2.5 pr-4 font-medium text-neutral-100">
                          {dj.displayName}
                        </td>
                        <td className="py-2.5 pr-4 text-neutral-400">
                          {dj.email}
                        </td>
                        <td className="py-2.5 pr-4 text-neutral-400">
                          {dj.stripeOnboardingStatus}
                        </td>
                        <td className="py-2.5">
                          <span className="rounded-full bg-neutral-800 px-2.5 py-0.5 text-xs font-semibold uppercase tracking-wider text-neutral-300">
                            {dj.status}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )
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
            {inviteState.success ? (
              <p className="text-sm text-green-300" role="status">
                Invitation sent. The DJ will receive an email with sign-up
                instructions.
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
          </form>
        </section>
      </div>
    </div>
  );
}
