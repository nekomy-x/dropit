/**
 * Barrel for the club panel primitives.
 *
 * Pages import from here (`../components/club`) and get the markup contract for
 * free: every primitive stamps its own `data-surface` / `data-control` /
 * `data-metric` / `data-source` attributes. See 01-contract.md §2.
 */

import './club-primitives.css';

export { default as Panel } from './Panel';
export type { PanelProps } from './Panel';

export { default as Tile } from './Tile';
export type { TileProps } from './Tile';

export { default as Metric } from './Metric';
export type { MetricProps } from './Metric';

export { default as SourceChip } from './SourceChip';
export type { SourceChipProps, MetricSource } from './SourceChip';

export { default as PreviewBanner } from './PreviewBanner';
export type { PreviewBannerProps } from './PreviewBanner';

export { default as Chip } from './Chip';
export type { ChipProps, ChipTone } from './Chip';

export { default as Segmented } from './Segmented';
export type { SegmentedProps, SegmentedOption } from './Segmented';

export { default as ListRow } from './ListRow';
export type { ListRowProps } from './ListRow';

export { default as Meter } from './Meter';
export type { MeterProps } from './Meter';

export { default as ToastRegion, useToasts } from './Toast';
export type { ToastInput, ToastTone, ToastRegionProps } from './Toast';

export { default as Skeleton, SkeletonText, SkeletonMetric } from './Skeleton';
export type { SkeletonProps } from './Skeleton';

export { default as EmptyState } from './EmptyState';
export type { EmptyStateProps } from './EmptyState';

export { default as ErrorState } from './ErrorState';
export type { ErrorStateProps } from './ErrorState';

export const CLUB_PRIMITIVE_CONTRACT = {
  surface: ['panel', 'tile', 'dialog'],
  control: ['button', 'field', 'segment', 'icon'],
  source: ['live', 'preview', 'unavailable'],
} as const;
