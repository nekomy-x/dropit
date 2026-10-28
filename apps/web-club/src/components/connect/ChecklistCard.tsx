import Icon, { type IconName } from './Icons';

export interface ChecklistItem {
  label: string;
  meta?: string;
  done: boolean;
}

/**
 * Right-hand card of the wizard: live progress of the current step.
 * Every row animates from dim → mint as the matching field is filled.
 */
export default function ChecklistCard({
  title,
  icon = 'doc',
  items,
  note,
}: {
  title: string;
  icon?: IconName;
  items: ChecklistItem[];
  note?: string;
}) {
  const done = items.filter((item) => item.done).length;
  const progress = items.length ? Math.round((done / items.length) * 100) : 0;

  return (
    <aside className="w-full shrink-0 rounded-card border border-drop-line bg-drop-surface p-6 lg:w-[380px]">
      <header className="flex items-center gap-2.5">
        <span className="text-drop-accent">
          <Icon name={icon} size={16} />
        </span>
        <h2 className="connect-mono-label flex-1 text-drop-muted">{title}</h2>
        <span className="font-mono text-[10px] text-drop-dim tabular">
          {done}/{items.length}
        </span>
      </header>

      {/* Progress rail — fills as the step gets completed. */}
      <div className="mt-4 h-px w-full overflow-hidden bg-drop-line">
        <div
          className="h-full bg-drop-accent transition-[width] duration-500 ease-out"
          style={{ width: `${progress}%` }}
        />
      </div>

      <ul className="mt-4 flex flex-col">
        {items.map((item, index) => (
          <li
            key={item.label}
            className={`flex items-start gap-3 py-3.5 ${
              index < items.length - 1 ? 'border-b border-drop-line/60' : ''
            }`}
          >
            <span
              className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition-all duration-300 ${
                item.done
                  ? 'border-drop-accent bg-drop-accent text-drop-ink'
                  : 'border-drop-line text-transparent'
              }`}
            >
              <Icon name="check" size={9} strokeWidth={3} />
            </span>
            <div className="min-w-0">
              <p
                className={`text-[13px] font-semibold transition-colors duration-300 ${
                  item.done ? 'text-drop-text' : 'text-drop-muted'
                }`}
              >
                {item.label}
              </p>
              {item.meta && <p className="mt-1 text-[11px] text-drop-dim">{item.meta}</p>}
            </div>
          </li>
        ))}
      </ul>

      {note && (
        <p className="mt-4 flex items-start gap-2.5 border-t border-drop-line/60 pt-4 text-[11px] leading-relaxed text-drop-muted">
          <span className="mt-0.5 shrink-0 text-drop-dim">
            <Icon name="info" size={14} />
          </span>
          {note}
        </p>
      )}
    </aside>
  );
}
