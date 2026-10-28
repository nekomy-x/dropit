/**
 * API client for the DROP IT admin panel (component C4).
 *
 * Aligns with the ACTUAL backend contracts implemented through F8
 * (docs/openapi.yaml v0.5.0 + crates/gateway-api/payouts_routes.rs):
 *
 *   LIVE (implemented on the backend, F6):
 *     - POST /v1/auth/admin/login     → TotpChallenge (2FA mandatory)
 *     - POST /v1/auth/admin/2fa       → TokenPair
 *     - POST /v1/auth/refresh         → TokenPair
 *     - POST /v1/admin/payouts/run    → {payouts_created, total_minor}
 *     - GET  /v1/admin/payouts        → {items: [{id, payee_type, payee_id,
 *                                        amount_minor, status, attempts, is_final}]}
 *     - POST /v1/admin/reconciliation/run → {run_id, status}
 *     - GET  /v1/admin/incidents      → {items: [{id, kind, message}]}
 *     - POST /v1/admin/incidents/:id/resolve → {resolved: true}
 *
 *   LIVE: dashboard/now, clubs list/patch, djs, payments, audit-log, platform-settings;
 *     - GET /v1/admin/dashboard/now
 *     - CRUD /v1/admin/clubs (+ :id GET/PATCH)
 *     - GET /v1/admin/djs, GET /v1/admin/guests
 *     - GET /v1/admin/payments
 *     - GET /v1/admin/audit-log
 *     - GET/PATCH /v1/admin/platform-settings
 *
 * Auth transport: Bearer tokens (access JWT 15 min + refresh 30 days,
 * POST /v1/auth/refresh rotation; admin 2FA is MANDATORY — the login
 * endpoint always answers with TotpChallenge). HTTP sessions via
 * httpOnly cookies are NOT implemented on the backend yet.
 * TODO(httpOnly-cookie-auth): switch to httpOnly cookie sessions + CSRF
 * token from `<meta name="csrf-token">` once the backend lands them —
 * then drop the in-memory Bearer token store.
 *
 * Bearer tokens live in MEMORY ONLY (module-level variables, never
 * localStorage — security rule). A page refresh therefore requires
 * re-login; that is accepted until the cookie-session backend lands.
 *
 * Money is always integer minor units (ADR-003); commission/share values
 * are basis points (10000 = 100%).
 */

// ---------------------------------------------------------------------------
// Shared API types
// ---------------------------------------------------------------------------

/** Error envelope `{code, message, details}` with a stable i18n key (crates/common/src/error.rs). */
export interface ApiError {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Domain types — live F6 surfaces (payouts_routes.rs serde DTOs)
// ---------------------------------------------------------------------------

/** `TokenPair` (routes.rs): access+refresh Bearer pair. */
export interface TokenPair {
  access_token: string;
  refresh_token: string;
  token_type: string;
  /** TTL of the access token, seconds (900 = 15 min). */
  expires_in: number;
}

/** `TotpChallenge` (routes.rs): 2FA is required to finish the login. */
export interface TotpChallenge {
  mfa_required: true;
}

/** `PayoutRunResponse` (payouts_routes.rs). */
export interface PayoutRunResponse {
  payouts_created: number;
  /** Net total of the created payouts, minor units. */
  total_minor: number;
}

/** `PayoutDto` (payouts_routes.rs) — one payout row. */
export interface PayoutDto {
  id: string;
  /** 'club' | 'dj' (payee_type string from the backend). */
  payee_type: string;
  payee_id: string;
  amount_minor: number;
  status: string;
  attempts: number;
  is_final: boolean;
}

/** `GET /v1/admin/payouts` envelope. */
export interface PayoutListResponse {
  items: PayoutDto[];
}

/** `ReconciliationResponse` (payouts_routes.rs). */
export interface ReconciliationResponse {
  run_id: string;
  /** 'ok' | 'discrepancies' | 'running' | 'failed'. */
  status: string;
}

/** `IncidentDto` (payouts_routes.rs) — open incident row. */
export interface IncidentDto {
  id: string;
  kind: string;
  message: string;
}

/** `GET /v1/admin/incidents` envelope (open incidents only). */
export interface IncidentListResponse {
  items: IncidentDto[];
}

// ---------------------------------------------------------------------------
// Domain types for panel endpoints and the remaining club detail stub
// ---------------------------------------------------------------------------

export type ClubStatus = 'pending_verification' | 'active' | 'suspended' | 'blocked';
export type StripeOnboardingStatus = 'pending' | 'verified' | 'restricted';
export type DjStatus = 'invited' | 'active' | 'suspended' | 'blocked';
export type GuestStatus = 'active' | 'blocked';

export interface Club {
  id: string;
  name: string;
  city?: string;
  slug: string;
  address: string;
  country: string;
  email: string;
  phone: string;
  logoUrl: string | null;
  stripeAccountId: string | null;
  stripeOnboardingStatus: StripeOnboardingStatus;
  status: ClubStatus;
  createdAt: string;
}

export interface ClubCreateInput {
  name: string;
  city: string;
  timezone: string;
  address: string;
}

export interface ClubCreateResult extends Club {
  qrMasterToken: string;
}

export interface ClubPatchInput {
  name?: string;
  address?: string;
  country?: string;
  phone?: string;
  status?: ClubStatus;
}

export interface AdminDj {
  id: string;
  clubId: string | null;
  email: string;
  displayName: string;
  stripeOnboardingStatus: StripeOnboardingStatus;
  status: DjStatus;
  createdAt: string;
}

export interface AdminGuest {
  id: string;
  displayName: string;
  email: string;
  createdAt: string;
  requestsCount: number;
  lastSeenAt: string;
  banned: boolean;
}

export type PaymentStatus =
  | 'intent'
  | 'processing'
  | 'captured'
  | 'partially_refunded'
  | 'refunded'
  | 'failed'
  | 'cancelling'
  | 'cancelled';

export interface AdminPayment {
  id: string;
  requestId: string;
  amountMinor: number;
  currency: string;
  status: PaymentStatus;
  refundedMinor: number;
  failureReason: string | null;
  createdAt: string;
  capturedAt: string | null;
}

export interface AuditLogEntry {
  id: string;
  actorType: string;
  actorId: string;
  action: string;
  entity: string;
  diff: Record<string, unknown> | null;
  at: string;
}

/** GET /v1/admin/dashboard/now — live platform metrics. */
export interface DashboardNow {
  activeSessions: number;
  requestsToday: number;
  requestsPerMinute: number;
  turnoverTodayMinor: number;
  currency: string;
  openIncidents: number;
}

/**
 * Deployment mode of this installation (`runtime_mode` on platform settings).
 * Both web panels render a fixed, non-dismissible strip for `test` and
 * `maintenance`; `normal` renders no strip. `auto` resolves on the service
 * side from the real stand facts (open signup / mock payments ⇒ `test`,
 * otherwise `normal`), unless `DROPIT_MAINTENANCE` forces `maintenance`.
 */
export type RuntimeMode = 'auto' | 'test' | 'maintenance' | 'normal';

/**
 * platform_settings (id=1). All bp fields are basis points (0..10000);
 * minor fields are integer cents.
 */
export interface PlatformSettings {
  commissionMode: 'percent' | 'fixed';
  commissionPercentBp: number;
  commissionFixedMinor: number;
  minPriceMinor: number;
  maxPriceMinor: number;
  djShareOfRemainderBp: number;
  currency: string;
  runtimeMode: RuntimeMode;
  updatedAt: string;
}

export type PlatformSettingsPatch = Partial<
  Omit<PlatformSettings, 'currency' | 'updatedAt'>
>;

// ---------------------------------------------------------------------------
// Transport layer
// ---------------------------------------------------------------------------

/** Thrown for non-2xx responses; `status === 401` → redirect to /login. */
export class ApiRequestError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details?: Record<string, unknown>;

  constructor(status: number, payload: ApiError) {
    super(payload.message);
    this.name = 'ApiRequestError';
    this.status = status;
    this.code = payload.code;
    this.details = payload.details;
  }
}

/** Local "backend not implemented yet" error — the client stub refuses without touching the network. */
export class NotImplementedStubError extends Error {
  readonly status = 501;
  readonly code = 'not_implemented';

  constructor(method: string) {
    super(`${method}: backend not implemented yet`);
    this.name = 'NotImplementedStubError';
  }
}

// --- in-memory Bearer token store (module scope; NEVER persisted) ------------

let accessToken: string | null = null;
let refreshToken: string | null = null;
let refreshPromise: Promise<TokenPair> | null = null;
let authGeneration = 0;

/** True after a successful 2FA login or token refresh. */
export function isAuthenticated(): boolean {
  return accessToken !== null;
}

/** Clears the in-memory tokens ("sign out"). */
export function clearTokens(): void {
  authGeneration += 1;
  accessToken = null;
  refreshToken = null;
  refreshPromise = null;
}

function storePair(pair: TokenPair, newSession = true): void {
  if (newSession) {
    authGeneration += 1;
    refreshPromise = null;
  }
  accessToken = pair.access_token;
  refreshToken = pair.refresh_token;
}

/**
 * fetch wrapper: Bearer Authorization from the in-memory store, JSON bodies.
 * 401 with a stored refresh token triggers one silent rotation
 * (POST /v1/auth/refresh — the backend rotates the family; reuse revokes it).
 */
async function request<T>(
  path: string,
  init: {
    method?: 'GET' | 'POST' | 'PATCH' | 'DELETE';
    body?: unknown;
    auth?: boolean;
    retryOn401?: boolean;
  } = {},
): Promise<T> {
  const { method = 'GET', body, auth = true, retryOn401 = true } = init;
  const generation = authGeneration;
  const sentToken = accessToken;

  const headers: Record<string, string> = { Accept: 'application/json' };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (auth && accessToken) headers['Authorization'] = `Bearer ${accessToken}`;

  const response = await fetch(`/v1${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (generation !== authGeneration) {
    throw new ApiRequestError(401, { code: 'unauthorized', message: 'Session changed' });
  }

  // Another request may already have rotated this token while this 401 was in flight.
  if (response.status === 401 && auth && retryOn401 && accessToken && sentToken !== accessToken) {
    return request<T>(path, { ...init, retryOn401: false });
  }

  if (response.status === 401 && auth && retryOn401 && refreshToken) {
    try {
      await refreshTokens();
      return request<T>(path, { ...init, retryOn401: false });
    } catch {
      if (generation === authGeneration) clearTokens();
    }
  }

  if (!response.ok) {
    let payload: ApiError = {
      code: `http_${response.status}`,
      message: response.statusText || 'Request failed',
    };
    try {
      const parsed = (await response.json()) as ApiError;
      if (parsed && typeof parsed.code === 'string') payload = parsed;
    } catch {
      // keep fallback envelope
    }
    if (response.status === 401 && auth && generation === authGeneration) clearTokens();
    throw new ApiRequestError(response.status, payload);
  }

  if (response.status === 204) return undefined as T;
  const result = (await response.json()) as T;
  if (generation !== authGeneration) {
    throw new ApiRequestError(401, { code: 'unauthorized', message: 'Session changed' });
  }
  return result;
}

// ---------------------------------------------------------------------------
// Typed API surface — LIVE endpoints (openapi.yaml 0.5.0, F1 + F6)
// ---------------------------------------------------------------------------

export interface AdminLoginRequest {
  email: string;
  password: string;
}

/**
 * POST /v1/auth/admin/login — step 1 (Argon2id password check).
 * Admin 2FA is MANDATORY, so the answer is (almost) always a
 * TotpChallenge; finish with `login2fa`.
 */
export async function login(
  payload: AdminLoginRequest,
): Promise<TokenPair | TotpChallenge> {
  const result = await request<TokenPair | TotpChallenge>('/auth/admin/login', {
    method: 'POST',
    body: payload,
    auth: false,
  });
  if ('access_token' in result) storePair(result);
  return result;
}

/** POST /v1/auth/admin/2fa — step 2 (6-digit TOTP code) → TokenPair. */
export async function login2fa(input: {
  email: string;
  totp_code: string;
}): Promise<TokenPair> {
  const pair = await request<TokenPair>('/auth/admin/2fa', {
    method: 'POST',
    body: input,
    auth: false,
  });
  storePair(pair);
  return pair;
}

/** POST /v1/auth/refresh — rotate the refresh-token family; reuse on the backend revokes it. */
export async function refreshTokens(): Promise<TokenPair> {
  if (!refreshToken) throw new ApiRequestError(401, { code: 'unauthorized', message: 'No refresh token' });
  if (refreshPromise) return refreshPromise;
  const generation = authGeneration;
  const token = refreshToken;
  const pending = request<TokenPair>('/auth/refresh', {
    method: 'POST',
    body: { refresh_token: token },
    auth: false,
  }).then((pair) => {
    if (generation !== authGeneration) {
      throw new ApiRequestError(401, { code: 'unauthorized', message: 'Session ended' });
    }
    storePair(pair, false);
    return pair;
  }).finally(() => {
    if (refreshPromise === pending) refreshPromise = null;
  });
  refreshPromise = pending;
  return pending;
}

// --- Payouts (F6) --------------------------------------------------------------

/** POST /v1/admin/payouts/run — manual trigger of the weekly payout cycle (prod: cron Mon 10:00 UTC). */
export function runPayoutCycle(): Promise<PayoutRunResponse> {
  return request<PayoutRunResponse>('/admin/payouts/run', { method: 'POST' });
}

/** GET /v1/admin/payouts — payouts from the last week. */
export function listPayouts(): Promise<PayoutListResponse> {
  return request<PayoutListResponse>('/admin/payouts');
}

/** POST /v1/admin/reconciliation/run — manual reconciliation trigger (prod: cron 05:00). */
export function runReconciliation(): Promise<ReconciliationResponse> {
  return request<ReconciliationResponse>('/admin/reconciliation/run', {
    method: 'POST',
  });
}

// --- Incidents (F6) ------------------------------------------------------------

/** GET /v1/admin/incidents — currently open incidents. */
export function listIncidents(): Promise<IncidentListResponse> {
  return request<IncidentListResponse>('/admin/incidents');
}

/** POST /v1/admin/incidents/:id/resolve → {resolved: true}. */
export function resolveIncident(id: string): Promise<{ resolved: boolean }> {
  return request<{ resolved: boolean }>(
    `/admin/incidents/${encodeURIComponent(id)}/resolve`,
    { method: 'POST' },
  );
}

// ---------------------------------------------------------------------------
// LIVE panel endpoints (panel_routes.rs) + remaining unsupported surfaces
// ---------------------------------------------------------------------------
// Dashboard, clubs, guests, DJs, payments, audit-log and settings are live.

/** GET /v1/admin/dashboard/now — live. */
export function getDashboardNow(): Promise<DashboardNow> {
  return request<Record<string, unknown>>('/admin/dashboard/now').then((r) => ({
    activeSessions: Number(r.active_sessions ?? 0),
    requestsToday: Number(r.requests_today ?? 0),
    requestsPerMinute: Number(r.requests_per_minute ?? 0),
    turnoverTodayMinor: Number(r.turnover_today_minor ?? 0),
    currency: String(r.currency ?? 'EUR'),
    openIncidents: Number(r.open_incidents ?? 0),
  }));
}

/** GET /v1/admin/clubs — live. */
export async function listClubs(): Promise<{ items: Club[] }> {
  const r = await request<{ items: Array<Record<string, unknown>> }>('/admin/clubs');
  return {
    items: r.items.map((it) => ({
      id: String(it.id),
      name: String(it.name ?? ''),
      city: String(it.city ?? ''),
      slug: '',
      address: '',
      country: '',
      email: String(it.email ?? ''),
      phone: '',
      logoUrl: null,
      stripeAccountId: null,
      stripeOnboardingStatus: 'pending',
      status: ((it.status as Club['status']) ?? 'active'),
      createdAt: '',
    })),
  };
}

export async function createClub(input: ClubCreateInput): Promise<ClubCreateResult> {
  const r = await request<{ club_id: string; name: string; qr_master_token: string }>('/admin/clubs', {
    method: 'POST', body: { name: input.name, city: input.city, timezone: input.timezone, address: input.address || null },
  });
  return {
    id: r.club_id, name: r.name, city: input.city, slug: '', address: input.address,
    country: '', email: '', phone: '', logoUrl: null, stripeAccountId: null,
    stripeOnboardingStatus: 'pending', status: 'pending_verification', createdAt: new Date().toISOString(),
    qrMasterToken: r.qr_master_token,
  };
}

/** backend not implemented yet: карточка клуба */
export function getClub(_id: string): Promise<Club> {
  return Promise.reject(new NotImplementedStubError('GET /v1/admin/clubs/:id'));
}

/** PATCH /v1/admin/clubs/:id — live (статус: блокировка/пауза; аудит пишется). */
export function patchClub(id: string, patch: ClubPatchInput): Promise<Club> {
  const body: Record<string, unknown> = {};
  if (patch.status !== undefined) body.status = patch.status;
  return request<Record<string, unknown>>(
    `/admin/clubs/${encodeURIComponent(id)}`,
    { method: 'PATCH', body },
  ).then((r) => ({
    id: String(r.id ?? id),
    name: '',
    city: '',
    slug: '',
    address: '',
    country: '',
    email: '',
    phone: '',
    logoUrl: null,
    stripeAccountId: null,
    stripeOnboardingStatus: 'pending',
    status: ((r.status as Club['status']) ?? patch.status ?? 'active'),
    createdAt: '',
  }));
}

/** GET /v1/admin/djs — live. */
export async function listDjs(): Promise<{ items: AdminDj[] }> {
  const r = await request<{ items: Array<Record<string, unknown>> }>('/admin/djs');
  return {
    items: r.items.map((it) => ({
      id: String(it.id),
      clubId: (it.club_id as string | null) ?? null,
      email: String(it.email ?? ''),
      displayName: String(it.display_name ?? ''),
      stripeOnboardingStatus: 'pending',
      status: ((it.status as AdminDj['status']) ?? 'active'),
      createdAt: '',
    })),
  };
}

export async function listGuests(clubId: string): Promise<{ items: AdminGuest[] }> {
  const r = await request<{ items: Array<Record<string, unknown>> }>(
    `/admin/guests?club_id=${encodeURIComponent(clubId)}`,
  );
  return { items: r.items.map((it) => ({
    id: String(it.id),
    displayName: String(it.display_name ?? ''),
    email: String(it.email ?? ''),
    createdAt: String(it.created_at ?? ''),
    requestsCount: Number(it.requests_count ?? 0),
    lastSeenAt: String(it.last_seen_at ?? ''),
    banned: Boolean(it.banned),
  })) };
}

export interface PaymentFilters {
  status?: PaymentStatus;
  clubId?: string;
  cursor?: string;
}

/** GET /v1/admin/payments — live (status and club filters supported). */
export async function listPayments(filters: PaymentFilters = {}): Promise<{
  items: AdminPayment[];
}> {
  const params = new URLSearchParams();
  if (filters.status) params.set('status', filters.status);
  if (filters.clubId) params.set('club_id', filters.clubId);
  const query = params.size ? `?${params.toString()}` : '';
  const r = await request<{ items: Array<Record<string, unknown>> }>(`/admin/payments${query}`);
  return {
    items: r.items.map((it) => ({
      id: String(it.id),
      requestId: String(it.request_id ?? ''),
      amountMinor: Number(it.amount_minor ?? 0),
      currency: 'EUR',
      status: ((it.status as AdminPayment['status']) ?? 'intent'),
      refundedMinor: 0,
      failureReason: null,
      createdAt: String(it.created_at ?? ''),
      capturedAt: null,
    })),
  };
}

/** GET /v1/admin/audit-log — live. */
export async function getAuditLog(): Promise<{ items: AuditLogEntry[] }> {
  const r = await request<{ items: Array<Record<string, unknown>> }>('/admin/audit-log');
  return {
    items: r.items.map((it) => ({
      id: String(it.id ?? ''),
      actorType: String(it.actor_type ?? ''),
      actorId: String(it.actor_id ?? ''),
      action: String(it.action ?? ''),
      entity: String(it.entity ?? ''),
      diff: null,
      at: String(it.at ?? ''),
    })),
  };
}

/** GET /v1/admin/platform-settings — live (NULL = не задано владельцем). */
export function getPlatformSettings(): Promise<PlatformSettings> {
  return request<Record<string, unknown>>('/admin/platform-settings').then(mapSettings);
}

function mapRuntimeMode(value: unknown): RuntimeMode {
  return value === 'test' || value === 'maintenance' || value === 'normal' ? value : 'auto';
}

function mapSettings(r: Record<string, unknown>): PlatformSettings {
  return {
    commissionMode: (r.commission_mode === 'fixed' ? 'fixed' : 'percent'),
    commissionPercentBp: Number(r.commission_percent_bp ?? 0),
    commissionFixedMinor: Number(r.commission_fixed_minor ?? 0),
    minPriceMinor: Number(r.min_price_minor ?? 0),
    maxPriceMinor: Number(r.max_price_minor ?? 0),
    djShareOfRemainderBp: Number(r.dj_share_of_remainder_bp ?? 0),
    currency: String(r.currency ?? 'EUR'),
    runtimeMode: mapRuntimeMode(r.runtime_mode),
    updatedAt: String(r.updated_at ?? ''),
  };
}

/** PATCH /v1/admin/platform-settings — live (валидация диапазонов + аудит). */
export function patchPlatformSettings(
  patch: PlatformSettingsPatch,
): Promise<PlatformSettings> {
  const body: Record<string, unknown> = {};
  if (patch.commissionMode !== undefined) body.commission_mode = patch.commissionMode;
  if (patch.commissionPercentBp !== undefined) body.commission_percent_bp = patch.commissionPercentBp;
  if (patch.commissionFixedMinor !== undefined) body.commission_fixed_minor = patch.commissionFixedMinor;
  if (patch.minPriceMinor !== undefined) body.min_price_minor = patch.minPriceMinor;
  if (patch.maxPriceMinor !== undefined) body.max_price_minor = patch.maxPriceMinor;
  if (patch.djShareOfRemainderBp !== undefined)
    body.dj_share_of_remainder_bp = patch.djShareOfRemainderBp;
  if (patch.runtimeMode !== undefined) body.runtime_mode = patch.runtimeMode;
  return request<Record<string, unknown>>('/admin/platform-settings', {
    method: 'PATCH',
    body,
  }).then(mapSettings);
}

// ---------------------------------------------------------------------------
// Mail (docs/mail-infra-contract.md §7)
// ---------------------------------------------------------------------------
// Admin-only delivery oversight. Rows are returned exactly as the backend
// stores them (snake_case, money/ids/statuses untouched); the screen only maps
// `status` onto a chip tone. Nothing here reformats a value.

/**
 * The complete `mail_message.status` vocabulary (CHECK constraint,
 * contract §2). These are the only values the backend can store, so the
 * status filter offers exactly this list.
 */
export const MAIL_STATUSES = [
  'queued',
  'sending',
  'sent',
  'delivered',
  'delivery_delayed',
  'bounced',
  'complained',
  'failed',
  'suppressed',
] as const;

export type MailStatus = (typeof MAIL_STATUSES)[number];

/** `MailItem` — one row of `mail_message` as the admin list returns it. */
export interface MailItem {
  id: string;
  to: string;
  to_name: string | null;
  subject: string;
  template: string;
  status: MailStatus;
  attempts: number;
  max_attempts: number;
  provider_id: string | null;
  last_error: string | null;
  created_at: string;
  sent_at: string | null;
  delivered_at: string | null;
  aggregate_type: string | null;
  aggregate_id: string | null;
}

/** `GET /v1/admin/mail` envelope — cursor pagination, `next_cursor` null at the end. */
export interface MailListResponse {
  items: MailItem[];
  next_cursor: string | null;
}

/** `GET /v1/admin/mail/stats` — counters over `window_hours` (24 h). */
export interface MailStats {
  window_hours: number;
  queued: number;
  sending: number;
  sent: number;
  delivered: number;
  bounced: number;
  complained: number;
  failed: number;
  suppressed: number;
  total: number;
}

/** One provider event from `mail_event`, as the detail route returns it. */
export interface MailEvent {
  id: string;
  type: string;
  received_at: string;
}

/** `GET /v1/admin/mail/{id}` — the row plus the stored body and its events. */
export interface MailDetail extends MailItem {
  html: string;
  text: string;
  vars: Record<string, unknown>;
  events: MailEvent[];
}

/** One row of `mail_suppression`. */
export interface MailSuppression {
  email: string;
  /** 'bounce' | 'complaint' | 'manual'. */
  reason: string;
  detail: string | null;
  created_at: string;
}

/** `GET /v1/admin/mail/suppressions` envelope. */
export interface MailSuppressionListResponse {
  items: MailSuppression[];
}

export interface MailListFilters {
  status?: string;
  q?: string;
  limit?: number;
  cursor?: string;
}

/** GET /v1/admin/mail — newest first, filtered by status and a free-text query. */
export function listMail(filters: MailListFilters = {}): Promise<MailListResponse> {
  const params = new URLSearchParams();
  if (filters.status) params.set('status', filters.status);
  if (filters.q) params.set('q', filters.q);
  if (filters.limit) params.set('limit', String(filters.limit));
  if (filters.cursor) params.set('cursor', filters.cursor);
  const query = params.size ? `?${params.toString()}` : '';
  return request<MailListResponse>(`/admin/mail${query}`);
}

/** GET /v1/admin/mail/stats — 24 h delivery counters. */
export function getMailStats(): Promise<MailStats> {
  return request<MailStats>('/admin/mail/stats');
}

/** GET /v1/admin/mail/{id} — one message with html, text, vars and events. */
export function getMailMessage(id: string): Promise<MailDetail> {
  return request<MailDetail>(`/admin/mail/${encodeURIComponent(id)}`);
}

/** POST /v1/admin/mail/{id}/resend — enqueues a copy, answers with the new id. */
export function resendMail(id: string): Promise<{ id: string; status: string }> {
  return request<{ id: string; status: string }>(
    `/admin/mail/${encodeURIComponent(id)}/resend`,
    { method: 'POST' },
  );
}

/** GET /v1/admin/mail/suppressions — addresses Resend refuses to deliver to. */
export function listMailSuppressions(limit = 50): Promise<MailSuppressionListResponse> {
  return request<MailSuppressionListResponse>(
    `/admin/mail/suppressions?limit=${encodeURIComponent(String(limit))}`,
  );
}

/** DELETE /v1/admin/mail/suppressions/{email} — lifts a suppression. */
export function removeMailSuppression(email: string): Promise<{ removed: boolean }> {
  return request<{ removed: boolean }>(
    `/admin/mail/suppressions/${encodeURIComponent(email)}`,
    { method: 'DELETE' },
  );
}
