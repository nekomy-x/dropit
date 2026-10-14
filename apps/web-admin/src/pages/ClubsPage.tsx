import { useCallback, useState, type FormEvent } from 'react';
import {
  ApiRequestError,
  createClub,
  listClubs,
  patchClub,
  NotImplementedStubError,
  type Club,
  type ClubCreateInput,
  type ClubStatus,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';

const STATUS_ACTIONS: ReadonlyArray<{ to: ClubStatus; label: string }> = [
  { to: 'active', label: 'Activate' },
  { to: 'suspended', label: 'Suspend' },
  { to: 'blocked', label: 'Block' },
];

/**
 * Clubs management — "backend not implemented yet": CRUD /v1/admin/clubs
 * has no backend routes yet, so the client methods reject locally with a
 * 501 stub and the page renders a waiting-for-backend empty state.
 */
export default function ClubsPage() {
  const fetchClubs = useCallback(() => listClubs(), []);
  const { data, loading, error } = useAsyncData<{ items: Club[] }>(fetchClubs);

  const [form, setForm] = useState<ClubCreateInput>({
    name: '',
    slug: '',
    email: '',
    address: '',
    country: '',
    phone: '',
  });
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const onCreate = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSubmitError(null);
    setSubmitting(true);
    try {
      await createClub(form);
      setForm({ name: '', slug: '', email: '', address: '', country: '', phone: '' });
    } catch (err) {
      setSubmitError(
        err instanceof ApiRequestError || err instanceof NotImplementedStubError
          ? err.message
          : 'Could not create club',
      );
    } finally {
      setSubmitting(false);
    }
  };

  const onStatus = async (club: Club, status: ClubStatus) => {
    setActionError(null);
    try {
      await patchClub(club.id, { status });
    } catch (err) {
      setActionError(
        err instanceof ApiRequestError || err instanceof NotImplementedStubError
          ? err.message
          : 'Status change failed',
      );
    }
  };

  const formIncomplete =
    form.name === '' || form.slug === '' || form.email === '';

  return (
    <div>
      <h1 className="page-title">Clubs</h1>
      <p className="page-subtitle">
        Verify, suspend or block clubs. Blocking stops new guest requests
        immediately.
      </p>

      {loading ? (
        <p className="mt-6 text-sm text-neutral-400">Loading clubs…</p>
      ) : null}

      {error ? (
        error.notImplemented ? (
          <p
            className="mt-6 rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
            role="status"
          >
            Club management is waiting for the backend — CRUD under{' '}
            <code>/v1/admin/clubs</code> is not implemented yet. The form
            below is a local 501 stub.
          </p>
        ) : (
          <p className="mt-6 text-sm text-red-300" role="alert">
            {error.message}
            {error.code ? ` (${error.code})` : ''}
          </p>
        )
      ) : null}

      {data && data.items.length > 0 ? (
        <div className="card mt-6 overflow-x-auto p-0">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-neutral-800 text-left text-xs uppercase tracking-wider text-neutral-500">
                <th className="py-3 px-6 font-medium">Club</th>
                <th className="py-3 px-4 font-medium">Country</th>
                <th className="py-3 px-4 font-medium">Stripe</th>
                <th className="py-3 px-4 font-medium">Status</th>
                <th className="py-3 px-6 font-medium">Actions</th>
              </tr>
            </thead>
            <tbody>
              {data.items.map((club) => (
                <tr key={club.id} className="border-b border-neutral-800/60">
                  <td className="py-3 px-6">
                    <p className="font-medium text-neutral-100">{club.name}</p>
                    <p className="text-xs text-neutral-500">{club.email}</p>
                  </td>
                  <td className="py-3 px-4 text-neutral-400">{club.country}</td>
                  <td className="py-3 px-4 text-neutral-400">
                    {club.stripeOnboardingStatus}
                  </td>
                  <td className="py-3 px-4">
                    <span className="rounded-full bg-neutral-800 px-2.5 py-0.5 text-xs font-semibold uppercase tracking-wider text-neutral-300">
                      {club.status}
                    </span>
                  </td>
                  <td className="py-3 px-6">
                    <div className="flex gap-2">
                      {STATUS_ACTIONS.filter((a) => a.to !== club.status).map(
                        (action) => (
                          <button
                            key={action.to}
                            type="button"
                            className="btn-secondary px-2 py-1 text-xs"
                            onClick={() => onStatus(club, action.to)}
                          >
                            {action.label}
                          </button>
                        ),
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}

      {actionError ? (
        <p className="mt-4 text-sm text-red-300" role="alert">
          {actionError}
        </p>
      ) : null}

      <section className="card mt-8">
        <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
          Register a new club
        </h2>
        <form onSubmit={onCreate} className="mt-4 grid gap-4 md:grid-cols-3" noValidate>
          <div>
            <label htmlFor="club-name" className="field-label">
              Name
            </label>
            <input
              id="club-name"
              type="text"
              required
              className="field-input"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
            />
          </div>
          <div>
            <label htmlFor="club-slug" className="field-label">
              Slug
            </label>
            <input
              id="club-slug"
              type="text"
              required
              placeholder="la-luz"
              className="field-input"
              value={form.slug}
              onChange={(e) => setForm({ ...form, slug: e.target.value })}
            />
          </div>
          <div>
            <label htmlFor="club-email" className="field-label">
              Email
            </label>
            <input
              id="club-email"
              type="email"
              required
              className="field-input"
              value={form.email}
              onChange={(e) => setForm({ ...form, email: e.target.value })}
            />
          </div>
          <div>
            <label htmlFor="club-address" className="field-label">
              Address
            </label>
            <input
              id="club-address"
              type="text"
              className="field-input"
              value={form.address}
              onChange={(e) => setForm({ ...form, address: e.target.value })}
            />
          </div>
          <div>
            <label htmlFor="club-country" className="field-label">
              Country
            </label>
            <input
              id="club-country"
              type="text"
              placeholder="ES"
              className="field-input"
              value={form.country}
              onChange={(e) => setForm({ ...form, country: e.target.value })}
            />
          </div>
          <div>
            <label htmlFor="club-phone" className="field-label">
              Phone
            </label>
            <input
              id="club-phone"
              type="tel"
              className="field-input"
              value={form.phone}
              onChange={(e) => setForm({ ...form, phone: e.target.value })}
            />
          </div>

          {submitError ? (
            <p className="text-sm text-red-300 md:col-span-3" role="alert">
              {submitError}
            </p>
          ) : null}

          <div className="md:col-span-3">
            <button
              type="submit"
              className="btn-primary"
              disabled={submitting || formIncomplete}
            >
              {submitting ? 'Creating…' : 'Create club'}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}
