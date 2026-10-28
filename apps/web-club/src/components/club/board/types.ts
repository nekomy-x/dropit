/**
 * The shape every figure on the board arrives in.
 *
 * Structurally identical to `BoardMetric<T>` in 01-contract.md §7, declared
 * here so the presentational panels do not import the data layer: the page is
 * the only place that knows about `useClubBoard()`.
 */
import type { MetricSource } from '../SourceChip';

export interface BoardMetricValue<T> {
  value: T | null;
  source: MetricSource;
  caption: string;
}
