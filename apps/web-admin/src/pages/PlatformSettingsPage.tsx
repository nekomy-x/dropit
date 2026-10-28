import { useCallback, useEffect, useState, type FormEvent } from 'react';
import { ApiRequestError, getPlatformSettings, patchPlatformSettings, type PlatformSettings, type RuntimeMode } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import { useLocale } from '../locale';

/** The four `runtime_mode` values, in the order the control offers them. */
const RUNTIME_MODES: ReadonlyArray<{ value: RuntimeMode; labelKey: string }> = [
  { value: 'auto', labelKey: 'platform.deploymentAuto' },
  { value: 'test', labelKey: 'platform.deploymentTest' },
  { value: 'maintenance', labelKey: 'platform.deploymentMaintenance' },
  { value: 'normal', labelKey: 'platform.deploymentNormal' },
];

export default function PlatformSettingsPage() {
  const { t } = useLocale();
  const fetchSettings = useCallback(() => getPlatformSettings(), []);
  const { data, loading, error, reload } = useAsyncData<PlatformSettings>(fetchSettings);
  const [form, setForm] = useState({ mode: 'percent' as 'percent' | 'fixed', commissionPercent: '', commissionFixed: '', minPrice: '', maxPrice: '', djShare: '' });
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  // Deployment mode is its own form: its own draft, save button and status, so
  // saving it never touches the economics draft and vice versa.
  const [runtimeMode, setRuntimeMode] = useState<RuntimeMode>('auto');
  const [savedRuntimeMode, setSavedRuntimeMode] = useState<RuntimeMode>('auto');
  const [modeSaving, setModeSaving] = useState(false);
  const [modeMessage, setModeMessage] = useState<string | null>(null);
  const [modeError, setModeError] = useState<string | null>(null);
  const modeDirty = runtimeMode !== savedRuntimeMode;

  useEffect(() => {
    if (!data) return;
    setForm({ mode: data.commissionMode === 'fixed' ? 'fixed' : 'percent', commissionPercent: String(data.commissionPercentBp / 100), commissionFixed: String(data.commissionFixedMinor / 100), minPrice: String(data.minPriceMinor / 100), maxPrice: String(data.maxPriceMinor / 100), djShare: String(data.djShareOfRemainderBp / 100) });
    setRuntimeMode(data.runtimeMode);
    setSavedRuntimeMode(data.runtimeMode);
  }, [data]);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault(); setSaveError(null); setMessage(null);
    const min = Math.round(Number(form.minPrice) * 100); const max = Math.round(Number(form.maxPrice) * 100);
    const shareBp = Math.round(Number(form.djShare) * 100); const commissionBp = Math.round(Number(form.commissionPercent) * 100);
    if (!(min < max) || min < 0 || max < 0 || shareBp < 0 || shareBp > 10000 || commissionBp < 0 || commissionBp > 10000) { setSaveError(t('platform.errorValidation')); return; }
    setSaving(true);
    try {
      await patchPlatformSettings({ commissionMode: form.mode, commissionPercentBp: commissionBp, commissionFixedMinor: Math.round(Number(form.commissionFixed) * 100), minPriceMinor: min, maxPriceMinor: max, djShareOfRemainderBp: shareBp });
      setMessage(t('platform.updated')); reload();
    } catch (err) { setSaveError(err instanceof ApiRequestError ? err.message : t('platform.saveFailed')); }
    finally { setSaving(false); }
  };

  const saveRuntimeMode = async () => {
    setModeError(null); setModeMessage(null); setModeSaving(true);
    try {
      const next = await patchPlatformSettings({ runtimeMode });
      setRuntimeMode(next.runtimeMode);
      setSavedRuntimeMode(next.runtimeMode);
      setModeMessage(t('platform.deploymentSaved'));
    } catch (err) {
      setModeError(err instanceof ApiRequestError ? err.message : t('platform.deploymentSaveFailed'));
    } finally { setModeSaving(false); }
  };

  return (
    <div>
      <PageHeading
        title={t('platform.title')}
        action={(
          <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
            <Icon name="lucide:refresh-cw" size={15} />
            {loading ? t('common.refreshing') : t('common.refresh')}
          </button>
        )}
      />

      {error ? <ErrorState message={error.message} code={error.code} onRetry={reload} /> : null}
      {loading && !data ? <Loading label={t('platform.loading')} /> : null}

      {data ? (
        <>
          <form onSubmit={submit}>
            <div className="panel-grid-2" style={{ alignItems: 'start' }}>
              <section className="panel-card" aria-labelledby="commission-heading">
                <header className="panel-card-head">
                  <div>
                    <h2 id="commission-heading" className="panel-card-title">{t('platform.commission')}</h2>
                    <p className="panel-card-sub">{t('platform.commissionSubtitle')}</p>
                  </div>
                </header>

                <div className="seg" role="group" aria-label={t('platform.commissionMode')}>
                  <button
                    type="button"
                    aria-pressed={form.mode === 'percent'}
                    onClick={() => setForm({ ...form, mode: 'percent' })}
                    className={`seg-item ${form.mode === 'percent' ? 'is-active' : ''}`.trim()}
                  >
                    {t('platform.percentage')}
                  </button>
                  <button
                    type="button"
                    aria-pressed={form.mode === 'fixed'}
                    onClick={() => setForm({ ...form, mode: 'fixed' })}
                    className={`seg-item ${form.mode === 'fixed' ? 'is-active' : ''}`.trim()}
                  >
                    {t('platform.fixedFee')}
                  </button>
                </div>

                <div style={{ marginTop: 16 }}>
                  {form.mode === 'percent' ? (
                    <>
                      <label className="field-label" htmlFor="commission-percent">{t('platform.commissionPercent')}</label>
                      <input
                        id="commission-percent"
                        type="number"
                        min="0"
                        max="100"
                        step="0.01"
                        className="field-input"
                        value={form.commissionPercent}
                        onChange={(e) => setForm({ ...form, commissionPercent: e.target.value })}
                      />
                    </>
                  ) : (
                    <>
                      <label className="field-label" htmlFor="commission-fixed">{t('platform.fixedFeeEur')}</label>
                      <input
                        id="commission-fixed"
                        type="number"
                        min="0"
                        step="0.01"
                        className="field-input"
                        value={form.commissionFixed}
                        onChange={(e) => setForm({ ...form, commissionFixed: e.target.value })}
                      />
                    </>
                  )}
                </div>
              </section>

              <section className="panel-card" aria-labelledby="pricing-heading">
                <header className="panel-card-head">
                  <div>
                    <h2 id="pricing-heading" className="panel-card-title">{t('platform.requestPricing')}</h2>
                    <p className="panel-card-sub">{t('platform.requestPricingSubtitle')}</p>
                  </div>
                </header>

                <div className="panel-grid-2">
                  <div>
                    <label className="field-label" htmlFor="min-price">{t('platform.minimum')}</label>
                    <input
                      id="min-price"
                      type="number"
                      min="0"
                      step="0.01"
                      className="field-input"
                      value={form.minPrice}
                      onChange={(e) => setForm({ ...form, minPrice: e.target.value })}
                    />
                  </div>
                  <div>
                    <label className="field-label" htmlFor="max-price">{t('platform.maximum')}</label>
                    <input
                      id="max-price"
                      type="number"
                      min="0"
                      step="0.01"
                      className="field-input"
                      value={form.maxPrice}
                      onChange={(e) => setForm({ ...form, maxPrice: e.target.value })}
                    />
                  </div>
                </div>

                <div style={{ marginTop: 16 }}>
                  <label className="field-label" htmlFor="dj-share">{t('platform.djShare')}</label>
                  <input
                    id="dj-share"
                    type="number"
                    min="0"
                    max="100"
                    step="0.01"
                    className="field-input"
                    value={form.djShare}
                    onChange={(e) => setForm({ ...form, djShare: e.target.value })}
                  />
                </div>
              </section>
            </div>

            {saveError ? <p className="panel-error" role="alert" style={{ marginTop: 16 }}>{saveError}</p> : null}
            {message ? <p className="admin-toast" role="status">{message}</p> : null}

            <div className="admin-form-actions" style={{ marginTop: 16, justifyContent: 'flex-end' }}>
              <button type="submit" className="btn btn-primary" disabled={saving}>{saving ? t('common.saving') : t('platform.save')}</button>
            </div>
          </form>

          <section className="panel-card panel-section" aria-labelledby="deployment-heading">
            <header className="panel-card-head">
              <div>
                <h2 id="deployment-heading" className="panel-card-title">{t('platform.deploymentTitle')}</h2>
                <p className="panel-card-sub">{t('platform.deploymentSubtitle')}</p>
              </div>
            </header>

            <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 12 }}>
              <div className="seg" role="group" aria-label={t('platform.deploymentTitle')} style={{ flexWrap: 'wrap' }}>
                {RUNTIME_MODES.map((option) => (
                  <button
                    key={option.value}
                    type="button"
                    aria-pressed={runtimeMode === option.value}
                    onClick={() => { setRuntimeMode(option.value); setModeMessage(null); setModeError(null); }}
                    className={`seg-item ${runtimeMode === option.value ? 'is-active' : ''}`.trim()}
                  >
                    {t(option.labelKey)}
                  </button>
                ))}
              </div>
              <p className="panel-card-sub" style={{ margin: 0, flex: '1 1 300px', maxWidth: '78ch' }}>
                {t('platform.deploymentAutoHint')}
              </p>
            </div>

            <div className="admin-form-actions" style={{ marginTop: 14, justifyContent: 'flex-end' }}>
              {modeError ? <p className="panel-error" role="alert" style={{ margin: 0 }}>{modeError}</p> : null}
              {modeMessage ? <p className="admin-toast" role="status" style={{ margin: 0 }}>{modeMessage}</p> : null}
              <button
                type="button"
                className="btn btn-primary"
                onClick={saveRuntimeMode}
                disabled={modeSaving || !modeDirty}
              >
                {modeSaving ? t('common.saving') : t('platform.deploymentSave')}
              </button>
            </div>
          </section>
        </>
      ) : null}
    </div>
  );
}
