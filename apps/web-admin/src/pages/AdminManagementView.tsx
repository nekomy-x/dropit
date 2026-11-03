import { useEffect, useMemo, useState, type FormEvent } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { useAdminData } from '../hooks/useAdminData';
import type { AdminCollection, AdminEntity } from '../data/adminMock';
import type { ClubStatus, DjStatus, PaymentStatus } from '../api/client';
import PowerTable, { type PowerTableBulkAction, type PowerTableColumn } from '../components/admin-table/PowerTable';
import EntityDetailDrawer, { type ConsoleEntity, type ConsoleEntityStatus } from '../components/admin-console/EntityDetailDrawer';
import QuickActionConsole from '../components/admin-console/QuickActionConsole';
import StatusBadge from '../components/StatusBadge';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import Select from '../components/ui/Select';
import { useLocale, type Translate } from '../locale';
import { STATUS_MESSAGE_KEY } from '../messages/status';

type Row = Record<string, unknown> & { id: string };
export type ManagementView = 'users' | 'djs' | 'clubs' | 'transactions';

/** Narrow seam over the mock hooks so every collection view shares one call contract. */
interface StoreAdapter {
  data: Row[];
  create: (input: Record<string, unknown>) => void;
  update: (id: string, patch: Record<string, unknown>) => void;
  remove: (id: string) => void;
  bulk: (ids: readonly string[], action: { type: 'update'; patch: Record<string, unknown> } | { type: 'delete' }) => { affected: number };
  download: (format: 'csv' | 'json', rows: readonly AdminEntity<AdminCollection>[]) => void;
}

const fmtCurrency = (minor: number, currency: string, locale: string) =>
  new Intl.NumberFormat(locale, { style: 'currency', currency, minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(minor / 100);
const fmtNumber = (value: number, locale: string) => new Intl.NumberFormat(locale).format(value);
const fmtDate = (value: unknown, locale: string) =>
  typeof value === 'string' && value ? new Intl.DateTimeFormat(locale, { day: '2-digit', month: 'short', year: '2-digit', hour: '2-digit', minute: '2-digit' }).format(new Date(value)) : '—';

/** Stored role code → catalog key. Kept apart from statuses: a role is not a state. */
const ROLE_MESSAGE_KEY: Record<string, string> = {
  admin: 'management.roleAdmin',
  club_staff: 'management.roleClubStaff',
  dj: 'management.roleDj',
  guest: 'management.roleGuest',
};

interface FieldSpec {
  key: string;
  header: string;
  editable?: boolean;
  options?: readonly string[];
  /** Column width hint; the primary column is allowed to take the most room. */
  width?: string;
}

interface CollectionSpec {
  collection: AdminCollection;
  title: string;
  createLabel: string;
  fields: readonly FieldSpec[];
  statuses: readonly string[];
  bulkActions: readonly { label: string; action: BulkActionKind }[];
}

type BulkActionKind = 'approve' | 'suspend' | 'delete' | 'role' | 'refund' | 'capture';

/** The four collection views, translated on the active language. */
function buildSpecs(t: Translate): Record<ManagementView, CollectionSpec> {
  return {
    users: {
      collection: 'users', title: t('management.users'),
      createLabel: t('management.addUser'),
      fields: [
        { key: 'displayName', header: t('management.fieldUser'), editable: true, width: '22%' },
        { key: 'email', header: t('management.fieldEmail'), editable: true, width: '20%' },
        { key: 'role', header: t('management.fieldRole'), editable: true, options: ['admin', 'club_staff', 'dj', 'guest'] },
        { key: 'status', header: t('management.fieldStatus'), editable: true, options: ['active', 'invited', 'suspended', 'blocked'] },
        { key: 'club', header: t('management.fieldWorkspace') },
        { key: 'createdAt', header: t('management.fieldCreated') },
        { key: 'lastSeen', header: t('management.fieldLastSeen') },
      ],
      statuses: ['active', 'invited', 'suspended', 'blocked'],
      bulkActions: [
        { label: t('management.actionApprove'), action: 'approve' }, { label: t('management.actionSuspend'), action: 'suspend' },
        { label: t('management.actionChangeRole'), action: 'role' }, { label: t('management.actionDelete'), action: 'delete' },
      ],
    },
    djs: {
      collection: 'djs', title: t('management.djs'),
      createLabel: t('management.inviteDj'),
      fields: [
        { key: 'displayName', header: t('management.fieldDj'), editable: true, width: '22%' },
        { key: 'email', header: t('management.fieldEmail'), editable: true, width: '20%' },
        { key: 'club', header: t('management.fieldClub') },
        { key: 'onboarding', header: t('management.fieldPayoutVerification') },
        { key: 'status', header: t('management.fieldStatus'), editable: true, options: ['invited', 'active', 'suspended', 'blocked'] },
        { key: 'createdAt', header: t('management.fieldCreated') },
      ],
      statuses: ['active', 'invited', 'suspended', 'blocked'],
      bulkActions: [
        { label: t('management.actionApprove'), action: 'approve' }, { label: t('management.actionSuspend'), action: 'suspend' }, { label: t('management.actionDelete'), action: 'delete' },
      ],
    },
    clubs: {
      collection: 'clubs', title: t('management.clubs'),
      createLabel: t('management.addClub'),
      fields: [
        { key: 'name', header: t('management.fieldClub'), editable: true, width: '24%' },
        { key: 'city', header: t('management.fieldCity'), editable: true, width: '14%' },
        { key: 'email', header: t('management.fieldContact') },
        { key: 'onboarding', header: t('management.fieldPayments') },
        { key: 'status', header: t('management.fieldStatus'), editable: true, options: ['pending_verification', 'active', 'suspended', 'blocked'] },
        { key: 'createdAt', header: t('management.fieldAdded') },
      ],
      statuses: ['pending_verification', 'active', 'suspended', 'blocked'],
      bulkActions: [
        { label: t('management.actionApprove'), action: 'approve' }, { label: t('management.actionSuspend'), action: 'suspend' }, { label: t('management.actionDelete'), action: 'delete' },
      ],
    },
    transactions: {
      collection: 'transactions', title: t('management.transactions'),
      createLabel: t('management.addTransaction'),
      fields: [
        { key: 'requestId', header: t('management.fieldRequest'), width: '18%' },
        { key: 'club', header: t('management.fieldClub') },
        { key: 'dj', header: t('management.fieldDj') },
        { key: 'amount', header: t('management.fieldAmount') },
        { key: 'status', header: t('management.fieldStatus'), editable: true, options: ['intent', 'processing', 'captured', 'partially_refunded', 'refunded', 'failed', 'cancelled'] },
        { key: 'createdAt', header: t('management.fieldCreated') },
      ],
      statuses: ['intent', 'processing', 'captured', 'partially_refunded', 'refunded', 'failed', 'cancelled'],
      bulkActions: [
        { label: t('management.actionMarkCaptured'), action: 'capture' }, { label: t('management.actionRefund'), action: 'refund' }, { label: t('management.actionDelete'), action: 'delete' },
      ],
    },
  };
}

const ACTION_VERB_KEY: Record<BulkActionKind, string> = {
  approve: 'management.verbApproved',
  suspend: 'management.verbSuspended',
  delete: 'management.verbDeleted',
  role: 'management.verbRole',
  refund: 'management.verbRefunded',
  capture: 'management.verbCaptured',
};

/** One place where a stored record becomes the shared drawer model. */
function toEntity(
  view: ManagementView,
  record: Row,
  lookups: { club: Map<string, string>; dj: Map<string, string> },
  locale: string,
  t: Translate,
): ConsoleEntity {
  const status = String(record.status ?? '');
  const consoleStatus: ConsoleEntityStatus =
    status === 'suspended' || status === 'blocked' ? 'Suspended'
      : status === 'invited' || status === 'pending_verification' || status === 'pending' ? 'Under review' : 'Active';
  const name = view === 'transactions'
    ? t('management.paymentRef', { id: String(record.id).slice(0, 12) })
    : String(record.name ?? record.displayName ?? record.id);
  const email = view === 'transactions' ? t('management.transactionRecord') : String(record.email ?? '—');
  const since = String(record.createdAt ?? '').slice(0, 10);
  const details: Array<{ label: string; value: string }> = [];
  const statusLabel = (value: unknown) => {
    const raw = String(value ?? '');
    const key = ROLE_MESSAGE_KEY[raw] ?? STATUS_MESSAGE_KEY[raw];
    return key ? t(key) : raw || '—';
  };

  if (view === 'users') {
    details.push(
      { label: t('management.fieldRole'), value: statusLabel(record.role) },
      { label: t('management.fieldWorkspace'), value: String(record.club ?? t('management.platform')) },
      { label: t('management.fieldLastSeen'), value: fmtDate(record.lastSeenAt, locale) },
      { label: t('management.fieldCreated'), value: fmtDate(record.createdAt, locale) },
    );
    return { id: record.id, kind: 'User', name, email, role: statusLabel(record.role), status: consoleStatus, since, details };
  }
  if (view === 'djs') {
    details.push(
      { label: t('management.fieldClub'), value: String(record.club ?? t('management.unassigned')) },
      { label: t('management.fieldPayoutVerification'), value: statusLabel(record.stripeOnboardingStatus) },
      { label: t('management.fieldCreated'), value: fmtDate(record.createdAt, locale) },
    );
    return { id: record.id, kind: 'DJ', name, email, role: t('management.roleDj'), status: consoleStatus, since, details };
  }
  if (view === 'clubs') {
    details.push(
      { label: t('management.fieldCity'), value: String(record.city ?? '—') },
      { label: t('management.fieldPayments'), value: statusLabel(record.stripeOnboardingStatus) },
      { label: t('management.fieldAddress'), value: String(record.address ?? '—') },
      { label: t('management.fieldAdded'), value: fmtDate(record.createdAt, locale) },
    );
    return { id: record.id, kind: 'Club', name, email, role: t('management.entityRoleVenue'), status: consoleStatus, since, details };
  }
  details.push(
    { label: t('management.fieldRequest'), value: String(record.requestId ?? '—') },
    { label: t('management.fieldClub'), value: String(lookups.club.get(String(record.clubId)) ?? '—') },
    { label: t('management.fieldDj'), value: String(lookups.dj.get(String(record.djId)) ?? '—') },
    { label: t('management.fieldAmount'), value: fmtCurrency(Number(record.amountMinor ?? 0), String(record.currency ?? 'EUR'), locale) },
    { label: t('management.detailCaptured'), value: fmtDate(record.capturedAt, locale) },
  );
  return { id: record.id, kind: 'User', name, email, role: t('management.entityRoleTransaction'), status: consoleStatus, since, details };
}

/** Applies a bulk action to whichever collection is active. */
function buildPatch(view: ManagementView, action: BulkActionKind): Record<string, unknown> | null {
  if (action === 'role') return view === 'users' ? { role: 'club_staff' } : null;
  if (action === 'approve') {
    if (view === 'clubs') return { status: 'active' as ClubStatus };
    if (view === 'transactions') return { status: 'captured' as PaymentStatus };
    return { status: 'active' };
  }
  if (action === 'suspend') return { status: view === 'clubs' ? ('suspended' as ClubStatus) : ('suspended' as DjStatus) };
  if (action === 'capture') return view === 'transactions' ? { status: 'captured' as PaymentStatus } : null;
  if (action === 'refund') return view === 'transactions' ? { status: 'refunded' as PaymentStatus, refundedMinor: 0 } : null;
  return null;
}

interface AdminManagementViewProps { view: ManagementView }

export default function AdminManagementView({ view }: AdminManagementViewProps) {
  const navigate = useNavigate();
  const { t, locale } = useLocale();
  const specs = useMemo(() => buildSpecs(t), [t]);
  const spec = specs[view];
  const [searchParams, setSearchParams] = useSearchParams();
  const [search, setSearch] = useState('');
  const [status, setStatusState] = useState(() => searchParams.get('status') ?? '');
  const setStatus = (nextStatus: string) => {
    setStatusState(nextStatus);
    const next = new URLSearchParams(searchParams);
    if (nextStatus) next.set('status', nextStatus); else next.delete('status');
    setSearchParams(next, { replace: true });
  };
  const [_visibleRows, setVisibleRows] = useState<Row[]>([]);
  const [selectedRows, setSelectedRows] = useState<Row[]>([]);
  const [entity, setEntity] = useState<ConsoleEntity | null>(null);
  const [drawerRecord, setDrawerRecord] = useState<Row | null>(null);
  const [toast, setToast] = useState('');
  const [showCreate, setShowCreate] = useState(false);
  const [draftName, setDraftName] = useState('');
  const [draftEmail, setDraftEmail] = useState('');

  const users = useAdminData('users');
  const djs = useAdminData('djs');
  const clubs = useAdminData('clubs');
  const transactions = useAdminData('transactions');

  const clubNames = useMemo(() => new Map(clubs.data.map((club) => [club.id, club.name])), [clubs.data]);
  const djNames = useMemo(() => new Map(djs.data.map((dj) => [dj.id, dj.displayName])), [djs.data]);
  const lookups = useMemo(() => ({ club: clubNames, dj: djNames }), [clubNames, djNames]);

  const rawRows = view === 'users' ? users.data : view === 'djs' ? djs.data : view === 'clubs' ? clubs.data : transactions.data;

  const store = useMemo<StoreAdapter>(() => {
    const source = view === 'users' ? users : view === 'djs' ? djs : view === 'clubs' ? clubs : transactions;
    return {
      data: source.data as unknown as Row[],
      create: (input) => { (source.create as unknown as (i: Record<string, unknown>) => void)(input); },
      update: (id, patch) => { (source.update as unknown as (i: string, p: Record<string, unknown>) => void)(id, patch); },
      remove: (id) => { (source.remove as unknown as (i: string) => void)(id); },
      bulk: (ids, action) => {
        const result = (source.bulk as unknown as (i: readonly string[], a: unknown) => { affected: number })(ids, action);
        return { affected: result.affected };
      },
      download: (format, rows) => { (source.download as unknown as (f: 'csv' | 'json', r: readonly AdminEntity<AdminCollection>[]) => void)(format, rows); },
    };
  }, [view, users, djs, clubs, transactions]);

  const rows = useMemo<Row[]>(() => rawRows.map((record) => {
    const base = record as unknown as Row;
    if (view === 'users') {
      return { ...base, club: base.clubId ? clubNames.get(String(base.clubId)) ?? String(base.clubId) : t('management.platform'), createdAt: fmtDate(base.createdAt, locale), lastSeen: fmtDate(base.lastSeenAt, locale) };
    }
    if (view === 'djs') {
      return { ...base, club: base.clubId ? clubNames.get(String(base.clubId)) ?? String(base.clubId) : t('management.unassigned'), createdAt: fmtDate(base.createdAt, locale) };
    }
    if (view === 'clubs') return { ...base, createdAt: fmtDate(base.createdAt, locale) };
    return {
      ...base,
      club: clubNames.get(String(base.clubId)) ?? String(base.clubId),
      dj: base.djId ? djNames.get(String(base.djId)) ?? String(base.djId) : '—',
      amount: fmtCurrency(Number(base.amountMinor ?? 0), String(base.currency ?? 'EUR'), locale),
      createdAt: fmtDate(base.createdAt, locale),
    };
  }), [rawRows, view, clubNames, djNames, locale, t]);

  /** Deep link from the Overview session monitor: /admin/clubs?record=club_204. */
  const recordId = searchParams.get('record');
  useEffect(() => {
    if (!recordId) return;
    const match = rawRows.find((record) => record.id === recordId);
    if (!match) return;
    setDrawerRecord(match as unknown as Row);
    setEntity(toEntity(view, match as unknown as Row, lookups, locale, t));
    const next = new URLSearchParams(searchParams);
    next.delete('record');
    setSearchParams(next, { replace: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recordId]);

  const columns = useMemo<PowerTableColumn<Row>[]>(() => spec.fields.map((field) => ({
    key: field.key,
    header: field.header,
    editable: field.editable,
    width: field.width,
    searchable: true,
    render: field.key === 'status' || field.key === 'onboarding'
      ? (value) => <StatusBadge status={String(value ?? 'unknown')} />
      : undefined,
    edit: field.options
      ? (value, _row, onChange) => {
          const options = field.options ?? [];
          return (
            <Select
              size="sm"
              className="power-table-edit-select"
              ariaLabel={t('management.editField', { field: field.header })}
              value={String(value ?? '')}
              onChange={onChange}
              options={options.map((option) => {
                const roleKey = ROLE_MESSAGE_KEY[option];
                const statusKey = STATUS_MESSAGE_KEY[option];
                const key = roleKey ?? statusKey;
                return { value: option, label: key ? t(key) : option };
              })}
            />
          );
        }
      : undefined,
  })), [spec.fields, t]);

  const applyBulk = (rowsToApply: readonly Row[], action: BulkActionKind) => {
    const ids = rowsToApply.map((row) => row.id);
    if (ids.length === 0) return;
    if (action === 'delete' && !window.confirm(t('management.confirmDelete', { count: ids.length }))) return;
    const patch = buildPatch(view, action);
    const affected = action === 'delete'
      ? store.bulk(ids, { type: 'delete' }).affected
      : patch ? store.bulk(ids, { type: 'update', patch }).affected : 0;
    if (affected === 0) {
      setToast(t('management.toastNoChanges', { count: ids.length }));
      return;
    }
    setToast(t('management.toastAffected', { count: fmtNumber(affected, locale), verb: t(ACTION_VERB_KEY[action]) }));
  };

  const bulkActions = useMemo<PowerTableBulkAction<Row>[]>(
    () => spec.bulkActions.map(({ label: actionLabel, action }) => ({
      label: actionLabel,
      // Only the delete verb destroys data; it is the one bullet that renders
      // as the shared trash control.
      destructive: action === 'delete',
      onSelect: (rowsToApply) => applyBulk(rowsToApply, action),
    })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [spec.bulkActions, store],
  );

  const openRecord = (row: Row) => {
    const match = rawRows.find((record) => record.id === row.id);
    if (!match) return;
    const record = match as unknown as Row;
    setDrawerRecord(record);
    setEntity(toEntity(view, record, lookups, locale, t));
  };

  const saveRecord = (changes: { name: string; email: string; role: string }) => {
    if (!drawerRecord) return;
    if (view === 'users') store.update(drawerRecord.id, { displayName: changes.name, email: changes.email, role: changes.role });
    else if (view === 'djs') store.update(drawerRecord.id, { displayName: changes.name, email: changes.email });
    else if (view === 'clubs') store.update(drawerRecord.id, { name: changes.name, email: changes.email });
    setToast(t('management.toastSaved'));
  };

  const changeStatus = (next: ConsoleEntityStatus) => {
    if (!drawerRecord) return;
    const nextStatus = next === 'Suspended' ? 'suspended' : next === 'Under review' ? 'invited' : 'active';
    if (view === 'clubs') store.update(drawerRecord.id, { status: (next === 'Under review' ? 'pending_verification' : nextStatus) as ClubStatus });
    else store.update(drawerRecord.id, { status: nextStatus });
    const recordName = String(drawerRecord.name ?? drawerRecord.displayName ?? drawerRecord.id);
    const statusKey = next === 'Suspended' ? 'status.suspended' : next === 'Under review' ? 'status.underReview' : 'status.active';
    setToast(t('management.toastStatus', { name: recordName, status: t(statusKey) }));
  };

  const inlineEdit = (row: Row, key: string, value: unknown) => {
    store.update(row.id, { [key]: value });
    setToast(t('management.toastFieldUpdated', { field: spec.fields.find((field) => field.key === key)?.header ?? t('management.fieldFallback') }));
  };

  const createRecord = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = draftName.trim();
    const email = draftEmail.trim();
    if (!name) return;
    const now = new Date().toISOString();
    if (view === 'users') store.create({ displayName: name, email: email || 'new.user@example.test', role: 'guest', status: 'active', clubId: null, createdAt: now, lastSeenAt: null });
    if (view === 'djs') store.create({ displayName: name, email: email || 'new.dj@example.test', clubId: clubs.data[0]?.id ?? null, stripeOnboardingStatus: 'pending', status: 'invited', createdAt: now });
    if (view === 'clubs') store.create({ name, email: email || 'ops@example.test', city: 'Berlin', slug: name.toLowerCase().replaceAll(/\s+/g, '-'), address: 'New venue', country: 'DE', phone: '', logoUrl: null, stripeAccountId: null, stripeOnboardingStatus: 'pending', status: 'pending_verification', createdAt: now });
    if (view === 'transactions') store.create({ requestId: `req_${Date.now()}`, clubId: clubs.data[0]?.id ?? 'club_unassigned', djId: djs.data[0]?.id ?? null, amountMinor: 1200, currency: 'EUR', status: 'captured', refundedMinor: 0, failureReason: null, createdAt: now, capturedAt: now });
    setDraftName(''); setDraftEmail(''); setShowCreate(false);
    setToast(t('management.toastCreated', { name, noun: spec.title }));
  };

  const exportRows = (format: 'csv' | 'json') => {
    const ids = new Set(selectedRows.map((row) => row.id));
    const source = selectedRows.length > 0 ? rawRows.filter((record) => ids.has(record.id)) : rawRows;
    store.download(format, source as unknown as AdminEntity<AdminCollection>[]);
    setToast(t('management.toastExported', { count: fmtNumber(source.length, locale), format: format.toUpperCase() }));
  };

  const totalVolume = view === 'transactions'
    ? fmtCurrency(rawRows.reduce((sum, record) => sum + Number((record as unknown as Row).amountMinor ?? 0), 0), 'EUR', locale)
    : null;

  const recordNoun = spec.title.toLowerCase();

  return (
    <div>
      <PageHeading
        title={spec.title}
        action={(
          <div className="admin-page-actions">
            <QuickActionConsole onAction={(action) => {
              const target: Record<string, string> = { review: 'users', suspend: 'clubs', audit: 'activity', role: 'access' };
              const section = target[action];
              if (section && section !== view) navigate(`/manage/${section}`);
            }} />
            <button type="button" className="btn btn-secondary" onClick={() => exportRows('csv')}>{t('management.exportCsv')}</button>
            <button type="button" className="btn btn-secondary" onClick={() => exportRows('json')}>{t('management.exportJson')}</button>
            <button type="button" className="btn btn-primary" onClick={() => setShowCreate((current) => !current)}>
              <Icon name={showCreate ? 'lucide:x' : 'lucide:plus'} size={15} />
              {showCreate ? t('common.cancel') : spec.createLabel}
            </button>
          </div>
        )}
      />

      {/*
        Provenance line: this whole surface is the local preview store, and the
        counts below are its record counts — not platform metrics.
      */}
      <p className="admin-page-note" role="status">
        <strong>{t('management.previewTitle')}</strong>{t('management.previewBody', { count: fmtNumber(rawRows.length, locale), noun: spec.title })}
        {totalVolume ? t('management.previewVolume', { amount: totalVolume }) : ''}{t('management.previewTail')}
      </p>

      {showCreate && (
        <section className="panel-card" style={{ marginBottom: 16 }}>
          <header className="panel-card-head">
            <div>
              <h2 className="panel-card-title">{spec.createLabel}</h2>
              <p className="panel-card-sub">{t('management.createSubtitle')}</p>
            </div>
          </header>
          <form onSubmit={createRecord}>
            <div className="admin-form-row">
              <label className="admin-field">
                <span>{view === 'clubs' ? t('management.fieldVenueName') : view === 'transactions' ? t('management.fieldReference') : t('management.fieldDisplayName')}</span>
                <input
                  name="name"
                  autoComplete="off"
                  className="field-input"
                  value={draftName}
                  onChange={(event) => setDraftName(event.target.value)}
                  placeholder={t('management.namePlaceholder')}
                  required
                />
              </label>
              {view !== 'clubs' && (
                <label className="admin-field">
                  <span>{t('management.fieldEmail')}</span>
                  <input
                    name="email"
                    type="email"
                    autoComplete="off"
                    spellCheck={false}
                    className="field-input"
                    value={draftEmail}
                    onChange={(event) => setDraftEmail(event.target.value)}
                    placeholder="name@example.com"
                    required
                  />
                </label>
              )}
            </div>
            <div className="admin-form-actions" style={{ marginTop: 14 }}>
              <button type="submit" className="btn btn-primary">{t('management.createRecord')}</button>
            </div>
          </form>
        </section>
      )}

      <PowerTable<Row>
        rows={rows}
        columns={columns}
        search={search}
        onSearchChange={setSearch}
        statusField="status"
        statusOptions={spec.statuses}
        status={status}
        onStatusChange={setStatus}
        selection
        onSelectionChange={(_ids, rowsSelected) => setSelectedRows(rowsSelected)}
        bulkActions={bulkActions}
        onInlineEdit={inlineEdit}
        onRowOpen={openRecord}
        onViewChange={setVisibleRows}
        initialSort={[{ key: 'createdAt', direction: 'desc' }]}
        searchPlaceholder={t('management.searchPlaceholder', { noun: recordNoun })}
        emptyState={t('management.emptyState', { noun: recordNoun })}
        recordNoun={recordNoun}
      />

      <p className="admin-table-hint">{t('management.tableHint')}</p>

      {toast ? <p className="admin-toast" role="status" aria-live="polite">{toast}</p> : null}

      {entity && (
        <EntityDetailDrawer
          key={`${entity.kind}:${entity.id}`}
          entity={entity}
          onClose={() => { setEntity(null); setDrawerRecord(null); }}
          onSave={saveRecord}
          onStatusChange={changeStatus}
        />
      )}
    </div>
  );
}