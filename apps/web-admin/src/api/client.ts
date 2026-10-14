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
 *   NOT IMPLEMENTED YET on the backend — local 501 stubs:
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
// Reference types — not implemented yet, used by the stub surface below
// ---------------------------------------------------------------------------

export type ClubStatus = 'active' | 'suspended' | 'blocked';
export type StripeOnboardingStatus = 'pending' | 'verified' | 'restricted';
export type DjStatus = 'active' | 'suspended' | 'blocked';
export type GuestStatus = 'active' | 'blocked';

export interface Club {
  id: string;
  name: string;
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
  slug: string;
  email: string;
  address: string;
  country: string;
  phone: string;
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
  platform: string;
  appVersion: string;
  firstSeen: string;
  lastSeen: string;
  status: GuestStatus;
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

/** GET /v1/admin/dashboard/now — live platform metrics (not implemented yet). */
export interface DashboardNow {
  activeSessions: number;
  requestsPerMinute: number;
  turnoverTodayMinor: number;
  currency: string;
  openIncidents: number;
}

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

/** True after a successful 2FA login or token refresh. */
export function isAuthenticated(): boolean {
  return accessToken !== null;
}

/** Clears the in-memory tokens ("sign out"). */
export function clearTokens(): void {
  accessToken = null;
  refreshToken = null;
}

function storePair(pair: TokenPair): void {
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
    method?: 'GET' | 'POST' | 'PATCH';
    body?: unknown;
    auth?: boolean;
    retryOn401?: boolean;
  } = {},
): Promise<T> {
  const { method = 'GET', body, auth = true, retryOn401 = true } = init;

  const headers: Record<string, string> = { Accept: 'application/json' };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (auth && accessToken) headers['Authorization'] = `Bearer ${accessToken}`;

  const response = await fetch(`/v1${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (response.status === 401 && auth && retryOn401 && refreshToken) {
    try {
      await refreshTokens();
      return request<T>(path, { ...init, retryOn401: false });
    } catch {
      clearTokens();
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
    if (response.status === 401) clearTokens();
    throw new ApiRequestError(response.status, payload);
  }

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
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
  const pair = await request<TokenPair>('/auth/refresh', {
    method: 'POST',
    body: { refresh_token: refreshToken },
    auth: false,
  });
  storePair(pair);
  return pair;
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
// NOT IMPLEMENTED YET — local 501 stubs ("backend not implemented yet")
// ---------------------------------------------------------------------------
// The gateway has no admin dashboard/clubs/djs/guests/payments/audit-log/
// platform-settings routes. These signatures keep the pages compiling and
// render explicit "waiting for backend" states; each call rejects locally
// with a 501-style error instead of hitting the network.
// TODO(backend): implement the admin reference-data endpoints, then
// replace the stub bodies with real `request<T>()` calls.
// ---------------------------------------------------------------------------

/** backend not implemented yet */
export function getDashboardNow(): Promise<DashboardNow> {
  return Promise.reject(new NotImplementedStubError('GET /v1/admin/dashboard/now'));
}

/** backend not implemented yet */
export function listClubs(): Promise<{ items: Club[] }> {
  return Promise.reject(new NotImplementedStubError('GET /v1/admin/clubs'));
}

/** backend not implemented yet */
export function createClub(_input: ClubCreateInput): Promise<Club> {
  return Promise.reject(new NotImplementedStubError('POST /v1/admin/clubs'));
}

/** backend not implemented yet */
export function getClub(_id: string): Promise<Club> {
  return Promise.reject(new NotImplementedStubError('GET /v1/admin/clubs/:id'));
}

/** backend not implemented yet */
export function patchClub(_id: string, _patch: ClubPatchInput): Promise<Club> {
  return Promise.reject(new NotImplementedStubError('PATCH /v1/admin/clubs/:id'));
}

/** backend not implemented yet */
export function listDjs(): Promise<{ items: AdminDj[] }> {
  return Promise.reject(new NotImplementedStubError('GET /v1/admin/djs'));
}

/** backend not implemented yet */
export function listGuests(): Promise<{ items: AdminGuest[] }> {
  return Promise.reject(new NotImplementedStubError('GET /v1/admin/guests'));
}

export interface PaymentFilters {
  status?: PaymentStatus;
  clubId?: string;
  cursor?: string;
}

/** backend not implemented yet */
export function listPayments(_filters: PaymentFilters = {}): Promise<{
  items: AdminPayment[];
}> {
  return Promise.reject(new NotImplementedStubError('GET /v1/admin/payments'));
}

/** backend not implemented yet */
export function getAuditLog(): Promise<{ items: AuditLogEntry[] }> {
  return Promise.reject(new NotImplementedStubError('GET /v1/admin/audit-log'));
}

/** backend not implemented yet */
export function getPlatformSettings(): Promise<PlatformSettings> {
  return Promise.reject(new NotImplementedStubError('GET /v1/admin/platform-settings'));
}

/** backend not implemented yet */
export function patchPlatformSettings(
  _patch: PlatformSettingsPatch,
): Promise<PlatformSettings> {
  return Promise.reject(new NotImplementedStubError('PATCH /v1/admin/platform-settings'));
}
