import { useCallback, useState } from 'react';
import { listClubs, listGuests, type AdminGuest, type Club } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';
import StatusBadge from '../components/StatusBadge';
import PageHeading from '../components/PageHeading';
import Select from '../components/ui/Select';
import { useLocale } from '../locale';

/**
 * Guest devices are scoped to a club by the backend contract.
 */
export default function GuestsPage() {
  const { t, locale } = useLocale();
  const [clubId, setClubId] = useState('');
  const fetchClubs = useCallback(() => listClubs(), []);
  const clubs = useAsyncData<{ items: Club[] }>(fetchClubs);
  const fetchGuests = useCallback(() => clubId ? listGuests(clubId) : Promise.resolve({ items: [] }), [clubId]);
  const { data, loading, error, reload } = useAsyncData<{ items: AdminGuest[] }>(fetchGuests);

  return (
    <div>
      <PageHeading
        title={t('guests.title')}
      />

      <section className="panel-card" aria-label={t('guests.cardLabel')}>
        <header className="panel-card-head">
          <div>
            <h2 className="panel-card-title">{t('guests.cardTitle')}</h2>
            <p className="panel-card-sub">{t('guests.cardSubtitle')}</p>
          </div>
        </header>

        <div style={{ maxWidth: 420 }}>
          <Select
            id="guest-club"
            label={t('guests.clubLabel')}
            value={clubId}
            onChange={setClubId}
            placeholder={clubs.loading ? t('guests.loadingClubs') : t('guests.selectClub')}
            options={[
              { value: '', label: t('guests.allClubs') },
              ...(clubs.data?.items ?? []).map((club) => ({ value: club.id, label: club.name })),
            ]}
          />
          {clubs.loading ? <p className="field-hint" style={{ marginTop: 6 }}>{t('guests.loadingClubList')}</p> : null}
        </div>

        {clubs.error ? <div style={{ marginTop: 16 }}><ErrorState message={clubs.error.message} code={clubs.error.code} onRetry={clubs.reload} /></div> : null}

        <div style={{ marginTop: 16 }}>
          {!clubId && !clubs.loading && !clubs.error ? (
            <div className="panel-empty">
              <strong>{t('guests.noClubTitle')}</strong>
              <span>{t('guests.noClubBody')}</span>
            </div>
          ) : null}

          {loading && clubId ? <Loading label={t('guests.loadingDevices')} /> : null}
          {error && clubId ? <ErrorState message={error.message} code={error.code} onRetry={reload} /> : null}

          {data && clubId && data.items.length > 0 ? (
            <div className="table-wrap">
              <table className="data-table" style={{ minWidth: 820 }}>
                <thead>
                  <tr>
                    <th scope="col">{t('guests.colDevice')}</th>
                    <th scope="col">{t('guests.colGuest')}</th>
                    <th scope="col" className="is-num">{t('guests.colRequests')}</th>
                    <th scope="col">{t('guests.colFirstSeen')}</th>
                    <th scope="col">{t('guests.colLastSeen')}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((guest) => (
                    <tr key={guest.id}>
                      <td className="is-mono" style={{ color: 'var(--c-text-muted)' }}>{guest.id}</td>
                      <td>{guest.displayName || guest.email || t('guests.guestFallback')}</td>
                      <td className="is-num">{guest.requestsCount}</td>
                      <td>{new Date(guest.createdAt).toLocaleString(locale)}</td>
                      <td>
                        {new Date(guest.lastSeenAt).toLocaleString(locale)}
                        {guest.banned ? <span style={{ marginLeft: 8 }}><StatusBadge status="blocked" /></span> : null}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}

          {data && clubId && data.items.length === 0 && !loading ? (
            <div className="panel-empty" role="status">
              <strong>{t('guests.emptyTitle')}</strong>
              <span>{t('guests.emptyBody')}</span>
            </div>
          ) : null}
        </div>
      </section>
    </div>
  );
}
