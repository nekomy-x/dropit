import { useCallback, useMemo, useSyncExternalStore } from 'react';
import {
  bulkAdminAction,
  createAdminRecord,
  deleteAdminRecord,
  downloadAdminData,
  exportAdminData,
  getAdminDataSnapshot,
  listAdminData,
  restoreAdminSnapshot,
  selectAdminData,
  subscribeAdminData,
  updateAdminRecord,
  type AdminBulkAction,
  type AdminBulkResult,
  type AdminCollection,
  type AdminEntity,
  type AdminExportFormat,
  type AdminQuery,
  type AdminRecordInput,
  type AdminRecordPatch,
} from '../data/adminMock';

/**
 * A frozen default so the memo below is not recomputed on every render when a
 * caller does not need a query (the common case for the management views).
 */
const EMPTY_QUERY: Record<string, unknown> = Object.freeze({});

/** Reactive collection facade over the local admin mock store. */
export function useAdminData<K extends AdminCollection>(
  collection: K,
  query: AdminQuery<K> = EMPTY_QUERY as AdminQuery<K>,
) {
  const subscribe = useCallback(
    (listener: () => void) => subscribeAdminData(collection, listener),
    [collection],
  );
  const getSnapshot = useCallback(() => getAdminDataSnapshot(collection), [collection]);
  const rows = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const data = useMemo(() => selectAdminData(rows, query), [rows, query]);

  const create = useCallback(
    (input: AdminRecordInput<K>) => createAdminRecord(collection, input),
    [collection],
  );
  const update = useCallback(
    (id: string, patch: AdminRecordPatch<K>) => updateAdminRecord(collection, id, patch),
    [collection],
  );
  const remove = useCallback((id: string) => deleteAdminRecord(collection, id), [collection]);
  const bulk = useCallback(
    (ids: readonly string[], action: AdminBulkAction<K>): AdminBulkResult<AdminEntity<K>> =>
      bulkAdminAction(collection, ids, action),
    [collection],
  );
  const exportData = useCallback(
    (format: AdminExportFormat, selectedRows: readonly AdminEntity<K>[] = data, columns?: readonly (keyof AdminEntity<K> & string)[]) =>
      exportAdminData(collection, format, selectedRows, columns),
    [collection, data],
  );
  const download = useCallback(
    (format: AdminExportFormat = 'csv', selectedRows: readonly AdminEntity<K>[] = data, columns?: readonly (keyof AdminEntity<K> & string)[]) =>
      downloadAdminData(collection, format, selectedRows, columns),
    [collection, data],
  );
  const list = useCallback(
    (nextQuery: AdminQuery<K> = {}) => listAdminData(collection, nextQuery),
    [collection],
  );
  const restore = useCallback(
    (before: readonly AdminEntity<K>[]) => restoreAdminSnapshot(collection, before),
    [collection],
  );

  return { data, allData: rows, create, update, remove, bulk, restore, exportData, download, list };
}
