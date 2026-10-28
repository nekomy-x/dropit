/**
 * API client for the DROP IT club panel (component C3, F7).
 *
 * Aligns with the ACTUAL backend contracts implemented through F8
 * (docs/openapi.yaml v0.5.0 + crates/gateway-api routes):
 *
 *   LIVE (implemented on the backend):
 *     - POST /v1/auth/club/login        → TokenPair | TotpChallenge
 *     - POST /v1/auth/club/2fa          → TokenPair
 *     - POST /v1/auth/club/master       → MasterLoginResponse
 *     - POST /v1/auth/refresh           → TokenPair
 *     - POST /v1/club/sessions          → SessionResponse (201)
 *     - POST /v1/club/sessions/:id/qr/generate → {session_id, qr_token, expires_at}
 *     - PATCH /v1/club/sessions/:id/close → SessionResponse
 *
 *   Most endpoints are LIVE (panel_routes.rs), including DJ invitation
 *   (club profile/pricing/DJs/earnings are F3+):
 *     - GET/PATCH /v1/club/profile
 *     - GET/POST /v1/club/pricing
 *     - GET /v1/club/djs, POST /v1/club/djs/invite
 *     - GET /v1/club/earnings
 *
 * Auth transport: the backend issues Bearer tokens (access JWT 15 min +
 * refresh 30 days, POST /v1/auth/refresh rotation). HTTP sessions via
 * httpOnly cookies are NOT implemented on the backend yet.
 * TODO(httpOnly-cookie-auth): switch to httpOnly cookie sessions + CSRF
 * token from `<meta name="csrf-token">` once the backend lands them —
 * then drop the in-memory Bearer token store.
 *
 * Bearer tokens live in MEMORY ONLY (module-level variables, never
 * localStorage — security rule: no tokens in persistent web storage).
 * A page refresh therefore requires re-login; that is accepted until the
 * cookie-session backend lands (see TODO above).
 *
 * All money values are signed integers in minor units (cents) — floats
 * are forbidden in money handling (ADR-003).
 */

// ---------------------------------------------------------------------------
// Shared API types
// ---------------------------------------------------------------------------

/** Error envelope used by every endpoint: `{code, message, details}` with a stable i18n key. */
export interface ApiError {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Domain types — mirror the backend serde DTOs exactly (sessions_routes.rs)
// ---------------------------------------------------------------------------

/** `TokenPair` (routes.rs): access+refresh Bearer pair. */
export interface TokenPair {
  access_token: string;
  refresh_token: string;
  token_type: string;
  /** TTL of the access token, seconds (900 = 15 min). */
  expires_in: number;
}

/** `TotpChallenge` (routes.rs): step 1 passed, a TOTP code is required. */
export interface TotpChallenge {
  mfa_required: true;
}

/** `MasterLoginResponse` (routes.rs): one-time club token exchanged for JWTs. */
export interface MasterLoginResponse {
  access_token: string;
  refresh_token: string;
  expires_in: number;
  club_id: string;
  role: 'club';
}

/** Session states per dropit_domain::SessionStatus (openapi 0.5.0). */
export type SessionStatus = 'created' | 'active' | 'paused_acceptance' | 'closed';

/**
 * `SessionResponse` (sessions_routes.rs): id/status/timezone, dj_id only
 * present when assigned (serde skip_serializing_if).
 */
export interface SessionResponse {
  id: string;
  status: SessionStatus;
  timezone: string;
  dj_id?: string;
}

/** `QrGenerateResponse` (sessions_routes.rs). qr_token is shown ONCE — render it into a QR image. */
export interface QrGenerateResponse {
  session_id: string;
  /** Plain 128-bit hex token for the QR image (the server stores only its sha256). */
  qr_token: string;
  /** Planned auto-close of the session (next 07:00 in the club's IANA zone). */
  expires_at: string;
}

/** `GET /v1/club/djs` response domain type. */
export type DjStatus = 'invited' | 'active' | 'suspended' | 'blocked';

/**
 * One row of the club roster.
 *
 * `online` / `onShift` are server facts, not panel guesses: `online` is a
 * heartbeat younger than `presenceTtlSeconds` (`POST /v1/dj/presence` from the
 * DJ console), `onShift` is an unclosed session assigned to this DJ.
 * `presenceAgeSeconds === null` means the DJ has never opened the console —
 * that is "unknown", not "offline", and the UI must say so.
 */
export interface Dj {
  id: string;
  email: string;
  displayName: string;
  status: DjStatus;
  note: string | null;
  lastSeenAt: string | null;
  presenceAgeSeconds: number | null;
  online: boolean;
  onShift: boolean;
  openSessionId: string | null;
  removed: boolean;
  removedAt: string | null;
  inviteExpiresAt: string | null;
  hasPassword: boolean;
  createdAt: string;
}

/** Envelope of `GET /v1/club/djs`. */
export interface DjListResponse {
  items: Dj[];
  /** How recent a heartbeat must be for `online` (server constant). */
  presenceTtlSeconds: number;
}

export interface DjInviteRequest {
  email: string;
  displayName: string;
}

/**
 * `PATCH /v1/club/djs/:id` — access state and/or the club's working note.
 *
 * The two fields are independent; `note: null` clears it. The API needs at
 * least one of them and answers 400 when neither is present.
 */
export function patchClubDj(
  id: string,
  patch: { status?: 'active' | 'suspended'; note?: string | null },
): Promise<{ id: string; display_name: string; status: DjStatus; note: string | null }> {
  return request(`/club/djs/${encodeURIComponent(id)}`, { method: 'PATCH', body: patch });
}

/** `PATCH /v1/club/djs/:id {status}` — kept for callers that only change access. */
export function patchClubDjStatus(id: string, status: 'active' | 'suspended'): Promise<{ id: string; display_name: string; status: DjStatus }> {
  return request(`/club/djs/${encodeURIComponent(id)}`, { method: 'PATCH', body: { status } });
}

/**
 * `DELETE /v1/club/djs/:id` — take the DJ off the roster.
 *
 * `deleted: 'hard'` means an unaccepted invitation was deleted outright;
 * `'soft'` means the row is kept (shift/payout history) and access revoked.
 */
export function removeClubDj(id: string): Promise<{ id: string; display_name?: string; deleted: 'soft' | 'hard'; status: DjStatus }> {
  return request(`/club/djs/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

/** `POST /v1/club/djs/:id/restore` — put a removed DJ back on the roster. */
export function restoreClubDj(id: string): Promise<{ id: string; display_name: string; status: DjStatus; removed: boolean }> {
  return request(`/club/djs/${encodeURIComponent(id)}/restore`, { method: 'POST' });
}

/** `POST /v1/dj/presence` — heartbeat from the DJ console (30 s while visible). */
export function pingDjPresence(): Promise<{ last_seen_at: string; online: boolean }> {
  return request('/dj/presence', { method: 'POST' });
}

export interface DjInviteResponse {
  djId: string;
  inviteToken: string;
  inviteExpiresAt: string;
}

export interface DjQueueItem {
  id: string;
  status: string;
  style: string | null;
  created_at: string;
  title: string;
  artist: string;
  cover_url?: string;
  preview_url?: string;
  isrc?: string;
  same_track_count: number;
}

export interface DjShift {
  id: string;
  status: SessionStatus;
  timezone: string;
  club_id: string | null;
}

/** `GET/POST /v1/club/pricing` response domain type. */
export interface PricingPolicy {
  id: string;
  clubId: string;
  /** Price in minor units (cents), e.g. 300 = 3.00 EUR. */
  priceMinor: number;
  /** DJ share of the remainder after commission, in basis points (10000 = 100%). */
  djShareBp: number | null;
  validFrom: string;
  validTo: string | null;
  createdAt: string;
}

/** `GET/PATCH /v1/club/profile` response domain type. */
export interface ClubProfile {
  id: string;
  name: string;
  slug: string;
  address: string;
  country: string;
  email: string;
  phone: string;
  logoUrl: string | null;
  stripeAccountId: string | null;
  stripeOnboardingStatus: 'pending' | 'verified' | 'restricted';
  status: 'pending_verification' | 'active' | 'suspended' | 'blocked';
  createdAt: string;
}

export interface ClubProfilePatch {
  name?: string;
  address?: string;
  country?: string;
  phone?: string;
  logoUrl?: string | null;
}

/** `GET /v1/club/earnings` response domain type. */
export interface EarningsSummary {
  period: string;
  /** Gross turnover of paid requests for the period, minor units. */
  grossMinor: number;
  /** Club net after the platform commission, minor units. */
  netMinor: number;
  currency: string;
  requestsCount: number;
}

export interface ClubDashboard {
  session: { id: string; status: SessionStatus } | null;
  requestsToday: number;
  earnedTotalMinor: number;
}

// ---------------------------------------------------------------------------
// Transport layer
// ---------------------------------------------------------------------------

/** Thrown for non-2xx responses; `status === 401` means "redirect to /login". */
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
let expiresAt: number | null = null;
let refreshPromise: Promise<TokenPair> | null = null;
let authGeneration = 0;
let authenticatedRole: 'club' | 'dj' | null = null;

/** True after a successful login (password-only or 2FA) or token refresh. */
export function isAuthenticated(role?: 'club' | 'dj'): boolean {
  return accessToken !== null && (role === undefined || authenticatedRole === role);
}

/** Exposed for debugging/telemetry only — do NOT log or persist this. */
export function accessTokenExpiry(): number | null {
  return expiresAt;
}

/** Clears the in-memory tokens ("sign out" — server-side revocation lands with the cookie-session backend). */
export function clearTokens(): void {
  authGeneration += 1;
  accessToken = null;
  refreshToken = null;
  expiresAt = null;
  refreshPromise = null;
  authenticatedRole = null;
}

function storeTokens(pair: { access_token: string; refresh_token: string; expires_in: number }, newSession = true, role?: 'club' | 'dj'): void {
  if (newSession) {
    authGeneration += 1;
    refreshPromise = null;
    if (role) authenticatedRole = role;
  }
  accessToken = pair.access_token;
  refreshToken = pair.refresh_token;
  expiresAt = Date.now() + pair.expires_in * 1000;
}

function storePair(pair: TokenPair, newSession = true): void {
  storeTokens(pair, newSession);
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
    idempotencyKey?: string;
    auth?: boolean;
    retryOn401?: boolean;
  } = {},
): Promise<T> {
  const { method = 'GET', body, idempotencyKey, auth = true, retryOn401 = true } = init;
  const generation = authGeneration;
  const sentToken = accessToken;

  const headers: Record<string, string> = { Accept: 'application/json' };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (auth && accessToken) headers['Authorization'] = `Bearer ${accessToken}`;
  if (idempotencyKey) headers['Idempotency-Key'] = idempotencyKey;

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
    // Access token expired (TTL 15 min) — rotate the refresh family once,
    // then replay the original request. A second 401 propagates to the UI
    // (useAsyncData redirects to /login).
    try {
      await refreshTokens();
      return request<T>(path, { ...init, retryOn401: false });
    } catch {
      if (generation === authGeneration) clearTokens();
    }
  }

  if (!response.ok) {
    // Error envelope `{code, message, details}` (crates/common error.rs);
    // tolerate non-JSON failures (e.g. a proxy 502) so the UI can still
    // show something meaningful.
    let payload: ApiError = {
      code: `http_${response.status}`,
      message: response.statusText || 'Request failed',
    };
    try {
      const parsed = (await response.json()) as ApiError;
      if (parsed && typeof parsed.code === 'string') payload = parsed;
    } catch {
      // keep the fallback envelope
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
// Typed API surface — club contract, LIVE endpoints (openapi.yaml 0.5.0)
// ---------------------------------------------------------------------------

export interface LoginRequest {
  email: string;
  password: string;
}

/**
 * POST /v1/auth/club/login — step 1 (Argon2id password check).
 * Returns EITHER a TokenPair (2FA not configured) or a TotpChallenge
 * (`mfa_required: true` → the UI must then call `login2fa`).
 */
export async function login(
  payload: LoginRequest,
): Promise<TokenPair | TotpChallenge> {
  const result = await request<TokenPair | TotpChallenge>('/auth/club/login', {
    method: 'POST',
    body: payload,
    auth: false,
  });
  if ('access_token' in result) storeTokens(result, true, 'club');
  return result;
}

/** POST /v1/auth/club/master — exchange the one-time admin-issued token for JWTs. */
export async function masterLogin(qrMasterToken: string): Promise<MasterLoginResponse> {
  const result = await request<MasterLoginResponse>('/auth/club/master', {
    method: 'POST',
    body: { qr_master_token: qrMasterToken },
    auth: false,
  });
  storeTokens(result, true, 'club');
  return result;
}

/** POST /v1/auth/dj/login — DJ account login. */
export async function djLogin(payload: LoginRequest): Promise<TokenPair | TotpChallenge> {
  const result = await request<TokenPair | TotpChallenge>('/auth/dj/login', {
    method: 'POST', body: payload, auth: false,
  });
  if ('access_token' in result) storeTokens(result, true, 'dj');
  return result;
}

/** POST /v1/auth/dj/2fa — second step for DJs with MFA enabled. */
export async function djLogin2fa(input: { email: string; totp_code: string }): Promise<TokenPair> {
  const pair = await request<TokenPair>('/auth/dj/2fa', { method: 'POST', body: input, auth: false });
  storeTokens(pair, true, 'dj');
  return pair;
}

// ---------------------------------------------------------------------------
// Open registration — docs/signup-contract.md (Test Mode stand)
// ---------------------------------------------------------------------------
// Open signup without e-mail confirmation, enabled by the backend flag
// DROPIT_OPEN_SIGNUP. The 201 response is a TokenPair *plus* the created
// entity (club / dj), so the UI never needs a second request to log in.
// Error codes: 400 validation_failed, 403 signup_disabled, 404 club_not_found,
// 409 email_taken, 429 rate_limited — all surface inline in the UI.

/** Created club echoed back with the tokens (clients may ignore it). */
export interface RegisteredClub {
  id: string;
  name: string;
  slug: string;
  email: string;
}

/** 201 body of POST /v1/auth/club/register. */
export interface ClubRegisterResponse extends TokenPair {
  club?: RegisteredClub;
}

export interface ClubRegisterRequest {
  name: string;
  email: string;
  password: string;
  /** Optional; only sent when the manual form supplies it. */
  city?: string;
  /** ISO-3166 alpha-2, optional. */
  country?: string;
}

/** POST /v1/auth/club/register — creates an active club and logs it in. */
export async function registerClub(payload: ClubRegisterRequest): Promise<ClubRegisterResponse> {
  const result = await request<ClubRegisterResponse>('/auth/club/register', {
    method: 'POST',
    body: payload,
    auth: false,
  });
  storeTokens(result, true, 'club');
  return result;
}

/** Created DJ echoed back with the tokens. `club_id === null` means freelance. */
export interface RegisteredDj {
  id: string;
  display_name: string;
  email: string;
  club_id: string | null;
}

/** 201 body of POST /v1/auth/dj/register. */
export interface DjRegisterResponse extends TokenPair {
  dj?: RegisteredDj;
}

export interface DjRegisterRequest {
  display_name: string;
  email: string;
  password: string;
  /** Optional club slug; unknown slug → 404 club_not_found. */
  club_slug?: string;
}

/** POST /v1/auth/dj/register — creates an active DJ and logs it in. */
export async function registerDj(payload: DjRegisterRequest): Promise<DjRegisterResponse> {
  const result = await request<DjRegisterResponse>('/auth/dj/register', {
    method: 'POST',
    body: payload,
    auth: false,
  });
  storeTokens(result, true, 'dj');
  return result;
}

/** POST /v1/auth/club/2fa — step 2 (6-digit TOTP code) → TokenPair. */
export async function login2fa(input: {
  email: string;
  totp_code: string;
}): Promise<TokenPair> {
  const pair = await request<TokenPair>('/auth/club/2fa', {
    method: 'POST',
    body: input,
    auth: false,
  });
  storeTokens(pair, true, 'club');
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

export interface CreateSessionInput {
  dj_id?: string;
  pricing_policy_id?: string;
  /** IANA zone, e.g. 'Europe/Madrid' (DST-safe planned auto-close at 07:00). */
  timezone: string;
}

/** POST /v1/club/sessions — club opens a shift → 201 SessionResponse (status=created). */
export function createSession(input: CreateSessionInput): Promise<SessionResponse> {
  return request<SessionResponse>('/club/sessions', {
    method: 'POST',
    body: input,
    idempotencyKey: newIdempotencyKey(),
  });
}

/**
 * Ключ идемпотентности для POST.
 *
 * `crypto.randomUUID()` существует только в secure context (HTTPS или
 * localhost). Стенд открывается по http://<IP>, где метода нет — вызов
 * бросал TypeError, и создание сессии падало ещё до запроса («Could not
 * open a session» при пустом списке сетевых вызовов). Фолбэк собирает
 * UUID v4 из `crypto.getRandomValues`, а если недоступен и он — из
 * Math.random: ключ нужен уникальным, не криптостойким.
 */
function newIdempotencyKey(): string {
  const c = typeof crypto !== 'undefined' ? crypto : undefined;
  if (c && typeof c.randomUUID === 'function') return c.randomUUID();

  const bytes = new Uint8Array(16);
  if (c && typeof c.getRandomValues === 'function') {
    c.getRandomValues(bytes);
  } else {
    for (let i = 0; i < bytes.length; i += 1) bytes[i] = Math.floor(Math.random() * 256);
  }
  // RFC 4122 v4: версия и вариант.
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** GET /v1/club/dashboard — current shift and tonight's activity. */
export async function getClubDashboard(): Promise<ClubDashboard> {
  const r = await request<Record<string, unknown>>('/club/dashboard');
  const session = r.session as { id?: string; status?: SessionStatus } | null;
  return {
    session: session?.id ? { id: session.id, status: session.status ?? 'created' } : null,
    requestsToday: Number(r.requests_today ?? 0),
    earnedTotalMinor: Number(r.earned_total_minor ?? 0),
  };
}

/**
 * Current open shift for the signed-in club, or `null` when there is none.
 *
 * F13 (restore the shift after reload): there is NO `GET /v1/club/sessions/:id`
 * on the backend and we must not invent one. The honest source is
 * `GET /v1/club/dashboard`, whose SQL
 * (`crates/gateway-api/src/panel_routes.rs`, `club_dashboard`) selects the
 * newest session of this club with `status <> 'closed'`:
 *
 *   WITH s AS (SELECT id, status FROM session
 *              WHERE club_id = $1 AND status <> 'closed'
 *              ORDER BY created_at DESC LIMIT 1)
 *   SELECT (SELECT id FROM s), (SELECT status FROM s), ...
 *
 * So `getClubDashboard().session` is exactly "the shift that is still open".
 * Returns `{id, status}` only — the dashboard response carries no `timezone`
 * or `dj_id`, so callers must not expect a full `SessionResponse`.
 */
export async function getCurrentClubSession(): Promise<{ id: string; status: SessionStatus } | null> {
  const dashboard = await getClubDashboard();
  return dashboard.session;
}

/**
 * POST /v1/club/sessions/:id/qr/generate — rotate the QR token.
 * The plain token appears in the response ONCE and is rendered as a QR image.
 */
export function generateSessionQr(sessionId: string): Promise<QrGenerateResponse> {
  return request<QrGenerateResponse>(
    `/club/sessions/${encodeURIComponent(sessionId)}/qr/generate`,
    { method: 'POST' },
  );
}

/** PATCH /v1/club/sessions/:id/close — close the shift (pending requests expire). */
export function closeSession(sessionId: string): Promise<SessionResponse> {
  return request<SessionResponse>(
    `/club/sessions/${encodeURIComponent(sessionId)}/close`,
    { method: 'PATCH' },
  );
}

// ---------------------------------------------------------------------------
// LIVE endpoints (panel_routes.rs): profile / pricing / djs / earnings
// ---------------------------------------------------------------------------
// The backend implements these since the panel-endpoints milestone
// (see docs/openapi.yaml "Панельные роуты"). Responses use snake_case;
// mapped to the camelCase types below.

/** GET /v1/club/profile — live. */
export async function getProfile(): Promise<ClubProfile> {
  const r = await request<Record<string, unknown>>('/club/profile');
  return {
    id: String(r.id),
    name: String(r.name ?? ''),
    slug: String(r.slug ?? ''),
    address: String(r.address ?? ''),
    country: String(r.country ?? ''),
    email: String(r.email ?? ''),
    phone: String(r.phone ?? ''),
    logoUrl: (r.logo_url as string | null) ?? null,
    stripeAccountId: null,
    stripeOnboardingStatus: ((r.stripe_onboarding_status as ClubProfile['stripeOnboardingStatus']) ?? 'pending'),
    status: ((r.status as ClubProfile['status']) ?? 'active'),
    createdAt: '',
  };
}

/** PATCH /v1/club/profile — live. */
export function patchProfile(patch: ClubProfilePatch): Promise<ClubProfile> {
  // camelCase → snake_case body.
  const body: Record<string, unknown> = {};
  if (patch.name !== undefined) body.name = patch.name;
  if (patch.address !== undefined) body.address = patch.address;
  if (patch.country !== undefined) body.country = patch.country;
  if (patch.phone !== undefined) body.phone = patch.phone;
  if (patch.logoUrl !== undefined) body.logo_url = patch.logoUrl;
  return request<Record<string, unknown>>('/club/profile', {
    method: 'PATCH',
    body,
  }).then(getProfileMapping);
}

async function getProfileMapping(r: Record<string, unknown>): Promise<ClubProfile> {
  return {
    id: String(r.id),
    name: String(r.name ?? ''),
    slug: String(r.slug ?? ''),
    address: String(r.address ?? ''),
    country: String(r.country ?? ''),
    email: String(r.email ?? ''),
    phone: String(r.phone ?? ''),
    logoUrl: (r.logo_url as string | null) ?? null,
    stripeAccountId: null,
    stripeOnboardingStatus: ((r.stripe_onboarding_status as ClubProfile['stripeOnboardingStatus']) ?? 'pending'),
    status: ((r.status as ClubProfile['status']) ?? 'active'),
    createdAt: '',
  };
}

/** GET /v1/club/pricing — live (история Q6: valid_to у старых). */
export async function getPricing(): Promise<{ items: PricingPolicy[] }> {
  const r = await request<{ items: Array<Record<string, unknown>> }>('/club/pricing');
  return {
    items: r.items.map((it) => ({
      id: String(it.id),
      clubId: '',
      priceMinor: Number(it.price_minor),
      djShareBp: (it.dj_share_bp as number | null) ?? null,
      validFrom: String(it.valid_from),
      validTo: (it.valid_to as string | null) ?? null,
      createdAt: String(it.valid_from),
    })),
  };
}

/** POST /v1/club/pricing — live (новая строка, старая закрывается). */
export function createPricing(
  input: { priceMinor: number; djShareBp: number | null },
): Promise<PricingPolicy> {
  return request<Record<string, unknown>>('/club/pricing', {
    method: 'POST',
    body: {
      price_minor: input.priceMinor,
      dj_share_bp: input.djShareBp,
    },
  }).then((r) => ({
    id: String(r.id),
    clubId: '',
    priceMinor: Number(r.price_minor),
    djShareBp: (r.dj_share_bp as number | null) ?? null,
    validFrom: new Date().toISOString(),
    validTo: null,
    createdAt: new Date().toISOString(),
  }));
}

/**
 * GET /v1/club/djs — live.
 *
 * `includeRemoved` also returns DJs taken off the roster, so the page can offer
 * "restore" without a second endpoint. The default roster view asks for the
 * active list only.
 */
export async function getDjs(includeRemoved = false): Promise<DjListResponse> {
  const query = includeRemoved ? '?include_removed=1' : '';
  const r = await request<{
    items: Array<Record<string, unknown>>;
    presence_ttl_seconds?: number;
  }>(`/club/djs${query}`);
  return {
    items: r.items.map((it) => ({
      id: String(it.id),
      email: String(it.email ?? ''),
      displayName: String(it.display_name ?? ''),
      status: ((it.status as Dj['status']) ?? 'active'),
      note: (it.note as string | null) ?? null,
      lastSeenAt: (it.last_seen_at as string | null) ?? null,
      presenceAgeSeconds: it.presence_age_seconds === null || it.presence_age_seconds === undefined
        ? null
        : Number(it.presence_age_seconds),
      online: it.online === true,
      onShift: it.on_shift === true,
      openSessionId: (it.open_session_id as string | null) ?? null,
      removed: it.removed === true,
      removedAt: (it.removed_at as string | null) ?? null,
      inviteExpiresAt: (it.invite_expires_at as string | null) ?? null,
      hasPassword: it.has_password === true,
      createdAt: String(it.created_at ?? ''),
    })),
    // A backend that does not send the TTL is not allowed to make the panel
    // invent one: the caller falls back to the sentence "presence unknown".
    presenceTtlSeconds: Number(r.presence_ttl_seconds ?? 0),
  };
}

/** POST /v1/club/djs/invite — invite link is returned once, expires in 7 days. */
export async function inviteDj(input: DjInviteRequest): Promise<DjInviteResponse> {
  const result = await request<{ dj_id: string; invite_token: string; invite_expires_at: string }>(
    '/club/djs/invite',
    { method: 'POST', body: { email: input.email, display_name: input.displayName } },
  );
  return { djId: result.dj_id, inviteToken: result.invite_token, inviteExpiresAt: result.invite_expires_at };
}

export function getDjShift(): Promise<{ session: DjShift | null }> {
  return request<{ session: DjShift | null }>('/dj/shift');
}

export async function getDjQueue(sort: 'new' | 'hot' = 'new'): Promise<{ items: DjQueueItem[] }> {
  return request<{ items: DjQueueItem[] }>(`/dj/queue?sort=${sort}`);
}

export function djRequestAction(id: string, action: 'accept' | 'reject' | 'played', reason?: string): Promise<unknown> {
  return request(`/dj/requests/${encodeURIComponent(id)}/${action}`, {
    method: 'POST', body: action === 'reject' ? { reason: reason ?? 'Declined by DJ' } : undefined,
  });
}

export function setDjAcceptance(paused: boolean): Promise<unknown> {
  return request(`/dj/acceptance/${paused ? 'pause' : 'resume'}`, { method: 'POST' });
}

export function acceptDjInvite(inviteToken: string, password: string): Promise<{ accepted: boolean }> {
  return request<{ accepted: boolean }>('/dj/invite/accept', {
    method: 'POST', body: { invite_token: inviteToken, password }, auth: false,
  });
}

export async function getDjStats(): Promise<{ requests: number; accepted: number; played: number; rejected: number; earnedMinor: number }> {
  const r = await request<Record<string, unknown>>('/dj/stats/today');
  return { requests: Number(r.requests_today ?? 0), accepted: Number(r.accepted ?? 0), played: Number(r.played ?? 0), rejected: Number(r.rejected ?? 0), earnedMinor: Number(r.earned_total_minor ?? 0) };
}

/**
 * GET /v1/club/earnings?period= — live.
 *
 * PERIOD SEMANTICS (label honestly!): the backend does NOT use calendar days.
 * `crates/gateway-api/src/panel_routes.rs` (`club_earnings`) builds a ROLLING
 * window ending at `now()`: `today` = `interval '1 day'`, `week` = `7 days`,
 * `month` = `30 days`. So "today" means "the last 24 hours", not "since
 * midnight" — UI copy must say so (F15).
 *
 * Field semantics:
 * - `netMinor` = sum of `split.amount_minor - reversed_minor` where the payee
 *   is this club, for splits whose payment was captured inside the window.
 *   It is the CLUB share after the platform commission; it does NOT include
 *   the DJ share (that is `dj_share_bp` of the remainder).
 * - `grossMinor` = sum of `payment.amount_minor` for captured payments.
 * - `payments` → `requestsCount` = count of CAPTURED payments in the window
 *   (unlike `getClubDashboard().requestsToday`, which counts all requests and
 *   is not filtered by payment at all).
 */
export function getEarnings(period: string): Promise<EarningsSummary> {
  return request<Record<string, unknown>>(
    `/club/earnings?period=${encodeURIComponent(period)}`,
  ).then((r) => ({
    period: String(r.period ?? period),
    grossMinor: Number(r.gross_minor ?? 0),
    netMinor: Number(r.net_minor ?? 0),
    currency: 'EUR',
    requestsCount: Number(r.payments ?? 0),
  }));
}

/* ------------------------------------------------------------------------- */
/* Password reset                                                             */
/* ------------------------------------------------------------------------- */

export type ResetRole = 'club' | 'dj' | 'admin';

export interface ResetRequestResult {
  requested: true;
  /**
   * Present only on a stand with `DROPIT_RESET_REVEAL_TOKEN` on: there is no
   * mailer in this deployment, so the server hands the link back instead of
   * pretending an e-mail went out. Absent in production.
   */
  reset_url?: string;
  expires_at?: string;
}

/**
 * POST /v1/auth/password/reset-request — always 202 for a well-formed request,
 * whether or not the address exists (no user enumeration). A 503 means the
 * endpoint is not deployed yet; callers must say so instead of showing
 * "check your inbox".
 */
export function requestPasswordReset(input: {
  email: string;
  role: ResetRole;
}): Promise<ResetRequestResult> {
  return request<ResetRequestResult>('/auth/password/reset-request', {
    method: 'POST',
    body: input,
    auth: false,
  });
}

/** POST /v1/auth/password/reset — 204; the token is single-use and expires. */
export async function resetPassword(input: { token: string; password: string }): Promise<void> {
  await request<void>('/auth/password/reset', {
    method: 'POST',
    body: input,
    auth: false,
  });
}
