import { useEffect, useRef, useState } from 'react';
import Icon from '../Icon';
import StatusBadge from '../StatusBadge';
import { useLocale } from '../../locale';

export type ConsoleEntityKind = 'User' | 'DJ' | 'Club';
export type ConsoleEntityStatus = 'Active' | 'Under review' | 'Suspended';
export interface ConsoleEntity {
  id: string;
  kind: ConsoleEntityKind;
  name: string;
  email: string;
  role?: string;
  status: ConsoleEntityStatus;
  since: string;
  details: ReadonlyArray<{ label: string; value: string }>;
}
export type ConsoleEntityChanges = { name: string; email: string; role: string };
type EntityField = keyof ConsoleEntityChanges;
type EntityFieldErrors = Partial<Record<EntityField, string>>;
export interface EntityDetailDrawerProps {
  entity?: ConsoleEntity;
  onClose?: () => void;
  onStatusChange?: (status: ConsoleEntityStatus) => void;
  onSave?: (changes: ConsoleEntityChanges) => void;
}

const EMPTY_ENTITY: ConsoleEntity = {
  id: 'usr_84f2a19c', kind: 'User', name: 'Maya Chen', email: 'maya.chen@example.com', role: 'Guest',
  status: 'Active', since: '2025-04-16', details: [],
};

/** The drawer's own status word → catalog key. The word itself is stored logic. */
const ENTITY_STATUS_KEY: Record<ConsoleEntityStatus, string> = {
  Active: 'status.active',
  'Under review': 'status.underReview',
  Suspended: 'status.suspended',
};

const ENTITY_KIND_KEY: Record<ConsoleEntity['kind'], string> = {
  User: 'drawer.kindUser',
  DJ: 'drawer.kindDj',
  Club: 'drawer.kindClub',
};
const FOCUSABLE_SELECTOR = 'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

/** Map the drawer's three status words onto the shared status chips. */
const STATUS_CHIP: Record<ConsoleEntityStatus, string> = {
  Active: 'active',
  'Under review': 'pending',
  Suspended: 'suspended',
};

/**
 * Record detail drawer.
 *
 * Presentation is the shared panel language: a `--c-surface` drawer, hairlines
 * between label/value pairs, `field-*` inputs and `btn-*` actions. Behaviour is
 * unchanged: it is a modal dialog with a focus trap, Escape closes it, and focus
 * returns to the control that opened it.
 */
export default function EntityDetailDrawer({ entity, onClose, onStatusChange, onSave }: EntityDetailDrawerProps) {
  const { t } = useLocale();
  const resolved: ConsoleEntity = entity ?? {
    ...EMPTY_ENTITY,
    role: t('management.roleGuest'),
    details: [
      { label: t('drawer.detailRequests'), value: '28' },
      { label: t('drawer.detailLastActive'), value: t('drawer.defaultLastActive') },
      { label: t('drawer.detailClub'), value: 'Velvet Room' },
    ],
  };
  const current = entity ?? resolved;
  const [status, setStatus] = useState(current.status);
  const [name, setName] = useState(current.name);
  const [email, setEmail] = useState(current.email);
  const [role, setRole] = useState(current.role ?? '');
  const [errors, setErrors] = useState<EntityFieldErrors>({});
  const [notice, setNotice] = useState('');
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const previouslyFocusedRef = useRef<HTMLElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    setStatus(current.status);
    setName(current.name);
    setEmail(current.email);
    setRole(current.role ?? '');
    setErrors({});
    setNotice('');
  }, [current]);

  useEffect(() => {
    previouslyFocusedRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    requestAnimationFrame(() => closeButtonRef.current?.focus());
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onCloseRef.current?.();
        return;
      }
      if (event.key !== 'Tab' || !dialogRef.current) return;
      const focusable = Array.from(dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const activeElement = document.activeElement;
      if (!dialogRef.current.contains(activeElement)) {
        event.preventDefault();
        (event.shiftKey ? last : first)?.focus();
      } else if (event.shiftKey && activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      if (previouslyFocusedRef.current?.isConnected) previouslyFocusedRef.current.focus();
    };
  }, []);

  const updateStatus = (nextStatus: ConsoleEntityStatus) => {
    setStatus(nextStatus);
    setNotice(t('drawer.statusChanged', { kind: t(ENTITY_KIND_KEY[current.kind]), status: t(ENTITY_STATUS_KEY[nextStatus]) }));
    onStatusChange?.(nextStatus);
  };
  const saveChanges = () => {
    const nextErrors: EntityFieldErrors = {};
    const nextName = name.trim();
    const nextEmail = email.trim();
    const nextRole = role.trim();
    if (!nextName) nextErrors.name = t('drawer.errorNameRequired');
    if (!nextEmail) nextErrors.email = t('drawer.errorEmailRequired');
    else if (!/^\S+@\S+\.\S+$/.test(nextEmail)) nextErrors.email = t('drawer.errorEmailInvalid');
    if (!nextRole) nextErrors.role = t('drawer.errorRoleRequired');
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) {
      setNotice(t('drawer.reviewFields'));
      return;
    }
    onSave?.({ name: nextName, email: nextEmail, role: nextRole });
    setNotice(t('drawer.detailsSaved'));
  };

  return (
    <div
      className="admin-drawer-backdrop"
      role="presentation"
      onMouseDown={(event) => { if (event.target === event.currentTarget) onClose?.(); }}
    >
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="entity-drawer-heading" className="admin-drawer">
        <header className="admin-drawer-head">
          <div style={{ minWidth: 0 }}>
            <p className="admin-eyebrow">{t('drawer.eyebrow', { kind: t(ENTITY_KIND_KEY[current.kind]) })}</p>
            <h2 id="entity-drawer-heading" className="panel-card-title" style={{ fontSize: 'var(--fs-20)' }}>{name || current.name}</h2>
          </div>
          <button ref={closeButtonRef} type="button" aria-label={t('drawer.close')} onClick={onClose} className="panel-icon-button">
            <Icon name="lucide:x" size={16} />
          </button>
        </header>

        <div className="admin-drawer-body">
          <p role="status" style={{ minHeight: 20, margin: 0, color: 'var(--c-accent-quiet)', fontSize: 'var(--fs-12)' }}>{notice}</p>

          <form style={{ display: 'grid', gap: 14, marginTop: 12 }} onSubmit={(event) => { event.preventDefault(); saveChanges(); }}>
            <div>
              <label htmlFor="entity-name" className="field-label">{t('drawer.name')}</label>
              <input
                id="entity-name"
                className="field-input"
                value={name}
                aria-invalid={Boolean(errors.name)}
                aria-describedby={errors.name ? 'entity-name-error' : undefined}
                onChange={(event) => { setName(event.target.value); if (errors.name) setErrors((current) => ({ ...current, name: undefined })); }}
              />
              {errors.name && <p id="entity-name-error" className="field-error" style={{ margin: '4px 0 0' }}>{errors.name}</p>}
            </div>
            <div>
              <label htmlFor="entity-email" className="field-label">{t('drawer.email')}</label>
              <input
                id="entity-email"
                type="email"
                className="field-input"
                value={email}
                aria-invalid={Boolean(errors.email)}
                aria-describedby={errors.email ? 'entity-email-error' : undefined}
                onChange={(event) => { setEmail(event.target.value); if (errors.email) setErrors((current) => ({ ...current, email: undefined })); }}
              />
              {errors.email && <p id="entity-email-error" className="field-error" style={{ margin: '4px 0 0' }}>{errors.email}</p>}
            </div>
            <div>
              <label htmlFor="entity-role" className="field-label">{t('drawer.role')}</label>
              <input
                id="entity-role"
                className="field-input"
                value={role}
                aria-invalid={Boolean(errors.role)}
                aria-describedby={errors.role ? 'entity-role-error' : undefined}
                onChange={(event) => { setRole(event.target.value); if (errors.role) setErrors((current) => ({ ...current, role: undefined })); }}
              />
              {errors.role && <p id="entity-role-error" className="field-error" style={{ margin: '4px 0 0' }}>{errors.role}</p>}
            </div>
            <button type="submit" className="btn btn-primary" style={{ width: '100%' }}>{t('drawer.saveDetails')}</button>
          </form>

          <dl className="admin-meta">
            <div>
              <dt>{t('drawer.status')}</dt>
              <dd><StatusBadge status={STATUS_CHIP[status]} /></dd>
            </div>
            <div><dt>{t('drawer.recordId')}</dt><dd className="is-mono">{current.id}</dd></div>
            <div><dt>{t('drawer.memberSince')}</dt><dd>{current.since}</dd></div>
            {current.details.map((detail) => <div key={detail.label}><dt>{detail.label}</dt><dd>{detail.value}</dd></div>)}
          </dl>
        </div>

        <footer className="admin-drawer-foot">
          <button type="button" className="btn btn-secondary" onClick={() => updateStatus('Under review')}>{t('drawer.flagForReview')}</button>
          <button type="button" className="btn btn-secondary" onClick={() => updateStatus(status === 'Suspended' ? 'Active' : 'Suspended')}>
            {status === 'Suspended' ? t('drawer.restoreAccess') : t('drawer.suspendAccess')}
          </button>
        </footer>
      </div>
    </div>
  );
}
