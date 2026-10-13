import { useCallback } from 'react';
import { getProfile, type ClubProfile } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';

/** GET /v1/club/profile — overview card for the club home screen. */
export default function DashboardPage() {
  const fetchProfile = useCallback(() => getProfile(), []);
  const { data, loading, error, reload } = useAsyncData<ClubProfile>(fetchProfile);

  return (
    <div>
      <h1 className="page-title">Dashboard</h1>
      <p className="page-subtitle">
        Club overview. Live session metrics arrive with the F2 backend.
      </p>

      <div className="mt-6">
        {loading ? <Loading label="Loading club profile…" /> : null}

        {error ? (
          <ErrorState
            message={error.message}
            code={error.code}
            onRetry={reload}
          />
        ) : null}

        {data ? (
          <div className="grid gap-6 md:grid-cols-2">
            <section className="card">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Club profile
              </h2>
              <dl className="mt-4 space-y-3 text-sm">
                <div className="flex justify-between gap-4">
                  <dt className="text-neutral-400">Name</dt>
                  <dd className="font-medium text-white">{data.name}</dd>
                </div>
                <div className="flex justify-between gap-4">
                  <dt className="text-neutral-400">Address</dt>
                  <dd className="font-medium text-neutral-200">{data.address}</dd>
                </div>
                <div className="flex justify-between gap-4">
                  <dt className="text-neutral-400">Country</dt>
                  <dd className="font-medium text-neutral-200">{data.country}</dd>
                </div>
                <div className="flex justify-between gap-4">
                  <dt className="text-neutral-400">Status</dt>
                  <dd>
                    <span className="rounded-full bg-green-900/40 px-2.5 py-0.5 text-xs font-semibold uppercase tracking-wider text-green-300">
                      {data.status}
                    </span>
                  </dd>
                </div>
              </dl>
            </section>

            <section className="card">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-neutral-400">
                Stripe payouts
              </h2>
              <p className="mt-4 text-sm text-neutral-300">
                Onboarding status:{' '}
                <span className="font-semibold text-white">
                  {data.stripeOnboardingStatus}
                </span>
              </p>
              <p className="mt-2 text-sm text-neutral-400">
                A verified Stripe account is required before guests can pay for
                requests in your sessions (see the payout policy in the
                technical map, Part V).
              </p>
              <a
                href="#stripe"
                className="btn-secondary mt-4 inline-flex"
                aria-disabled="true"
              >
                Start onboarding
              </a>
            </section>
          </div>
        ) : null}
      </div>
    </div>
  );
}
