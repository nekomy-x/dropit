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
/**
 * Cards are tall, so a phone list renders a page of them instead of the whole
 * filtered set: 123 records as 123 cards made the page 37,000px long and the
 * bottom of the screen's own content unreachable. "Show more" appends the next
 * page; the toolbar counter keeps reporting the real filtered total.
 */
const MOBILE_PAGE_SIZE = 20;

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
  /**
   * A phone cannot show an eight-column grid: below 768px the same rows render
   * as record cards. The query is read in an effect, never during render, so the
   * component stays renderable on the server and in tests.
   */
  const [compact, setCompact] = useState(false);
  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return undefined;
    const query = window.matchMedia('(max-width: 767px)');
    const sync = () => setCompact(query.matches);
    sync();
    query.addEventListener('change', sync);
    return () => query.removeEventListener('change', sync);
  }, []);
  /** How many cards the phone list shows; grows by one page at a time. */
  const [mobileCount, setMobileCount] = useState(MOBILE_PAGE_SIZE);
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
  /** The page of cards the phone list renders, and the rows it has not reached. */
  const mobileEntries = useMemo(() => displayedEntries.slice(0, mobileCount), [displayedEntries, mobileCount]);
  const mobileRemaining = displayedEntries.length - mobileEntries.length;
  const mobileTotal = displayedEntries.length;
  const mobileIds = useMemo(() => mobileEntries.map(({ row, index }) => resolveRowId(row, index)), [mobileEntries, resolveRowId]);
  const allVisibleSelected = visibleIds.length > 0 && visibleIds.every((id) => selected.has(id));
  const someVisibleSelected = visibleIds.some((id) => selected.has(id)) && !allVisibleSelected;
  /* The phone select-all speaks for the cards it can see, the desktop one for
   * every filtered row it virtualizes. */
  const allPageSelected = mobileIds.length > 0 && mobileIds.every((id) => selected.has(id));
  const somePageSelected = mobileIds.some((id) => selected.has(id)) && !allPageSelected;
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
    if (selectAllRef.current) selectAllRef.current.indeterminate = compact ? somePageSelected : someVisibleSelected;
  }, [compact, somePageSelected, someVisibleSelected]);

  /** A new filter or sort starts the phone list from its first page again. */
  useEffect(() => {
    setMobileCount(MOBILE_PAGE_SIZE);
  }, [search, status, sorting, rows.length]);

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

  /**
   * The one inline editor. The desktop cell and the mobile card row both open
   * it, so a column's `edit` renderer cannot drift between the two layouts.
   */
  const renderInlineEditor = (column: PowerTableColumn<T>, row: T, value: unknown) => (
    <div className="power-table-editor" onClick={(event) => event.stopPropagation()}>
      {column.edit?.(value as T[keyof T], row, (next) => setEditing((current) => (current ? { ...current, value: next } : current))) ?? (
        <input
          autoFocus
          className="field-input power-table-editor-input"
          value={displayValue(value)}
          onChange={(event) => setEditing((current) => (current ? { ...current, value: event.target.value } : current))}
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
  );

  const renderCellValue = (column: PowerTableColumn<T>, row: T) => {
    const value = getField(row, column.key) as T[keyof T];
    return column.render ? column.render(value, row) : displayValue(value) || '—';
  };

    const renderEditButton = (column: PowerTableColumn<T>, row: T, index: number) => (
    <button
      type="button"
      className="btn-quiet btn-sm power-table-edit"
      onClick={(event) => { event.stopPropagation(); beginEdit(row, index, column.key); }}
      aria-label={t('table.editColumn', { column: column.key })}
    >
      {t('table.edit')}
    </button>
  );

  /**
   * The visible name of a per-column edit trigger: "Edit E-mail" when the
   * column header is a translated string, otherwise the plain "Edit" verb.
   * Desktop keeps its compact "Edit" per cell; a card shows the full name
   * because a column of identical links says nothing.
   */
  const editLabel = (column: PowerTableColumn<T>) => (
    typeof column.header === 'string' ? t('table.editColumn', { column: column.header }) : t('table.edit')
  );

  /**
   * Below 768px a row becomes a record card: the identity column on top (bold,
   * chips included), then one `label / value` line per remaining column, then
   * one action row with the inline editors. Nothing is dropped, no column is
   * clipped, and the desktop table below is untouched.
   */
  const renderCards = () => {
    const [primaryColumn, ...detailColumns] = columns;
    const cardColumns = primaryColumn ? [primaryColumn, ...detailColumns] : detailColumns;
    const editableColumns = onInlineEdit ? cardColumns.filter((column) => column.editable) : [];
    const togglePage = () => {
      const next = new Set(selected);
      if (allPageSelected) mobileIds.forEach((id) => next.delete(id));
      else mobileIds.forEach((id) => next.add(id));
      setSelected(next);
      notifySelection(next);
    };
    return (
      <>
        {selection && mobileEntries.length > 0 ? (
          <div className="power-table-mobile-selectall">
            <input
              ref={selectAllRef}
              aria-label={t('table.selectAll')}
              type="checkbox"
              checked={allPageSelected}
              onChange={togglePage}
            />
            <span aria-hidden="true">{t('table.selectAll')}</span>
            {somePageSelected ? <span className="sr-only">{t('table.selected', { count: selectedRows.length })}</span> : null}
          </div>
        ) : null}
        <ul className="power-table-mobile">
          {mobileEntries.map(({ row, index }) => {
            const id = resolveRowId(row, index);
            const isSelected = selected.has(id);
            const primaryEditing = primaryColumn !== undefined && editing?.id === id && editing.key === primaryColumn.key;
            return (
              <li
                key={id}
                className={['power-table-mobile-card', isSelected ? 'is-selected' : ''].filter(Boolean).join(' ')}
                style={{ cursor: onRowOpen ? 'pointer' : undefined }}
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
                <div className="power-table-mobile-head">
                  {selection ? (
                    <input
                      className="power-table-mobile-check"
                      aria-label={t('table.selectRow', { id })}
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => toggleRow(row, index)}
                    />
                  ) : null}
                  <div className="power-table-mobile-identity">
                    {primaryColumn && editing && primaryEditing
                      ? renderInlineEditor(primaryColumn, row, editing.value)
                      : primaryColumn ? (
                        <span className="power-table-mobile-title">{renderCellValue(primaryColumn, row)}</span>
                      ) : null}
                  </div>
                </div>

                {detailColumns.length ? (
                  <dl className="power-table-mobile-meta">
                    {detailColumns.map((column) => {
                      const isEditing = editing !== null && editing.id === id && editing.key === column.key;
                      return (
                        <div key={column.key} className={['power-table-mobile-row', isEditing ? 'is-editing' : ''].filter(Boolean).join(' ')}>
                          <dt>{column.header}</dt>
                          <dd>
                            {editing && isEditing
                              ? renderInlineEditor(column, row, editing.value)
                              : <span className="power-table-mobile-value">{renderCellValue(column, row)}</span>}
                          </dd>
                        </div>
                      );
                    })}
                  </dl>
                ) : null}

                {editableColumns.length ? (
                  <div className="power-table-mobile-actions">
                    {editableColumns.map((column) => (
                      <button
                        key={column.key}
                        type="button"
                        className="btn btn-secondary btn-sm"
                        onClick={(event) => { event.stopPropagation(); beginEdit(row, index, column.key); }}
                        aria-label={editLabel(column)}
                      >
                        {editLabel(column)}
                      </button>
                    ))}
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
        {mobileRemaining > 0 ? (
          <div className="power-table-mobile-more">
            <span className="power-table-mobile-more-count" aria-live="polite">
              {mobileEntries.length} / {mobileTotal}
            </span>
            <button
              type="button"
              className="btn btn-secondary btn-sm"
              onClick={() => setMobileCount((count) => count + MOBILE_PAGE_SIZE)}
            >
              {t('table.showMore')}
            </button>
          </div>
        ) : null}
      </>
    );
  };

  return (
    <section className={`panel-card ${className}`.trim()}>
      <div className="power-table-toolbar">
        <label className="power-table-search">
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
        style={compact ? undefined : { maxHeight: VIEWPORT_HEIGHT, overflow: 'auto' }}
        onScroll={compact ? undefined : (event) => setScrollTop(event.currentTarget.scrollTop)}
      >
        {compact ? renderCards() : (
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
                    const isEditing = editing !== null && editing.id === id && editing.key === column.key;
                    return (
                      <td key={column.key} className={column.className}>
                        {editing && isEditing ? renderInlineEditor(column, row, editing.value) : (
                          <div style={{ display: 'flex', minWidth: 0, alignItems: 'center', gap: 8 }}>
                            <span style={{ minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                              {renderCellValue(column, row)}
                            </span>
                            {column.editable && onInlineEdit ? renderEditButton(column, row, index) : null}
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
        )}
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
