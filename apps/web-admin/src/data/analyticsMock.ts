import type { ActiveSession, AnalyticsKpi, AnalyticsPeriod, AnalyticsPoint, HealthIndicator } from '../components/analytics';

export const analyticsMockKpis: Readonly<Record<AnalyticsPeriod, ReadonlyArray<AnalyticsKpi>>> = {
  '24h': [
    { label: 'Active Clubs', value: '18', change: '+12.5%', changeLabel: 'vs previous day', trend: [9, 11, 10, 13, 15, 14, 18], tone: 'positive', description: 'Venues with a live room' },
    { label: 'Online DJs', value: '26', change: '+8.3%', changeLabel: 'vs previous day', trend: [16, 18, 17, 20, 23, 24, 26], tone: 'positive', description: 'Currently connected' },
    { label: 'Total Requests', value: '2,481', change: '+8.4%', changeLabel: 'vs previous day', trend: [240, 300, 282, 340, 366, 405, 481], tone: 'positive' },
    { label: 'Revenue / Tips', value: '€18,420', change: '+11.2%', changeLabel: 'vs previous day', trend: [1220, 1560, 1480, 2020, 2650, 3310, 4180], tone: 'positive', description: 'Captured payments' },
    { label: 'Server Load', value: '42%', change: '-6 pp', changeLabel: 'vs previous day', trend: [68, 62, 65, 57, 54, 48, 42], tone: 'positive', description: 'Current CPU utilization' },
  ],
  '7d': [
    { label: 'Active Clubs', value: '22', change: '+9.1%', changeLabel: 'vs previous week', trend: [16, 18, 17, 20, 19, 21, 22], tone: 'positive' },
    { label: 'Online DJs', value: '31', change: '+10.7%', changeLabel: 'vs previous week', trend: [21, 22, 24, 26, 28, 29, 31], tone: 'positive' },
    { label: 'Total Requests', value: '14,902', change: '+14.2%', changeLabel: 'vs previous week', trend: [1980, 2050, 2220, 2310, 2020, 2150, 2172], tone: 'positive' },
    { label: 'Revenue / Tips', value: '€112,680', change: '+17.5%', changeLabel: 'vs previous week', trend: [11400, 13600, 14500, 15220, 16300, 17800, 19220], tone: 'positive' },
    { label: 'Server Load', value: '48%', change: '-3 pp', changeLabel: 'vs previous week', trend: [55, 54, 52, 51, 49, 50, 48], tone: 'positive' },
  ],
  '30d': [
    { label: 'Active Clubs', value: '24', change: '+18.0%', changeLabel: 'vs previous month', trend: [19, 18, 20, 21, 20, 23, 24], tone: 'positive' },
    { label: 'Online DJs', value: '34', change: '+13.3%', changeLabel: 'vs previous month', trend: [27, 28, 29, 31, 30, 33, 34], tone: 'positive' },
    { label: 'Total Requests', value: '61,480', change: '+21.7%', changeLabel: 'vs previous month', trend: [1800, 1900, 2050, 2140, 2300, 2400, 2500], tone: 'positive' },
    { label: 'Revenue / Tips', value: '€482,910', change: '+19.8%', changeLabel: 'vs previous month', trend: [12000, 13200, 14800, 15400, 17300, 18600, 20800], tone: 'positive' },
    { label: 'Server Load', value: '46%', change: '-4 pp', changeLabel: 'vs previous month', trend: [57, 55, 54, 52, 51, 49, 46], tone: 'positive' },
  ],
};

export const analyticsMockSeries: Readonly<Record<AnalyticsPeriod, ReadonlyArray<AnalyticsPoint>>> = {
  '24h': [
    { label: '00:00', value: 98 }, { label: '04:00', value: 124 }, { label: '08:00', value: 116 }, { label: '12:00', value: 218 }, { label: '16:00', value: 304 }, { label: '20:00', value: 402 }, { label: 'Now', value: 481 },
  ],
  '7d': [
    { label: 'Mon', value: 1800 }, { label: 'Tue', value: 2050 }, { label: 'Wed', value: 2220 }, { label: 'Thu', value: 2310 }, { label: 'Fri', value: 2020 }, { label: 'Sat', value: 2150 }, { label: 'Sun', value: 2172 },
  ],
  '30d': [
    { label: 'W1', value: 12400 }, { label: 'W2', value: 14020 }, { label: 'W3', value: 15840 }, { label: 'W4', value: 19220 },
  ],
};

export const analyticsMockRevenueSeries: Readonly<Record<AnalyticsPeriod, ReadonlyArray<AnalyticsPoint>>> = {
  '24h': [
    { label: '00:00', value: 1220 }, { label: '04:00', value: 1560 }, { label: '08:00', value: 1480 }, { label: '12:00', value: 2020 }, { label: '16:00', value: 2650 }, { label: '20:00', value: 3310 }, { label: 'Now', value: 4180 },
  ],
  '7d': [
    { label: 'Mon', value: 11400 }, { label: 'Tue', value: 13600 }, { label: 'Wed', value: 14500 }, { label: 'Thu', value: 15220 }, { label: 'Fri', value: 16300 }, { label: 'Sat', value: 17800 }, { label: 'Sun', value: 19220 },
  ],
  '30d': [
    { label: 'W1', value: 108420 }, { label: 'W2', value: 114800 }, { label: 'W3', value: 121260 }, { label: 'W4', value: 138430 },
  ],
};

export const analyticsMockHealth: ReadonlyArray<HealthIndicator> = [
  { name: 'Request intake', status: 'healthy', detail: 'Receiving guest requests', latency: '142 ms', updatedAt: 'just now' },
  { name: 'Payments', status: 'healthy', detail: 'Capture and webhook processing', latency: '184 ms', updatedAt: 'just now' },
  { name: 'Realtime delivery', status: 'degraded', detail: 'Elevated retry volume in EU-West', latency: '486 ms', updatedAt: '2 min ago' },
  { name: 'Payouts', status: 'healthy', detail: 'Settlement queue is clear', latency: '205 ms', updatedAt: 'just now' },
];

export const analyticsMockSessions: ReadonlyArray<ActiveSession> = [
  { id: 'ses_club_204', venue: 'Sala Norte', city: 'Madrid', dj: 'Ana Sol', requests: 148, guests: 326, startedAt: '21:34', status: 'live', signal: 98 },
  { id: 'ses_club_205', venue: 'Club Bruma', city: 'Barcelona', dj: 'Marco Vale', requests: 97, guests: 214, startedAt: '22:10', status: 'starting', signal: 94 },
  { id: 'ses_club_206', venue: 'Bajo Cero', city: 'Valencia', dj: 'Jules North', requests: 211, guests: 452, startedAt: '20:56', status: 'paused', signal: 96 },
];
