import { useEffect, useState } from 'react';
import { useLocale } from '../../locale';

export type ConsoleRole = 'Platform admin' | 'Club manager' | 'DJ' | 'Support';
export type ConsolePermission = 'Manage users' | 'Manage venues' | 'Issue refunds' | 'Edit queue' | 'View audit log';
const ROLES: ConsoleRole[] = ['Platform admin', 'Club manager', 'DJ', 'Support'];
const PERMISSIONS: ConsolePermission[] = ['Manage users', 'Manage venues', 'Issue refunds', 'Edit queue', 'View audit log'];
const INITIAL: Record<ConsoleRole, ConsolePermission[]> = {
  'Platform admin': [...PERMISSIONS],
  'Club manager': ['Manage venues', 'Issue refunds', 'Edit queue'],
  DJ: ['Edit queue'],
  Support: ['Manage users', 'View audit log'],
};
const STORAGE_KEY = 'dropit.admin-console.role-permissions';

/** Stored role name → catalog key. The stored name is the storage key, so it stays English. */
const ROLE_KEY: Record<ConsoleRole, string> = {
  'Platform admin': 'roleMatrix.rolePlatformAdmin',
  'Club manager': 'roleMatrix.roleClubManager',
  DJ: 'roleMatrix.roleDj',
  Support: 'roleMatrix.roleSupport',
};

/** Stored permission name → catalog key. */
const PERMISSION_KEY: Record<ConsolePermission, string> = {
  'Manage users': 'roleMatrix.permManageUsers',
  'Manage venues': 'roleMatrix.permManageVenues',
  'Issue refunds': 'roleMatrix.permIssueRefunds',
  'Edit queue': 'roleMatrix.permEditQueue',
  'View audit log': 'roleMatrix.permViewAuditLog',
};

function isRole(value: string): value is ConsoleRole {
  return ROLES.includes(value as ConsoleRole);
}
function isPermission(value: string): value is ConsolePermission {
  return PERMISSIONS.includes(value as ConsolePermission);
}
function loadPermissions(): Record<ConsoleRole, ConsolePermission[]> {
  if (typeof window === 'undefined') return { ...INITIAL };
  try {
    const parsed: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? 'null');
    if (!parsed || typeof parsed !== 'object') return { ...INITIAL };
    const candidate = parsed as Record<string, unknown>;
    return ROLES.reduce<Record<ConsoleRole, ConsolePermission[]>>((result, role) => {
      const values = candidate[role];
      result[role] = Array.isArray(values) ? values.filter((value): value is ConsolePermission => typeof value === 'string' && isPermission(value)) : [...INITIAL[role]];
      return result;
    }, { ...INITIAL });
  } catch {
    return { ...INITIAL };
  }
}

/**
 * Role × permission matrix.
 *
 * A `data-table` inside a `panel-card`: the permission is the row head, each
 * role a column, and every cell states its meaning in words ("Allowed" /
 * "No access") next to the checkbox, so the grid is readable without colour and
 * with a screen reader.
 */
export default function RolePermissionMatrix() {
  const { t } = useLocale();
  const [permissions, setPermissions] = useState<Record<ConsoleRole, ConsolePermission[]>>(loadPermissions);
  const [message, setMessage] = useState('');

  useEffect(() => {
    try { window.localStorage.setItem(STORAGE_KEY, JSON.stringify(permissions)); } catch { /* Storage may be unavailable in private browsing. */ }
  }, [permissions]);

  const toggle = (role: ConsoleRole, permission: ConsolePermission) => {
    if (role === 'Platform admin' || !isRole(role)) return;
    setPermissions((current) => {
      const enabled = current[role].includes(permission);
      setMessage(enabled
        ? t('roleMatrix.revoked', { permission: t(PERMISSION_KEY[permission]), role: t(ROLE_KEY[role]) })
        : t('roleMatrix.granted', { permission: t(PERMISSION_KEY[permission]), role: t(ROLE_KEY[role]) }));
      return { ...current, [role]: enabled ? current[role].filter((item) => item !== permission) : [...current[role], permission] };
    });
  };

  return (
    <section aria-labelledby="permission-matrix-heading" className="panel-card">
      <header className="panel-card-head">
        <div>
          <h2 id="permission-matrix-heading" className="panel-card-title">{t('roleMatrix.title')}</h2>
          <p className="panel-card-sub">{t('roleMatrix.subtitle')}</p>
        </div>
        <p role="status" style={{ margin: 0, color: 'var(--c-accent-quiet)', fontSize: 'var(--fs-11)' }}>{message}</p>
      </header>

      <div className="admin-matrix-scroll table-wrap">
        <table className="data-table" style={{ minWidth: 680 }}>
          <thead>
            <tr>
              <th scope="col">{t('roleMatrix.permissionColumn')}</th>
              {ROLES.map((role) => <th scope="col" key={role}>{t(ROLE_KEY[role])}</th>)}
            </tr>
          </thead>
          <tbody>
            {PERMISSIONS.map((permission) => (
              <tr key={permission}>
                <th scope="row" className="is-strong" style={{ paddingLeft: 0, fontWeight: 500, textAlign: 'left' }}>{t(PERMISSION_KEY[permission])}</th>
                {ROLES.map((role) => {
                  const checked = permissions[role].includes(permission);
                  return (
                    <td key={role}>
                      <label style={{ display: 'inline-flex', minHeight: 24, alignItems: 'center', gap: 8, cursor: role === 'Platform admin' ? 'not-allowed' : 'pointer' }}>
                        <input
                          aria-label={t('roleMatrix.cellLabel', { permission: t(PERMISSION_KEY[permission]), role: t(ROLE_KEY[role]) })}
                          type="checkbox"
                          checked={checked}
                          disabled={role === 'Platform admin'}
                          onChange={() => toggle(role, permission)}
                        />
                        <span className="admin-matrix-meaning">{checked ? t('roleMatrix.allowed') : t('roleMatrix.noAccess')}</span>
                      </label>
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
