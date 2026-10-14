import { useCallback } from 'react';
import { getProfile, type ClubProfile } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';

/**
 * Dashboard — "backend not implemented yet": GET /v1/club/profile has no
 * backend route yet, so the client method rejects locally with a 501 stub
 * and the page renders a waiting-for-backend empty state. The live club
 * surface is the Sessions page (F2).
 */
export default function DashboardPage() {
  const fetchProfile = useCallback(() => getProfile(), []);
  const { data, loading, error } = useAsyncData<ClubProfile>(fetchProfile);

  void data;

  return (
    <div>
      <h1 className="page-title">Dashboard</h1>
      <p className="page-subtitle">
        Club overview. Live session management is on the Sessions page.
      </p>

      <div className="mt-6">
        {loading ? (
          <p className="text-sm text-neutral-400">Loading club profile…</p>
        ) : null}

        {error ? (
          error.notImplemented ? (
            <p
              className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
              role="status"
            >
              Club profile is waiting for the backend —{' '}
              <code>GET /v1/club/profile</code> is not implemented yet. Head to{' '}
              <a href="/sessions" className="text-brand-500 hover:text-brand-600">
                Sessions
              </a>{' '}
              for the live shift flow (open session, QR, close).
            </p>
          ) : (
            <p className="text-sm text-red-300" role="alert">
              {error.message}
              {error.code ? ` (${error.code})` : ''}
            </p>
          )
        ) : null}
      </div>
    </div>
  );
}
