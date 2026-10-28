import { useEffect, useMemo, useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import QuickActionConsole from '../components/admin-console/QuickActionConsole';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import { useLocale } from '../locale';

interface FlagSpec { key: string; labelKey: string; detailKey: string; default: boolean }

const FLAGS: readonly FlagSpec[] = [
  { key: 'guestRequests', labelKey: 'settings.flagGuestRequests', detailKey: 'settings.flagGuestRequestsDetail', default: true },
  { key: 'djInvites', labelKey: 'settings.flagDjInvites', detailKey: 'settings.flagDjInvitesDetail', default: true },
  { key: 'instantPayouts', labelKey: 'settings.flagInstantPayouts', detailKey: 'settings.flagInstantPayoutsDetail', default: false },
  { key: 'autoModeration', labelKey: 'settings.flagAutoModeration', detailKey: 'settings.flagAutoModerationDetail', default: true },
];

const STORAGE_KEY = 'dropit.admin.settings';

interface SettingsState {
  flags: Record<string, boolean>;
  commissionPercent: string;
  maxPrice: string;
  autoHoldThreshold: string;
}

function loadSettings(): SettingsState {
  const fallback: SettingsState = {
    flags: Object.fromEntries(FLAGS.map((flag) => [flag.key, flag.default])),
    commissionPercent: '12',
    maxPrice: '50',
    autoHoldThreshold: '3',
  };
  if (typeof window === 'undefined') return fallback;
  try {
    const parsed: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? 'null');
    if (!parsed || typeof parsed !== 'object') return fallback;
    const candidate = parsed as Partial<SettingsState>;
    return {
      flags: { ...fallback.flags, ...(candidate.flags ?? {}) },
      commissionPercent: candidate.commissionPercent ?? fallback.commissionPercent,
      maxPrice: candidate.maxPrice ?? fallback.maxPrice,
      autoHoldThreshold: candidate.autoHoldThreshold ?? fallback.autoHoldThreshold,
    };
  } catch {
    return fallback;
  }
}

export default function AdminSettingsPage() {
  const navigate = useNavigate();
  const { t } = useLocale();
  const [settings, setSettings] = useState<SettingsState>(loadSettings);
  const [draft, setDraft] = useState({ commissionPercent: settings.commissionPercent, maxPrice: settings.maxPrice, autoHoldThreshold: settings.autoHoldThreshold });
  const [error, setError] = useState('');
  const [toast, setToast] = useState('');

  useEffect(() => {
    try { window.localStorage.setItem(STORAGE_KEY, JSON.stringify(settings)); } catch { /* Storage may be unavailable. */ }
  }, [settings]);

  useEffect(() => {
    if (!toast) return undefined;
    const timer = window.setTimeout(() => setToast(''), 4000);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const toggleFlag = (key: string, label: string) => {
    setSettings((current) => {
      const next = { ...current, flags: { ...current.flags, [key]: !current.flags[key] } };
      setToast(next.flags[key] ? t('settings.toastEnabled', { label }) : t('settings.toastDisabled', { label }));
      return next;
    });
  };

  const submitEconomics = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const commission = Number(draft.commissionPercent);
    const maxPrice = Number(draft.maxPrice);
    const threshold = Number(draft.autoHoldThreshold);
    if (!Number.isFinite(commission) || commission < 0 || commission > 100) { setError(t('settings.errorCommission')); return; }
    if (!Number.isFinite(maxPrice) || maxPrice <= 0) { setError(t('settings.errorMaxPrice')); return; }
    if (!Number.isInteger(threshold) || threshold < 0 || threshold > 50) { setError(t('settings.errorThreshold')); return; }
    setError('');
    setSettings((current) => ({ ...current, commissionPercent: String(commission), maxPrice: String(maxPrice), autoHoldThreshold: String(threshold) }));
    setToast(t('settings.toastSaved', { commission, maxPrice, threshold }));
  };

  const activeFlags = useMemo(() => FLAGS.filter((flag) => settings.flags[flag.key]).length, [settings.flags]);

  return (
    <div>
      <PageHeading
        title={t('settings.title')}
        action={(
          <>
            <QuickActionConsole onAction={() => navigate('/manage/access')} />
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => { setSettings(loadSettings()); setToast(t('settings.toastReset')); }}
            >
              <Icon name="lucide:rotate-ccw" size={15} />
              {t('settings.resetDraft')}
            </button>
          </>
        )}
      />

      <p className="admin-page-note">
        <strong>{t('management.previewTitle')}</strong> {t('settings.noteLead')} <span className="admin-code">dropit.admin.settings</span> {t('settings.noteMid')}{' '}
        <span className="admin-code">{t('settings.liveRulesName')}</span>.
      </p>

      {toast ? <p className="admin-toast" role="status" aria-live="polite" style={{ marginBottom: 16 }}>{toast}</p> : null}

      <div className="panel-grid-2" style={{ alignItems: 'start' }}>
        <section className="panel-card" aria-labelledby="economics-heading">
          <header className="panel-card-head">
            <div>
              <h2 id="economics-heading" className="panel-card-title">{t('settings.economicsTitle')}</h2>
              <p className="panel-card-sub">{t('settings.economicsSubtitle')}</p>
            </div>
          </header>

          <form onSubmit={submitEconomics} noValidate style={{ display: 'grid', gap: 14 }}>
            <label className="admin-field">
              <span>{t('settings.commissionRate')}</span>
              <input
                id="commission-percent"
                aria-label={t('settings.commissionRateLabel')}
                name="commissionPercent"
                type="number"
                min="0"
                max="100"
                step="0.1"
                inputMode="decimal"
                autoComplete="off"
                className="field-input"
                value={draft.commissionPercent}
                onChange={(event) => setDraft({ ...draft, commissionPercent: event.target.value })}
              />
              <small>{t('settings.commissionExample')}</small>
            </label>
            <label className="admin-field">
              <span>{t('settings.maxPrice')}</span>
              <input
                id="max-price"
                aria-label={t('settings.maxPriceLabel')}
                name="maxPrice"
                type="number"
                min="1"
                max="1000"
                step="1"
                inputMode="decimal"
                autoComplete="off"
                className="field-input"
                value={draft.maxPrice}
                onChange={(event) => setDraft({ ...draft, maxPrice: event.target.value })}
              />
              <small>{t('settings.maxPriceHint')}</small>
            </label>
            <label className="admin-field">
              <span>{t('settings.autoHold')}</span>
              <input
                id="auto-hold-threshold"
                aria-label={t('settings.autoHoldLabel')}
                name="autoHoldThreshold"
                type="number"
                min="0"
                max="50"
                step="1"
                inputMode="numeric"
                autoComplete="off"
                className="field-input"
                value={draft.autoHoldThreshold}
                onChange={(event) => setDraft({ ...draft, autoHoldThreshold: event.target.value })}
              />
              <small>{t('settings.autoHoldHint')}</small>
            </label>

            {error ? <p className="field-error" role="alert" style={{ margin: 0 }}>{error}</p> : null}

            <div className="admin-form-actions">
              <button type="submit" className="btn btn-primary">{t('settings.saveRules')}</button>
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => setDraft({ commissionPercent: settings.commissionPercent, maxPrice: settings.maxPrice, autoHoldThreshold: settings.autoHoldThreshold })}
              >
                {t('settings.discard')}
              </button>
            </div>
          </form>
        </section>

        <section className="panel-card" aria-labelledby="flags-heading">
          <header className="panel-card-head">
            <div>
              <h2 id="flags-heading" className="panel-card-title">{t('settings.flagsTitle')}</h2>
              <p className="panel-card-sub">{t('settings.flagsSubtitle', { active: activeFlags, total: FLAGS.length })}</p>
            </div>
          </header>

          <ul className="admin-flag-list">
            {FLAGS.map((flag) => {
              const on = Boolean(settings.flags[flag.key]);
              return (
                <li key={flag.key}>
                  <div style={{ minWidth: 0 }}>
                    <strong>{t(flag.labelKey)}</strong>
                    <small>{t(flag.detailKey)}</small>
                  </div>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={on}
                    aria-label={t(flag.labelKey)}
                    className={`admin-toggle ${on ? 'is-on' : ''}`.trim()}
                    onClick={() => toggleFlag(flag.key, t(flag.labelKey))}
                  >
                    <span />
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      </div>

      <section className="panel-card panel-section" aria-label={t('settings.previewScope')}>
        <header className="panel-card-head">
          <div>
            <h2 className="panel-card-title">{t('settings.previewScope')}</h2>
            <p className="panel-card-sub">{t('settings.previewScopeSubtitle')}</p>
          </div>
        </header>
        <p style={{ margin: 0, color: 'var(--c-text-2)', fontSize: 'var(--fs-13)', lineHeight: 1.55, maxWidth: '92ch' }}>
          {t('settings.scopeLead')} <span className="admin-code">dropit.admin.settings</span> {t('settings.scopeMid')}{' '}
          <span className="admin-code">getPlatformSettings</span> {t('settings.scopeAnd')} <span className="admin-code">patchPlatformSettings</span>{' '}
          {t('settings.scopeFrom')} <span className="admin-code">api/client</span> {t('settings.scopeTail')}
        </p>
      </section>
    </div>
  );
}
