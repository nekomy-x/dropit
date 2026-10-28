import type {
  AdminDj,
  AdminPayment,
  AuditLogEntry,
  Club,
} from '../api/client';

export type AdminUserRole = 'admin' | 'club_staff' | 'dj' | 'guest';
export type AdminUserStatus = 'active' | 'invited' | 'suspended' | 'blocked';

export interface AdminUser {
  id: string;
  displayName: string;
  email: string;
  role: AdminUserRole;
  status: AdminUserStatus;
  clubId: string | null;
  createdAt: string;
  lastSeenAt: string | null;
}

/** A transaction keeps the existing payment contract and adds admin relations. */
export interface AdminTransaction extends AdminPayment {
  clubId: string;
  djId: string | null;
}

export interface AdminCollectionMap {
  users: AdminUser;
  djs: AdminDj;
  clubs: Club;
  transactions: AdminTransaction;
  audit: AuditLogEntry;
}

export type AdminCollection = keyof AdminCollectionMap;
export type AdminEntity<K extends AdminCollection> = AdminCollectionMap[K];
export type AdminRecordInput<K extends AdminCollection> = Omit<AdminEntity<K>, 'id'> & {
  id?: string;
};
export type AdminRecordPatch<K extends AdminCollection> = Partial<Omit<AdminEntity<K>, 'id'>>;

export interface AdminSort<K extends AdminCollection> {
  field: keyof AdminEntity<K>;
  direction: 'asc' | 'desc';
}

export interface AdminQuery<K extends AdminCollection> {
  search?: string;
  /** Exact field match; an array means "match any". */
  filters?: Partial<Record<keyof AdminEntity<K>, unknown>>;
  /** When entities have a status field, match one or more statuses. */
  status?: string | readonly string[];
  sort?: readonly AdminSort<K>[];
  offset?: number;
  limit?: number;
}

export interface AdminBulkResult<T> {
  affected: number;
  items: T[];
  /** Immutable before-images allow callers to implement an explicit rollback. */
  before: T[];
}

export type AdminBulkAction<K extends AdminCollection> =
  | { type: 'update'; patch: AdminRecordPatch<K> }
  | { type: 'delete' };

export interface AdminRestoreResult {
  affected: number;
  restoredIds: string[];
}

const SEED_DATA: { [K in AdminCollection]: AdminEntity<K>[] } = {
  users: [
    {
      id: 'usr_1001', displayName: 'Marta Ruiz', email: 'marta@dropit.example', role: 'admin',
      status: 'active', clubId: null, createdAt: '2025-11-12T10:20:00.000Z', lastSeenAt: '2026-10-24T20:14:00.000Z',
    },
    {
      id: 'usr_1002', displayName: 'Leo Martin', email: 'leo@dropit.example', role: 'club_staff',
      status: 'active', clubId: 'club_204', createdAt: '2026-02-03T15:00:00.000Z', lastSeenAt: '2026-10-24T21:33:00.000Z',
    },
    {
      id: 'usr_1003', displayName: 'Nina Costa', email: 'nina@example.test', role: 'guest',
      status: 'invited', clubId: 'club_204', createdAt: '2026-10-23T18:42:00.000Z', lastSeenAt: null,
    },
  ],
  djs: [
    {
      id: 'dj_301', clubId: 'club_204', email: 'ana@mix.example', displayName: 'Ana Sol',
      stripeOnboardingStatus: 'verified', status: 'active', createdAt: '2026-03-14T20:00:00.000Z',
    },
    {
      id: 'dj_302', clubId: 'club_205', email: 'marco@mix.example', displayName: 'Marco Vale',
      stripeOnboardingStatus: 'pending', status: 'invited', createdAt: '2026-10-19T12:00:00.000Z',
    },
    {
      id: 'dj_303', clubId: null, email: 'jules@mix.example', displayName: 'Jules North',
      stripeOnboardingStatus: 'restricted', status: 'suspended', createdAt: '2025-12-02T09:30:00.000Z',
    },
  ],
  clubs: [
    {
      id: 'club_204', name: 'Sala Norte', city: 'Madrid', slug: 'sala-norte', address: 'Calle de la Luna 14',
      country: 'ES', email: 'ops@salanorte.example', phone: '+34 910 000 204', logoUrl: null,
      stripeAccountId: 'acct_mock_204', stripeOnboardingStatus: 'verified', status: 'active',
      createdAt: '2025-09-18T08:00:00.000Z',
    },
    {
      id: 'club_205', name: 'Club Bruma', city: 'Barcelona', slug: 'club-bruma', address: 'Carrer de Sants 52',
      country: 'ES', email: 'hello@bruma.example', phone: '+34 930 000 205', logoUrl: null,
      stripeAccountId: null, stripeOnboardingStatus: 'pending', status: 'pending_verification',
      createdAt: '2026-10-20T11:00:00.000Z',
    },
    {
      id: 'club_206', name: 'Bajo Cero', city: 'Valencia', slug: 'bajo-cero', address: 'Avinguda del Port 6',
      country: 'ES', email: 'team@bajocero.example', phone: '+34 960 000 206', logoUrl: null,
      stripeAccountId: 'acct_mock_206', stripeOnboardingStatus: 'restricted', status: 'suspended',
      createdAt: '2025-06-02T14:30:00.000Z',
    },
  ],
  transactions: [
    {
      id: 'txn_8001', requestId: 'req_5011', clubId: 'club_204', djId: 'dj_301', amountMinor: 850,
      currency: 'EUR', status: 'captured', refundedMinor: 0, failureReason: null,
      createdAt: '2026-10-24T20:42:00.000Z', capturedAt: '2026-10-24T20:42:03.000Z',
    },
    {
      id: 'txn_8002', requestId: 'req_5012', clubId: 'club_205', djId: 'dj_302', amountMinor: 1200,
      currency: 'EUR', status: 'failed', refundedMinor: 0, failureReason: 'Payment method declined',
      createdAt: '2026-10-24T20:51:00.000Z', capturedAt: null,
    },
    {
      id: 'txn_8003', requestId: 'req_5013', clubId: 'club_204', djId: 'dj_301', amountMinor: 600,
      currency: 'EUR', status: 'partially_refunded', refundedMinor: 300, failureReason: null,
      createdAt: '2026-10-24T21:04:00.000Z', capturedAt: '2026-10-24T21:04:02.000Z',
    },
  ],
  audit: [
    {
      id: 'aud_9001', actorType: 'admin', actorId: 'usr_1001', action: 'club.status_changed',
      entity: 'club_204', diff: { status: { from: 'pending_verification', to: 'active' } }, at: '2026-10-24T19:58:00.000Z',
    },
    {
      id: 'aud_9002', actorType: 'admin', actorId: 'usr_1001', action: 'dj.suspended',
      entity: 'dj_303', diff: { status: { from: 'active', to: 'suspended' } }, at: '2026-10-24T20:11:00.000Z',
    },
    {
      id: 'aud_9003', actorType: 'system', actorId: 'payments', action: 'transaction.failed',
      entity: 'txn_8002', diff: { reason: 'Payment method declined' }, at: '2026-10-24T20:51:02.000Z',
    },
  ],
};

type MutableStore = { [K in AdminCollection]: AdminEntity<K>[] };

/** Expand the compact hand-written fixtures into a deterministic operator-sized dataset. */
function expandSeedData(base: typeof SEED_DATA): MutableStore {
  const users = base.users.map((item) => ({ ...item }));
  const djs = base.djs.map((item) => ({ ...item }));
  const clubs = base.clubs.map((item) => ({ ...item }));
  const transactions = base.transactions.map((item) => ({ ...item }));
  const audit = base.audit.map((item) => ({ ...item, diff: item.diff ? { ...item.diff } : null }));
  const statuses: Array<AdminUserStatus> = ['active', 'active', 'invited', 'suspended'];
  const djStatuses: Array<AdminDj['status']> = ['active', 'active', 'invited', 'suspended'];
  const clubStatuses: Array<Club['status']> = ['active', 'active', 'pending_verification', 'suspended'];
  const paymentStatuses: Array<AdminTransaction['status']> = ['captured', 'processing', 'failed', 'refunded'];
  for (let index = 1; index <= 120; index += 1) {
    const clubId = `club_${300 + index}`;
    const djId = `dj_${400 + index}`;
    const userId = `usr_${500 + index}`;
    const transactionId = `txn_${900 + index}`;
    const at = new Date(Date.UTC(2026, 9, 24, 10 + (index % 12), index % 60)).toISOString();
    clubs.push({
      id: clubId, name: `Night Room ${String(index).padStart(3, '0')}`, city: ['Madrid', 'Barcelona', 'Valencia'][index % 3] ?? 'Madrid',
      slug: `night-room-${index}`, address: `${index} Avenida de la Noche`, country: 'ES',
      email: `ops${index}@dropit.example`, phone: `+34 910 000 ${String(index).padStart(3, '0')}`, logoUrl: null,
      stripeAccountId: index % 4 === 0 ? null : `acct_mock_${index}`,
      stripeOnboardingStatus: index % 4 === 0 ? 'pending' : 'verified', status: clubStatuses[index % clubStatuses.length] ?? 'active', createdAt: at,
    });
    djs.push({
      id: djId, clubId, email: `dj${index}@dropit.example`, displayName: `Resident DJ ${String(index).padStart(3, '0')}`,
      stripeOnboardingStatus: index % 4 === 0 ? 'pending' : 'verified', status: djStatuses[index % djStatuses.length] ?? 'active', createdAt: at,
    });
    users.push({
      id: userId, displayName: `Operator ${String(index).padStart(3, '0')}`, email: `user${index}@dropit.example`,
      role: index % 4 === 0 ? 'club_staff' : index % 5 === 0 ? 'dj' : 'guest', status: statuses[index % statuses.length] ?? 'active',
      clubId, createdAt: at, lastSeenAt: index % 7 === 0 ? null : at,
    });
    const paymentStatus = paymentStatuses[index % paymentStatuses.length] ?? 'captured';
    transactions.push({
      id: transactionId, requestId: `req_${1100 + index}`, clubId, djId, amountMinor: 500 + ((index * 125) % 4200), currency: 'EUR',
      status: paymentStatus, refundedMinor: paymentStatus === 'refunded' ? 500 : 0,
      failureReason: paymentStatus === 'failed' ? 'Payment method declined' : null, createdAt: at,
      capturedAt: paymentStatus === 'captured' ? at : null,
    });
    audit.push({
      id: `aud_${1200 + index}`, actorType: index % 6 === 0 ? 'system' : 'admin', actorId: index % 6 === 0 ? 'payments' : 'local-admin',
      action: `${index % 2 === 0 ? 'club' : 'transaction'}.${index % 3 === 0 ? 'updated' : 'created'}`,
      entity: index % 2 === 0 ? clubId : transactionId, diff: { generated: true, index }, at,
    });
  }
  return { users, djs, clubs, transactions, audit };
}

const INITIAL_DATA = expandSeedData(SEED_DATA);

function copySeedData(source: MutableStore = INITIAL_DATA): MutableStore {
  return {
    users: source.users.map((item) => ({ ...item })),
    djs: source.djs.map((item) => ({ ...item })),
    clubs: source.clubs.map((item) => ({ ...item })),
    transactions: source.transactions.map((item) => ({ ...item })),
    audit: source.audit.map((item) => ({ ...item, diff: item.diff ? { ...item.diff } : null })),
  };
}

const STORAGE_KEY = 'dropit.admin.mock.v1';
const STORAGE_VERSION = 1;
interface StoredAdminData {
  version: number;
  collections: MutableStore;
}

function persistStore(): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify({ version: STORAGE_VERSION, collections: store } satisfies StoredAdminData));
  } catch {
    // Storage may be disabled, quota-limited, or unavailable in private contexts.
  }
}

function loadStoredData(): MutableStore {
  if (typeof window === 'undefined') return copySeedData();
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return copySeedData();
    const parsed = JSON.parse(raw) as Partial<StoredAdminData>;
    if (parsed.version !== STORAGE_VERSION || !parsed.collections) return copySeedData();
    if (!(Object.keys(SEED_DATA) as AdminCollection[]).every((collection) => Array.isArray(parsed.collections?.[collection]))) return copySeedData();
    return copySeedData(parsed.collections);
  } catch {
    return copySeedData();
  }
}

let store = loadStoredData();
let nextId = 1;
const subscribers = new Map<AdminCollection, Set<() => void>>();

function notify(collection: AdminCollection): void {
  persistStore();
  subscribers.get(collection)?.forEach((subscriber) => subscriber());
}

export function subscribeAdminData(collection: AdminCollection, subscriber: () => void): () => void {
  let listeners = subscribers.get(collection);
  if (!listeners) {
    listeners = new Set();
    subscribers.set(collection, listeners);
  }
  listeners.add(subscriber);
  return () => {
    listeners?.delete(subscriber);
    if (listeners?.size === 0) subscribers.delete(collection);
  };
}

/** Stable snapshot for React's useSyncExternalStore. Mutations replace the collection array. */
export function getAdminDataSnapshot<K extends AdminCollection>(collection: K): AdminEntity<K>[] {
  return store[collection];
}

function toSearchableString(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

function matchesFilter(value: unknown, expected: unknown): boolean {
  if (Array.isArray(expected)) return expected.some((candidate) => matchesFilter(value, candidate));
  if (typeof value === 'string' && typeof expected === 'string') {
    return value.toLocaleLowerCase() === expected.toLocaleLowerCase();
  }
  return value === expected;
}

function compareValues(a: unknown, b: unknown): number {
  if (a === b) return 0;
  if (a === null || a === undefined) return 1;
  if (b === null || b === undefined) return -1;
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  if (typeof a === 'boolean' && typeof b === 'boolean') return Number(a) - Number(b);
  return toSearchableString(a).localeCompare(toSearchableString(b), undefined, {
    numeric: true,
    sensitivity: 'base',
  });
}

/** Pure filtering and stable multi-column sorting; safe to use directly in components. */
export function selectAdminData<K extends AdminCollection>(
  rows: readonly AdminEntity<K>[],
  query: AdminQuery<K> = {},
): AdminEntity<K>[] {
  const search = query.search?.trim().toLocaleLowerCase();
  const filters = Object.entries(query.filters ?? {}) as Array<[keyof AdminEntity<K>, unknown]>;
  const statuses = query.status === undefined
    ? null
    : (Array.isArray(query.status) ? query.status : [query.status]).map((status) => status.toLocaleLowerCase());
  const statusKey = 'status' as keyof AdminEntity<K>;

  let result = rows.filter((row) => {
    const record = row as Record<keyof AdminEntity<K>, unknown>;
    if (search && !Object.values(record).some((value) => toSearchableString(value).toLocaleLowerCase().includes(search))) {
      return false;
    }
    if (!filters.every(([field, expected]) => matchesFilter(record[field], expected))) return false;
    if (statuses && !statuses.includes(toSearchableString(record[statusKey]).toLocaleLowerCase())) return false;
    return true;
  });

  if (query.sort?.length) {
    result = result
      .map((row, index) => ({ row, index }))
      .sort((left, right) => {
        for (const rule of query.sort ?? []) {
          const comparison = compareValues(
            (left.row as Record<keyof AdminEntity<K>, unknown>)[rule.field],
            (right.row as Record<keyof AdminEntity<K>, unknown>)[rule.field],
          );
          if (comparison !== 0) return rule.direction === 'asc' ? comparison : -comparison;
        }
        return left.index - right.index;
      })
      .map(({ row }) => row);
  }

  const offset = Math.max(0, query.offset ?? 0);
  const end = query.limit === undefined ? undefined : offset + Math.max(0, query.limit);
  return result.slice(offset, end);
}

export function listAdminData<K extends AdminCollection>(
  collection: K,
  query: AdminQuery<K> = {},
): AdminEntity<K>[] {
  return selectAdminData(store[collection], query);
}

function createId(collection: AdminCollection): string {
  const prefixes: Record<AdminCollection, string> = {
    users: 'usr', djs: 'dj', clubs: 'club', transactions: 'txn', audit: 'aud',
  };
  return `${prefixes[collection]}_mock_${Date.now().toString(36)}_${nextId++}`;
}

function addAuditEntry(action: string, entity: string, diff: Record<string, unknown> | null): void {
  const row: AuditLogEntry = {
    id: createId('audit'), actorType: 'admin', actorId: 'local-admin', action, entity,
    diff, at: new Date().toISOString(),
  };
  store.audit = [row, ...store.audit];
  notify('audit');
}

export function createAdminRecord<K extends AdminCollection>(
  collection: K,
  input: AdminRecordInput<K>,
): AdminEntity<K> {
  const item = { ...input, id: input.id ?? createId(collection) } as AdminEntity<K>;
  store[collection] = [item, ...store[collection]] as MutableStore[K];
  if (collection !== 'audit') addAuditEntry(`${collection}.created`, item.id, null);
  notify(collection);
  return item;
}

export function updateAdminRecord<K extends AdminCollection>(
  collection: K,
  id: string,
  patch: AdminRecordPatch<K>,
): AdminEntity<K> {
  const existing = store[collection].find((item) => item.id === id);
  if (!existing) throw new Error(`${collection} record ${id} was not found`);
  const updated = { ...existing, ...patch, id } as AdminEntity<K>;
  store[collection] = store[collection].map((item) => item.id === id ? updated : item) as MutableStore[K];
  if (collection !== 'audit') addAuditEntry(`${collection}.updated`, id, { before: existing, after: updated });
  notify(collection);
  return updated;
}

export function deleteAdminRecord<K extends AdminCollection>(collection: K, id: string): boolean {
  const current = store[collection];
  const existing = current.find((item) => item.id === id);
  if (!existing) return false;
  store[collection] = current.filter((item) => item.id !== id) as MutableStore[K];
  if (collection !== 'audit') addAuditEntry(`${collection}.deleted`, id, { before: existing });
  notify(collection);
  return true;
}

export function bulkAdminAction<K extends AdminCollection>(
  collection: K,
  ids: readonly string[],
  action: AdminBulkAction<K>,
): AdminBulkResult<AdminEntity<K>> {
  const selected = new Set(ids);
  const matched = store[collection].filter((item) => selected.has(item.id));
  const beforeSnapshot = matched.map((item) => JSON.parse(JSON.stringify(item)) as AdminEntity<K>);
  if (action.type === 'delete') {
    store[collection] = store[collection].filter((item) => !selected.has(item.id)) as MutableStore[K];
    if (collection !== 'audit' && matched.length) {
      addAuditEntry(`${collection}.bulk_deleted`, matched.map((item) => item.id).join(','), { affected: matched.length });
    }
    if (matched.length) notify(collection);
    return { affected: matched.length, items: [], before: beforeSnapshot };
  }

  const before = beforeSnapshot;
  const updatedItems = matched.map((item) => ({ ...item, ...action.patch, id: item.id }) as AdminEntity<K>);
  const updatedById = new Map(updatedItems.map((item) => [item.id, item]));
  store[collection] = store[collection].map((item) => updatedById.get(item.id) ?? item) as MutableStore[K];
  if (collection !== 'audit' && updatedItems.length) {
    addAuditEntry(`${collection}.bulk_updated`, updatedItems.map((item) => item.id).join(','), { before, after: updatedItems, patch: action.patch, affected: updatedItems.length });
  }
  if (updatedItems.length) notify(collection);
  return { affected: updatedItems.length, items: updatedItems, before };
}

/** Restore rows from a bulk result's before-image (works for updates and deletes). */
export function restoreAdminSnapshot<K extends AdminCollection>(
  collection: K,
  before: readonly AdminEntity<K>[],
): AdminRestoreResult {
  if (!before.length) return { affected: 0, restoredIds: [] };
  const byId = new Map(before.map((item) => [item.id, item]));
  const current = store[collection];
  const existingIds = new Set(current.map((item) => item.id));
  const restoredIds = before.map((item) => item.id);
  const restored = current.map((item) => byId.get(item.id) ?? item);
  before.forEach((item) => {
    if (!existingIds.has(item.id)) restored.push(item);
  });
  store[collection] = restored as MutableStore[K];
  if (collection !== 'audit') addAuditEntry(`${collection}.bulk_restored`, restoredIds.join(','), { restoredIds });
  notify(collection);
  return { affected: restoredIds.length, restoredIds };
}

export type AdminExportFormat = 'csv' | 'json';

function csvValue(value: unknown): string {
  const normalized = value === null || value === undefined
    ? ''
    : typeof value === 'object' ? JSON.stringify(value) : String(value);
  return /[",\r\n]/.test(normalized) ? `"${normalized.replaceAll('"', '""')}"` : normalized;
}

export function serializeAdminData<K extends AdminCollection>(
  rows: readonly AdminEntity<K>[],
  format: AdminExportFormat,
  columns?: readonly (keyof AdminEntity<K> & string)[],
): string {
  if (format === 'json') return JSON.stringify(rows, null, 2);
  const exportColumns = columns?.length
    ? [...columns]
    : [...new Set(rows.flatMap((row) => Object.keys(row) as Array<keyof AdminEntity<K> & string>))];
  if (exportColumns.length === 0) return '';
  const header = exportColumns.map(csvValue).join(',');
  const body = rows.map((row) => {
    const values = row as unknown as Record<string, unknown>;
    return exportColumns.map((column) => csvValue(values[column])).join(',');
  });
  return [header, ...body].join('\r\n');
}

export function exportAdminData<K extends AdminCollection>(
  collection: K,
  format: AdminExportFormat = 'json',
  rows: readonly AdminEntity<K>[] = store[collection],
  columns?: readonly (keyof AdminEntity<K> & string)[],
): string {
  return serializeAdminData(rows, format, columns);
}

export function downloadAdminData<K extends AdminCollection>(
  collection: K,
  format: AdminExportFormat = 'csv',
  rows: readonly AdminEntity<K>[] = store[collection],
  columns?: readonly (keyof AdminEntity<K> & string)[],
): void {
  if (typeof document === 'undefined' || typeof URL === 'undefined') return;
  const content = exportAdminData(collection, format, rows, columns);
  const type = format === 'csv' ? 'text/csv;charset=utf-8' : 'application/json;charset=utf-8';
  const blob = new Blob([content], { type });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `dropit-admin-${collection}-${new Date().toISOString().slice(0, 10)}.${format}`;
  anchor.click();
  URL.revokeObjectURL(url);
}

/** Reset utility for tests and story/dev consumers; resets every collection to its seed. */
export function resetAdminMockData(): void {
  store = copySeedData();
  persistStore();
  for (const collection of Object.keys(SEED_DATA) as AdminCollection[]) notify(collection);
}
