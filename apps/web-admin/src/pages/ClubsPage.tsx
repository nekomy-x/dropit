import { useCallback, useState } from 'react';
import {
  ApiRequestError,
  createClub,
  listClubs,
  patchClub,
  type Club,
  type ClubCreateInput,
  type ClubStatus,
  type Page,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

const STATUS_ACTIONS: ReadonlyArray<{ to: ClubStatus; label: string }> = [
  { to: 'active', label: 'Activate' },
  { to: 'suspended', label: 'Suspend' },
  { to: 'blocked', label: 'Block' },
];

/** Clubs management: list + create form + status actions (CRUD /v1/admin/clubs). */
export default function ClubsPage() {
  const fetchClubs = useCallback(() => listClubs(), []);
  const { data, loading, error, reload } = useAsyncData<Page<Club>>(fetchClubs);

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

  const onCreate = async () => {
    setSubmitError(null);
    setSubmitting(true);
    try {
      await createClub(form);
      setForm({
        name: '',
        slug: '',
        email: '',
        address: '',
        country: '',
        phone: '',
      });
      reload();
    } catch (err) {
      setSubmitError(
        err instanceof ApiRequestError ? err.message : 'Could not create club',
      );
    } finally {
      setSubmitting(false);
    }
  };

  const onStatus = async (club: Club, status: ClubStatus) => {
    setActionError(null);
    try {
      await patchClub(club.id, { status });
      reload();
    } catch (err) {
      setActionError(
        err instanceof ApiRequestError
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

      {actionError ? (
        <p className="mt-4 text-sm text-red-300" role="alert">
          {actionError}
        </p>
      ) : null}

      <div className="mt-6">
        {loading ? <Loading label="Loading clubs…" /> : null}
        {error ? (
          <ErrorState
            message={error.message}
            code={error.code}
            onRetry={reload}
          />
        ) : null}
        {data ? (
          data.items.length === 0 ? (
            <p className="text-sm text-neutral-400">No clubs registered yet.</p>
          ) : (
            <div className="card overflow-x-auto p-0">
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
                    <tr
                      key={club.id}
                      className="border-b border-neutral-800/60"
                    >
                      <td className="py-3 px-6">
                        <p className="font-medium text-neutral-100">
                          {club.name}
                        </p>
                        <p className="text-xs text-neutral-500">{club.email}</p>
                      </td>
                      <td className="py-3 px-4 text-neutral-400">
                        {club.country}
                      </td>
                      <td className="py-3 px-4 text-neutral-400">
                        {club.stripeOnboardingStatus}
                      </td>
                      <td className="py-3 px-4">
                        <span
                          className={`rounded-full px-2.5 py-0.5 text-xs font-semibold uppercase tracking-wider ${
                            club.status === 'active'
                              ? 'bg-green-900/40 text-green-300'
                              : 'bg-red-900/40 text-red-300'
                          }`}
                        >
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
          )
        ) : null}
      </div>

      <section className="card mt-8">
        <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
          Register a new club
        </h2>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void onCreate();
          }}
          className="mt-4 grid gap-4 md:grid-cols-3"
          noValidate
        >
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
