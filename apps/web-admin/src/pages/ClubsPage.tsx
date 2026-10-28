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
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';
import StatusBadge from '../components/StatusBadge';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import { useLocale } from '../locale';
import { STATUS_MESSAGE_KEY } from '../messages/status';

const STATUS_ACTIONS: ReadonlyArray<{ to: ClubStatus; labelKey: string }> = [
  { to: 'active', labelKey: 'clubs.activate' },
  { to: 'suspended', labelKey: 'clubs.suspend' },
  { to: 'blocked', labelKey: 'clubs.block' },
];

export default function ClubsPage() {
  const { t, locale } = useLocale();
  const count = (value: number) => new Intl.NumberFormat(locale).format(value);
  const fetchClubs = useCallback(() => listClubs(), []);
  const { data, loading, error, reload } = useAsyncData<{ items: Club[] }>(fetchClubs);

  const [form, setForm] = useState<ClubCreateInput>({
    name: '',
    city: '',
    timezone: 'Europe/Madrid',
    address: '',
  });
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [updatingClubId, setUpdatingClubId] = useState<string | null>(null);
  const [createdToken, setCreatedToken] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const onCreate = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSubmitError(null);
    setSubmitting(true);
    try {
      const created = await createClub(form);
      setCreatedToken(created.qrMasterToken);
      setCopied(false);
      setForm({ name: '', city: '', timezone: 'Europe/Madrid', address: '' });
      reload();
    } catch (err) {
      setSubmitError(
        err instanceof ApiRequestError || err instanceof NotImplementedStubError
          ? err.message
          : t('clubs.createFailed'),
      );
    } finally {
      setSubmitting(false);
    }
  };

  const onStatus = async (club: Club, status: ClubStatus) => {
    setActionError(null);
    setActionMessage(null);
    setUpdatingClubId(club.id);
    try {
      await patchClub(club.id, { status });
      setActionMessage(t('clubs.statusUpdated', { name: club.name, status: t(STATUS_MESSAGE_KEY[status] ?? 'status.unknown') }));
      reload();
    } catch (err) {
      setActionError(
        err instanceof ApiRequestError || err instanceof NotImplementedStubError
          ? err.message
          : t('clubs.statusFailed'),
      );
    } finally {
      setUpdatingClubId(null);
    }
  };

  const copyToken = async () => {
    if (!createdToken || !navigator.clipboard) {
      setActionError(t('clubs.clipboardUnavailable'));
      return;
    }
    try {
      await navigator.clipboard.writeText(createdToken);
      setCopied(true);
    } catch {
      setActionError(t('clubs.copyFailed'));
    }
  };

  const formIncomplete =
    form.name === '' || form.city === '' || form.timezone === '';

  return (
    <div>
      <PageHeading
        title={t('clubs.title')}
        action={(
          <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
            <Icon name="lucide:refresh-cw" size={15} />
            {loading ? t('common.refreshing') : t('common.refresh')}
          </button>
        )}
      />

      {error ? <ErrorState message={error.message} code={error.code} onRetry={reload} /> : null}
      {loading && !data ? <Loading label={t('clubs.loading')} /> : null}

      {data ? (
        <section className="panel-card" aria-label={t('clubs.cardLabel')}>
          <header className="panel-card-head">
            <div>
              <h2 className="panel-card-title">{t('clubs.allClubs')}</h2>
              <p className="panel-card-sub">{t('clubs.cardSubtitle')}</p>
            </div>
            <div className="panel-card-tools">
              <span className="chip chip-neutral">{t('clubs.count', { count: count(data.items.length) })}</span>
            </div>
          </header>

          {data.items.length === 0 ? (
            <div className="panel-empty">
              <strong>{t('clubs.emptyTitle')}</strong>
              <span>{t('clubs.emptyBody')}</span>
            </div>
          ) : (
            <div className="table-wrap">
              <table className="data-table" style={{ minWidth: 720 }}>
                <thead>
                  <tr>
                    <th scope="col">{t('clubs.colClub')}</th>
                    <th scope="col">{t('clubs.colCity')}</th>
                    <th scope="col">{t('clubs.colStripe')}</th>
                    <th scope="col">{t('clubs.colStatus')}</th>
                    <th scope="col">{t('clubs.colActions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((club) => (
                    <tr key={club.id}>
                      <td className="is-strong">
                        <span style={{ display: 'block' }}>{club.name}</span>
                        <span className="is-mono" style={{ color: 'var(--c-text-muted-sm)' }}>{club.id.slice(0, 8)}</span>
                      </td>
                      <td>{club.city || '—'}</td>
                      <td><StatusBadge status={club.stripeOnboardingStatus} /></td>
                      <td><StatusBadge status={club.status} /></td>
                      <td>
                        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
                          {STATUS_ACTIONS.filter((a) => a.to !== club.status).map((action) => (
                            <button
                              key={action.to}
                              type="button"
                              className="btn btn-secondary btn-sm"
                              onClick={() => onStatus(club, action.to)}
                              disabled={updatingClubId !== null}
                            >
                              {updatingClubId === club.id ? t('clubs.updating') : t(action.labelKey)}
                            </button>
                          ))}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      ) : null}

      {createdToken ? (
        <section className="panel-card" style={{ marginTop: 16 }} role="status">
          <p style={{ margin: 0, color: 'var(--c-text)', fontSize: 'var(--fs-13)', fontWeight: 500 }}>
            {t('clubs.tokenNote')}
          </p>
          <code
            style={{
              display: 'block',
              marginTop: 8,
              padding: 12,
              borderRadius: 'var(--r-tile)',
              background: 'var(--c-inset)',
              color: 'var(--c-accent-quiet)',
              fontFamily: 'var(--font-mono)',
              fontSize: 'var(--fs-12)',
              overflowWrap: 'anywhere',
            }}
          >
            {createdToken}
          </code>
          <button type="button" className="btn btn-secondary" style={{ marginTop: 12 }} onClick={() => { void copyToken(); }}>
            {copied ? t('clubs.copied') : t('clubs.copyToken')}
          </button>
        </section>
      ) : null}

      {actionError ? <p className="panel-error" role="alert" style={{ marginTop: 16 }}>{actionError}</p> : null}
      {actionMessage ? <p className="admin-toast" role="status">{actionMessage}</p> : null}

      <section className="panel-card" style={{ marginTop: 16 }}>
        <header className="panel-card-head">
          <div>
            <h2 className="panel-card-title">{t('clubs.registerTitle')}</h2>
            <p className="panel-card-sub">{t('clubs.registerSubtitle')}</p>
          </div>
        </header>
        <form onSubmit={onCreate} noValidate>
          <div className="admin-form-row">
            <div>
              <label htmlFor="club-name" className="field-label">{t('clubs.fieldName')}</label>
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
              <label htmlFor="club-city" className="field-label">{t('clubs.fieldCity')}</label>
              <input
                id="club-city"
                type="text"
                required
                placeholder={t('clubs.cityPlaceholder')}
                className="field-input"
                value={form.city}
                onChange={(e) => setForm({ ...form, city: e.target.value })}
              />
            </div>
            <div>
              <label htmlFor="club-timezone" className="field-label">{t('clubs.fieldTimezone')}</label>
              <input
                id="club-timezone"
                type="text"
                required
                className="field-input"
                value={form.timezone}
                onChange={(e) => setForm({ ...form, timezone: e.target.value })}
              />
            </div>
            <div>
              <label htmlFor="club-address" className="field-label">{t('clubs.fieldAddress')}</label>
              <input
                id="club-address"
                type="text"
                className="field-input"
                value={form.address}
                onChange={(e) => setForm({ ...form, address: e.target.value })}
              />
            </div>
          </div>

          {submitError ? <p className="field-error" role="alert" style={{ marginTop: 12 }}>{submitError}</p> : null}

          <div className="admin-form-actions" style={{ marginTop: 16 }}>
            <button type="submit" className="btn btn-primary" disabled={submitting || formIncomplete}>
              {submitting ? t('clubs.creating') : t('clubs.createClub')}
            </button>
            <span className="field-hint">{t('clubs.qrHint')}</span>
          </div>
        </form>
      </section>
    </div>
  );
}
