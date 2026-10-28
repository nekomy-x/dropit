/**
 * SourceChip — the honesty marker required by §5.1 and 01-contract.md §2.
 *
 * Renders the word (`live`, `preview`, `unavailable`) in a tinted chip, so
 * status never depends on colour alone (§4.5, §5.3). The 5px dot that used to
 * sit before the word was a second copy of the same fact and is gone. The
 * parent Metric stamps `data-source` from the same value, which is what the
 * acceptance script reads.
 */
import { useLocale } from '../../locale';

export type MetricSource = 'live' | 'preview' | 'unavailable';

const LABEL_KEYS: Record<MetricSource, string> = {
  live: 'source.live',
  preview: 'source.preview',
  unavailable: 'source.unavailable',
};

const TITLE_KEYS: Record<MetricSource, string> = {
  live: 'source.liveTitle',
  preview: 'source.previewTitle',
  unavailable: 'source.unavailableTitle',
};

export interface SourceChipProps {
  source: MetricSource;
  /** Override the visible word (kept as a full word, not an icon). */
  label?: string;
}

export default function SourceChip({ source, label }: SourceChipProps) {
  const { t } = useLocale();
  return (
    <span
      className={`club-source-chip club-source-${source}`}
      data-source-chip={source}
      title={t(TITLE_KEYS[source])}
    >
      {label ?? t(LABEL_KEYS[source])}
    </span>
  );
}
