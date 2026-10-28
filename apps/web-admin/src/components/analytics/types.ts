export type AnalyticsPeriod = '24h' | '7d' | '30d';

export type AnalyticsTone = 'neutral' | 'positive' | 'warning' | 'critical';

export type HealthStatus = 'healthy' | 'degraded' | 'outage' | 'unknown';

export interface AnalyticsPoint {
  label: string;
  value: number;
  detail?: string;
}

export interface AnalyticsKpi {
  label: string;
  value: string;
  change?: string;
  changeLabel?: string;
  trend?: number[];
  tone?: AnalyticsTone;
  description?: string;
}

export interface HealthIndicator {
  name: string;
  status: HealthStatus;
  detail: string;
  latency?: string;
  updatedAt?: string;
}

export type SessionStatus = 'live' | 'starting' | 'paused';

export interface ActiveSession {
  id: string;
  venue: string;
  city?: string;
  dj: string;
  requests: number;
  startedAt: string;
  status: SessionStatus;
  guests: number;
  signal: number;
}
