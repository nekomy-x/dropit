import { useCallback } from 'react';
import { getPlatformSettings, type PlatformSettings } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';

/**
 * Platform settings — "backend not implemented yet": GET/PATCH
 * /v1/admin/platform-settings have no backend routes yet, so the client
 * methods reject locally with a 501 stub and the page renders a
 * waiting-for-backend empty state.
 *
 * Once the backend lands, the full editing form (commission mode, price
 * limits, DJ share — validated client-side: bp integers 0..10000, money
 * integers in cents, min price < max price per ADR-003) is restored.
 */
export default function PlatformSettingsPage() {
  const fetchSettings = useCallback(() => getPlatformSettings(), []);
  const { data, loading, error } = useAsyncData<PlatformSettings>(fetchSettings);

  void data;

  return (
    <div>
      <h1 className="page-title">Platform settings</h1>
      <p className="page-subtitle">
        Commission, price limits and DJ revenue share. Changes apply to new
        requests only; every payment keeps an immutable snapshot of the rules
        it was charged under.
      </p>

      <div className="mt-6">
        {loading ? (
          <p className="text-sm text-neutral-400">Loading platform settings…</p>
        ) : null}

        {error ? (
          error.notImplemented ? (
            <p
              className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-6 text-sm text-neutral-400"
              role="status"
            >
              Platform settings are waiting for the backend —{' '}
              <code>GET/PATCH /v1/admin/platform-settings</code> are not
              implemented yet. Settings keep the defaults from the seeded{' '}
              <code>platform_settings</code> row (30% commission, 1–10 EUR
              price band, 50/50 club/DJ split of the remainder).
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
