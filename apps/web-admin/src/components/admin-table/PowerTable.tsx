import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import Select from '../ui/Select';
import TrashAction from '../ui/TrashAction';
import { useLocale } from '../../locale';
import { STATUS_MESSAGE_KEY } from '../../messages/status';

export type PowerTableRow = object;
export type PowerTableSortDirection = 'asc' | 'desc';

export interface PowerTableSort<T extends PowerTableRow> {
  key: keyof T & string;
  direction: PowerTableSortDirection;
}

export interface PowerTableColumn<T extends PowerTableRow> {
  key: keyof T & string;
  header: ReactNode;
  sortable?: boolean;
  searchable?: boolean;
  className?: string;
  headerClassName?: string;
  /** Preferred column width (CSS length). The primary column gets the most room. */
  width?: string;
  render?: (value: T[keyof T], row: T) => ReactNode;
  editable?: boolean;
  /** Optional editor, including a custom select. The returned value is saved by Save. */
  edit?: (value: T[keyof T], row: T, onChange: (value: unknown) => void) => ReactNode;
}

export interface PowerTableBulkAction<T extends PowerTableRow> {
  label: string;
  onSelect: (rows: T[]) => void | Promise<void>;
  disabled?: boolean;
  /**
   * Marks the one bullet in a bulk bar that destroys data. It renders as the
   * shared `TrashAction` — a bin that opens on approach — instead of a neutral
   * button, so "Delete" is never read as just another verb in the row of them.
   */
  destructive?: boolean;
}

export interface PowerTableProps<T extends PowerTableRow> {
  rows: readonly T[];
  columns: readonly PowerTableColumn<T>[];
  getRowId?: (row: T, index: number) => string;
  selection?: boolean;
  onSelectionChange?: (ids: string[], rows: T[]) => void;
  bulkActions?: readonly PowerTableBulkAction<T>[];
  search?: string;
  onSearchChange?: (search: string) => void;
  searchPlaceholder?: string;
  statusField?: keyof T & string;
  statusOptions?: readonly string[];
  status?: string;
  onStatusChange?: (status: string) => void;
  initialSort?: readonly PowerTableSort<T>[];
  sort?: readonly PowerTableSort<T>[];
  onSortChange?: (sort: PowerTableSort<T>[]) => void;
  onInlineEdit?: (row: T, key: keyof T & string, value: unknown) => void | Promise<void>;
  /** Called when a non-interactive part of a row is opened. */
  onRowOpen?: (row: T) => void;
  /** Receives the current filtered and sorted view, useful for export. */
  onViewChange?: (rows: T[]) => void;
  stickyHeader?: boolean;
  emptyState?: ReactNode;
  className?: string;
  /** What the rows are, for the "N of M" counter and the toolbar label. */
  recordNoun?: string;
}

const ROW_HEIGHT = 64;
const VIEWPORT_HEIGHT = 560;
const OVERSCAN_ROWS = 5;

function displayValue(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

function getField<T extends object>(row: T, key: keyof T & string): unknown {
  return (row as unknown as Record<string, unknown>)[key];
}

function compare(a: unknown, b: unknown): number {
  if (a === b) return 0;
  if (a === null || a === undefined) return 1;
  if (b === null || b === undefined) return -1;
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  return displayValue(a).localeCompare(displayValue(b), undefined, {
    numeric: true,
    sensitivity: 'base',
  });
}

function isInteractiveTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && Boolean(target.closest('button, input, select, textarea, a, [role="button"]'));
}

/**
 * Dense admin table with local filtering, virtualized rows, and operator-safe
 * actions.
 *
 * Presentation is the shared `panel-card` + `data-table` pair: one surface, one
 * hairline per row, no border on every edge, and no card nested in the panel.
 * Every control keeps a ≥36px target (44px on touch) and the focus ring comes
 * from panel.css, so the table is operable by keyboard in both themes.
 */
export default function PowerTable<T extends PowerTableRow>({
  rows,
  columns,
  getRowId,
  selection = false,
  onSelectionChange,
  bulkActions = [],
  search: controlledSearch,
  onSearchChange,
  searchPlaceholder,
  statusField,
  statusOptions = [],
  status: controlledStatus,
  onStatusChange,
  initialSort = [],
  sort: controlledSort,
  onSortChange,
  onInlineEdit,
  onRowOpen,
  onViewChange,
  stickyHeader = true,
  emptyState,
  className = '',
  recordNoun,
}: PowerTableProps<T>) {
  const { t } = useLocale();
  const noun = recordNoun ?? t('table.recordNoun');
  const searchHint = searchPlaceholder ?? t('table.searchPlaceholder');
  const emptyCopy = emptyState ?? t('table.emptyState');
  const [searchState, setSearchState] = useState('');
  const [statusState, setStatusState] = useState('');
  const [sortState, setSortState] = useState<PowerTableSort<T>[]>(() => [...initialSort]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [scrollTop, setScrollTop] = useState(0);
  const [editing, setEditing] = useState<{ id: string; key: keyof T & string; value: unknown } | null>(null);
  const [saving, setSaving] = useState(false);
  const [editError, setEditError] = useState<string | null>(null);
  const selectAllRef = useRef<HTMLInputElement>(null);
  const search = controlledSearch ?? searchState;
  const status = controlledStatus ?? statusState;
  const sorting = controlledSort ?? sortState;
  /** A stable row identity: the caller's resolver when provided, otherwise `row.id`. */
  const resolveRowId = useCallback(
    (row: T, index: number) => (getRowId ? getRowId(row, index) : displayValue('id' in row ? row.id : undefined) || String(index)),
    [getRowId],
  );
  const rowIds = useMemo(() => rows.map(resolveRowId), [rows, resolveRowId]);

  const filteredRows = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    const searchableColumns = columns.filter((column) => column.searchable !== false);
    return rows.filter((row) => {
      if (query && !searchableColumns.some((column) => displayValue(getField(row, column.key)).toLocaleLowerCase().includes(query))) {
        return false;
      }
      if (status && statusField && displayValue(getField(row, statusField)) !== status) return false;
      return true;
    });
  }, [columns, rows, search, status, statusField]);

  const displayedEntries = useMemo(() => {
    const entries = filteredRows.map((row) => ({ row, index: rows.indexOf(row) }));
    if (!sorting.length) return entries;
    return entries.sort((a, b) => {
      for (const rule of sorting) {
        const result = compare(getField(a.row, rule.key), getField(b.row, rule.key));
        if (result !== 0) return rule.direction === 'asc' ? result : -result;
      }
      return a.index - b.index;
    });
  }, [filteredRows, rows, sorting]);

  const displayedRows = useMemo(() => displayedEntries.map(({ row }) => row), [displayedEntries]);

  /** Notifies the caller of the current view. Kept in a ref so an inline callback cannot retrigger the effect. */
  const viewChangeRef = useRef(onViewChange);
  viewChangeRef.current = onViewChange;
  useEffect(() => {
    viewChangeRef.current?.(displayedRows);
  }, [displayedRows]);

  useEffect(() => {
    if (selected.size === 0) return;
    const visible = new Set(rowIds);
    setSelected((current) => {
      const next = new Set([...current].filter((id) => visible.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [rowIds, selected.size]);

  const selectedRows = useMemo(
    () => rows.filter((row, index) => selected.has(resolveRowId(row, index))),
    [resolveRowId, rows, selected],
  );
  const visibleIds = useMemo(() => displayedEntries.map(({ row, index }) => resolveRowId(row, index)), [displayedEntries, resolveRowId]);
  const allVisibleSelected = visibleIds.length > 0 && visibleIds.every((id) => selected.has(id));
  const someVisibleSelected = visibleIds.some((id) => selected.has(id)) && !allVisibleSelected;
  const firstVisible = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_ROWS);
  const lastVisible = Math.min(
    displayedEntries.length,
    firstVisible + Math.ceil(VIEWPORT_HEIGHT / ROW_HEIGHT) + OVERSCAN_ROWS * 2,
  );
  const renderedEntries = displayedEntries.slice(firstVisible, lastVisible);
  const topSpacerHeight = firstVisible * ROW_HEIGHT;
  const bottomSpacerHeight = Math.max(0, (displayedEntries.length - lastVisible) * ROW_HEIGHT);
  const columnCount = columns.length + (selection ? 1 : 0);

  useEffect(() => {
    if (selectAllRef.current) selectAllRef.current.indeterminate = someVisibleSelected;
  }, [someVisibleSelected]);

  const setSearch = (value: string) => {
    setSearchState(value);
    onSearchChange?.(value);
  };
  const setStatus = (value: string) => {
    setStatusState(value);
    onStatusChange?.(value);
  };
  const emitSort = (next: PowerTableSort<T>[]) => {
    setSortState(next);
    onSortChange?.(next);
  };
  const toggleSort = (key: keyof T & string, additive: boolean) => {
    const existing = sorting.find((rule) => rule.key === key);
    const nextDirection: PowerTableSortDirection | null = existing?.direction === 'asc'
      ? 'desc'
      : existing?.direction === 'desc' ? null : 'asc';
    const withoutKey = sorting.filter((rule) => rule.key !== key);
    emitSort(nextDirection ? [...(additive ? withoutKey : []), { key, direction: nextDirection }] : withoutKey);
  };
  const notifySelection = (next: Set<string>) => {
    onSelectionChange?.([...next], rows.filter((row, index) => next.has(resolveRowId(row, index))));
  };
  const toggleRow = (row: T, index: number) => {
    const id = resolveRowId(row, index);
    const next = new Set(selected);
    if (next.has(id)) next.delete(id); else next.add(id);
    setSelected(next);
    notifySelection(next);
  };
  const toggleAll = () => {
    const next = new Set(selected);
    if (allVisibleSelected) visibleIds.forEach((id) => next.delete(id));
    else visibleIds.forEach((id) => next.add(id));
    setSelected(next);
    notifySelection(next);
  };
  const beginEdit = (row: T, index: number, key: keyof T & string) => {
    setEditError(null);
    setEditing({ id: resolveRowId(row, index), key, value: getField(row, key) });
  };
  const cancelEdit = () => {
    setEditError(null);
    setEditing(null);
  };
  const commitEdit = async () => {
    if (!editing || saving) return;
    const row = rows.find((item, index) => resolveRowId(item, index) === editing.id);
    if (!row || !onInlineEdit) return;
    setSaving(true);
    setEditError(null);
    try {
      await onInlineEdit(row, editing.key, editing.value);
      setEditing(null);
    } catch (error) {
      setEditError(error instanceof Error ? error.message : t('table.saveError'));
    } finally {
      setSaving(false);
    }
  };
  const handleEditKey = (event: KeyboardEvent<HTMLInputElement | HTMLSelectElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      void commitEdit();
    }
    if (event.key === 'Escape') {
      event.preventDefault();
      cancelEdit();
    }
  };

  return (
    <section className={`panel-card ${className}`.trim()}>
      <div className="power-table-toolbar">
        <label style={{ flex: 1, minWidth: 220 }}>
          <span className="sr-only">{searchHint}</span>
          <input className="field-input" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={searchHint} />
        </label>
        {statusField && statusOptions.length > 0 ? (
          <Select
            className="power-table-status"
            ariaLabel={t('table.filterByStatus')}
            value={status}
            onChange={setStatus}
            placeholder={t('table.allStatuses')}
            options={[
              { value: '', label: t('table.allStatuses') },
              ...statusOptions.map((option) => {
                const key = STATUS_MESSAGE_KEY[option];
                return { value: option, label: key ? t(key) : option.replaceAll('_', ' ') };
              }),
            ]}
          />
        ) : null}
        <span
          className="chip chip-neutral"
          aria-live="polite"
          title={t('table.range', { shown: displayedEntries.length, total: rows.length, noun })}
        >
          {displayedEntries.length} / {rows.length}
        </span>
      </div>

      <div
        className="power-table-scroll"
        style={{ maxHeight: VIEWPORT_HEIGHT, overflow: 'auto' }}
        onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
      >
        <table className="data-table" style={{ width: '100%', minWidth: 720, tableLayout: 'auto' }}>
          <thead className={stickyHeader ? 'power-table-head is-sticky' : 'power-table-head'}>
            <tr>
              {selection ? (
                <th scope="col" style={{ width: 44 }}>
                  <input
                    ref={selectAllRef}
                    aria-label={t('table.selectAll')}
                    type="checkbox"
                    checked={allVisibleSelected}
                    onChange={toggleAll}
                  />
                </th>
              ) : null}
              {columns.map((column) => {
                const activeSort = sorting.find((rule) => rule.key === column.key);
                return (
                  <th key={column.key} scope="col" className={column.headerClassName} style={column.width ? { width: column.width } : undefined}>
                    {column.sortable === false ? column.header : (
                      <button
                        type="button"
                        onClick={(event) => toggleSort(column.key, event.shiftKey)}
                        title={t('table.multiSort')}
                        className="table-sort"
                      >
                        {column.header}
                        <span aria-hidden="true" className="table-sort-mark" style={{ color: activeSort ? 'var(--c-accent-quiet)' : 'var(--c-text-muted-sm)' }}>
                          {activeSort ? `${activeSort.direction === 'asc' ? '↑' : '↓'}${sorting.indexOf(activeSort) + 1}` : '↕'}
                        </span>
                      </button>
                    )}
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {topSpacerHeight > 0 ? <tr aria-hidden="true"><td colSpan={columnCount} style={{ height: topSpacerHeight, padding: 0 }} /></tr> : null}
            {renderedEntries.map(({ row, index }) => {
              const id = resolveRowId(row, index);
              const isSelected = selected.has(id);
              return (
                <tr
                  key={id}
                  className={[isSelected ? 'is-selected' : '', editing?.id === id ? 'is-editing' : ''].filter(Boolean).join(' ') || undefined}
                  style={{ height: ROW_HEIGHT, cursor: onRowOpen ? 'pointer' : undefined }}
                  onClick={(event) => {
                    if (!isInteractiveTarget(event.target)) onRowOpen?.(row);
                  }}
                  onKeyDown={(event) => {
                    if ((event.key === 'Enter' || event.key === ' ') && !isInteractiveTarget(event.target)) {
                      event.preventDefault();
                      onRowOpen?.(row);
                    }
                  }}
                  tabIndex={onRowOpen ? 0 : undefined}
                >
                  {selection ? (
                    <td style={{ width: 44 }}>
                      <input
                        aria-label={t('table.selectRow', { id })}
                        type="checkbox"
                        checked={isSelected}
                        onChange={() => toggleRow(row, index)}
                      />
                    </td>
                  ) : null}
                  {columns.map((column) => {
                    const value = getField(row, column.key) as T[keyof T];
                    const isEditing = editing?.id === id && editing.key === column.key;
                    return (
                      <td key={column.key} className={column.className}>
                        {isEditing ? (
                          <div className="power-table-editor" onClick={(event) => event.stopPropagation()}>
                            {column.edit?.(editing.value as T[keyof T], row, (next) => setEditing({ ...editing, value: next })) ?? (
                              <input
                                autoFocus
                                className="field-input"
                                style={{ height: 30, minWidth: 0, flex: 1 }}
                                value={displayValue(editing.value)}
                                onChange={(event) => setEditing({ ...editing, value: event.target.value })}
                                onKeyDown={handleEditKey}
                                aria-label={t('table.editColumn', { column: column.key })}
                              />
                            )}
                            <button type="button" className="btn btn-primary btn-sm" disabled={saving} onClick={() => void commitEdit()}>
                              {saving ? t('common.saving') : t('common.save')}
                            </button>
                            <button type="button" className="btn btn-secondary btn-sm" disabled={saving} onClick={cancelEdit}>{t('common.cancel')}</button>
                            {editError ? <span className="field-error" role="alert">{editError}</span> : null}
                          </div>
                        ) : (
                          <div style={{ display: 'flex', minWidth: 0, alignItems: 'center', gap: 8 }}>
                            <span style={{ minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                              {column.render ? column.render(value, row) : displayValue(value) || '—'}
                            </span>
                            {column.editable && onInlineEdit ? (
                              <button
                                type="button"
                                className="btn-quiet btn-sm power-table-edit"
                                onClick={(event) => { event.stopPropagation(); beginEdit(row, index, column.key); }}
                                aria-label={t('table.editColumn', { column: column.key })}
                              >
                                Edit
                              </button>
                            ) : null}
                          </div>
                        )}
                      </td>
                    );
                  })}
                </tr>
              );
            })}
            {bottomSpacerHeight > 0 ? <tr aria-hidden="true"><td colSpan={columnCount} style={{ height: bottomSpacerHeight, padding: 0 }} /></tr> : null}
          </tbody>
        </table>
        {displayedEntries.length === 0 ? <div className="panel-empty" role="status">{emptyCopy}</div> : null}
      </div>

      {selection && selectedRows.length > 0 ? (
        <div className="power-table-bulk" role="toolbar" aria-label={t('table.bulkActions')}>
          <span className="chip chip-accent">{t('table.selected', { count: selectedRows.length })}</span>
          {bulkActions.map((action) => (
            action.destructive ? (
              <TrashAction
                key={action.label}
                label={action.label}
                detail={t('table.selected', { count: selectedRows.length })}
                disabled={action.disabled}
                onClick={(event) => { event.stopPropagation(); void action.onSelect(selectedRows); }}
              />
            ) : (
              <button
                key={action.label}
                type="button"
                className="btn btn-secondary btn-sm"
                disabled={action.disabled}
                onClick={(event) => { event.stopPropagation(); void action.onSelect(selectedRows); }}
              >
                {action.label}
              </button>
            )
          ))}
        </div>
      ) : null}
    </section>
  );
}
