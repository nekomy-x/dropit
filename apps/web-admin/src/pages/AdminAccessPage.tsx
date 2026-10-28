import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import RolePermissionMatrix, { type ConsolePermission, type ConsoleRole } from '../components/admin-console/RolePermissionMatrix';
import QuickActionConsole from '../components/admin-console/QuickActionConsole';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import { useLocale } from '../locale';

/** Role names are shared with the matrix: both name the same four roles. */
const ROLE_LABEL_KEY: Record<ConsoleRole, string> = {
  'Platform admin': 'roleMatrix.rolePlatformAdmin',
  'Club manager': 'roleMatrix.roleClubManager',
  DJ: 'roleMatrix.roleDj',
  Support: 'roleMatrix.roleSupport',
};

const ROLE_SUMMARY: ReadonlyArray<{ role: ConsoleRole; purposeKey: string; scopeKey: string }> = [
  { role: 'Platform admin', purposeKey: 'access.purposePlatformAdmin', scopeKey: 'access.scopePlatformAdmin' },
  { role: 'Club manager', purposeKey: 'access.purposeClubManager', scopeKey: 'access.scopeClubManager' },
  { role: 'DJ', purposeKey: 'access.purposeDj', scopeKey: 'access.scopeDj' },
  { role: 'Support', purposeKey: 'access.purposeSupport', scopeKey: 'access.scopeSupport' },
];

const GUARDRAILS: ReadonlyArray<{ titleKey: string; detailKey: string }> = [
  { titleKey: 'access.guardLockedTitle', detailKey: 'access.guardLockedDetail' },
  { titleKey: 'access.guardLocalTitle', detailKey: 'access.guardLocalDetail' },
  { titleKey: 'access.guardGrantFirstTitle', detailKey: 'access.guardGrantFirstDetail' },
];

export default function AdminAccessPage() {
  const navigate = useNavigate();
  const { t } = useLocale();
  const [toast, setToast] = useState('');

  return (
    <div>
      <PageHeading
        title={t('access.title')}
        action={(
          <>
            <QuickActionConsole onAction={() => navigate('/manage/activity')} />
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => setToast(t('access.toast'))}
            >
              <Icon name="lucide:info" size={15} />
              {t('access.howChangesSave')}
            </button>
          </>
        )}
      />

      {toast ? <p className="admin-toast" role="status" aria-live="polite" style={{ marginBottom: 16 }}>{toast}</p> : null}

      <section className="panel-card" aria-label={t('access.roleOverviewLabel')}>
        <header className="panel-card-head">
          <div>
            <h2 className="panel-card-title">{t('access.rolesHeading')}</h2>
            <p className="panel-card-sub">{t('access.rolesSubtitle')}</p>
          </div>
        </header>
        <ul className="list" style={{ margin: 0, padding: 0, listStyle: 'none' }}>
          {ROLE_SUMMARY.map(({ role, purposeKey, scopeKey }) => (
            <li key={role} className="list-row" style={{ alignItems: 'flex-start' }}>
              <span className="list-main">
                <span className="list-title">{t(ROLE_LABEL_KEY[role])}</span>
                <span className="list-sub">{t(purposeKey)}</span>
              </span>
              <span className="list-sub" style={{ maxWidth: '48ch', margin: 0, textAlign: 'right' }}>{t(scopeKey)}</span>
            </li>
          ))}
        </ul>
      </section>

      <div className="panel-section">
        <RolePermissionMatrix />
      </div>

      <section className="panel-card panel-section" aria-label={t('access.guardrailsLabel')}>
        <header className="panel-card-head">
          <div>
            <h2 className="panel-card-title">{t('access.guardrailsHeading')}</h2>
            <p className="panel-card-sub">{t('access.guardrailsSubtitle')}</p>
          </div>
        </header>
        <ul className="list" style={{ margin: 0, padding: 0, listStyle: 'none' }}>
          {GUARDRAILS.map(({ titleKey, detailKey }) => (
            <li key={titleKey} className="list-row" style={{ alignItems: 'flex-start' }}>
              <span className="list-main">
                <span className="list-title">{t(titleKey)}</span>
                <span className="list-sub">{t(detailKey)}</span>
              </span>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}

export type { ConsolePermission, ConsoleRole };
