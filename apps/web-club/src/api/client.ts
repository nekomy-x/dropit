/**
 * API client for the DROP IT club panel (component C3, F7).
 *
 * Aligns with the ACTUAL backend contracts implemented through F8
 * (docs/openapi.yaml v0.5.0 + crates/gateway-api routes):
 *
 *   LIVE (implemented on the backend):
 *     - POST /v1/auth/club/login        → TokenPair | TotpChallenge
 *     - POST /v1/auth/club/2fa          → TokenPair
 *     - POST /v1/auth/refresh           → TokenPair
 *     - POST /v1/club/sessions          → SessionResponse (201)
 *     - POST /v1/club/sessions/:id/qr/generate → {session_id, qr_token, expires_at}
 *     - PATCH /v1/club/sessions/:id/close → SessionResponse
 *
 *   NOT IMPLEMENTED YET on the backend — methods below are local 501 stubs
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

/** Reference type for the not-yet-implemented /v1/club/djs listing. */
export type DjStatus = 'active' | 'suspended' | 'blocked';

export interface Dj {
  id: string;
  clubId: string | null;
  email: string;
  displayName: string;
  stripeAccountId: string | null;
  stripeOnboardingStatus: 'pending' | 'verified' | 'restricted';
  status: DjStatus;
  createdAt: string;
}

export interface DjInviteRequest {
  email: string;
  displayName: string;
}

/** Reference type for the not-yet-implemented /v1/club/pricing listing. */
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

/** Reference type for the not-yet-implemented /v1/club/profile. */
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
  status: 'active' | 'suspended' | 'blocked';
  createdAt: string;
}

export interface ClubProfilePatch {
  name?: string;
  address?: string;
  country?: string;
  phone?: string;
  logoUrl?: string | null;
}

/** Reference type for the not-yet-implemented /v1/club/earnings. */
export interface EarningsSummary {
  period: string;
  /** Gross turnover of paid requests for the period, minor units. */
  grossMinor: number;
  /** Club net after the platform commission, minor units. */
  netMinor: number;
  currency: string;
  requestsCount: number;
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

/** True after a successful login (password-only or 2FA) or token refresh. */
export function isAuthenticated(): boolean {
  return accessToken !== null;
}

/** Exposed for debugging/telemetry only — do NOT log or persist this. */
export function accessTokenExpiry(): number | null {
  return expiresAt;
}

/** Clears the in-memory tokens ("sign out" — server-side revocation lands with the cookie-session backend). */
export function clearTokens(): void {
  accessToken = null;
  refreshToken = null;
  expiresAt = null;
}

function storePair(pair: TokenPair): void {
  accessToken = pair.access_token;
  refreshToken = pair.refresh_token;
  expiresAt = Date.now() + pair.expires_in * 1000;
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
    idempotencyKey?: string;
    auth?: boolean;
    retryOn401?: boolean;
  } = {},
): Promise<T> {
  const { method = 'GET', body, idempotencyKey, auth = true, retryOn401 = true } = init;

  const headers: Record<string, string> = { Accept: 'application/json' };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (auth && accessToken) headers['Authorization'] = `Bearer ${accessToken}`;
  if (idempotencyKey) headers['Idempotency-Key'] = idempotencyKey;

  const response = await fetch(`/v1${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (response.status === 401 && auth && retryOn401 && refreshToken) {
    // Access token expired (TTL 15 min) — rotate the refresh family once,
    // then replay the original request. A second 401 propagates to the UI
    // (useAsyncData redirects to /login).
    try {
      await refreshTokens();
      return request<T>(path, { ...init, retryOn401: false });
    } catch {
      clearTokens();
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
    if (response.status === 401) clearTokens();
    throw new ApiRequestError(response.status, payload);
  }

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
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
  if ('access_token' in result) storePair(result);
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
    idempotencyKey: crypto.randomUUID(),
  });
}

/**
 * POST /v1/club/sessions/:id/qr/generate — rotate the QR token.
 * The plain token appears in the response ONCE; the QR image rendering
 * remains a placeholder until the encoder module lands.
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
// NOT IMPLEMENTED YET — local 501 stubs ("backend not implemented yet")
// ---------------------------------------------------------------------------
// The backend (F0–F8) has no /v1/club/profile, /v1/club/pricing,
// /v1/club/djs, /v1/club/earnings routes. These signatures keep the pages
// compiling and render explicit "waiting for backend" states; each call
// rejects locally with a 501-style error instead of hitting the network.
// TODO(backend): implement the club reference-data endpoints (F3+), then
// replace the stub bodies with real `request<T>()` calls.
// ---------------------------------------------------------------------------

/** backend not implemented yet */
export function getProfile(): Promise<ClubProfile> {
  return Promise.reject(new NotImplementedStubError('GET /v1/club/profile'));
}

/** backend not implemented yet */
export function patchProfile(_patch: ClubProfilePatch): Promise<ClubProfile> {
  return Promise.reject(new NotImplementedStubError('PATCH /v1/club/profile'));
}

/** backend not implemented yet */
export function getPricing(): Promise<{ items: PricingPolicy[] }> {
  return Promise.reject(new NotImplementedStubError('GET /v1/club/pricing'));
}

/** backend not implemented yet */
export function createPricing(
  _input: { priceMinor: number; djShareBp: number | null },
  _idempotencyKey: string,
): Promise<PricingPolicy> {
  return Promise.reject(new NotImplementedStubError('POST /v1/club/pricing'));
}

/** backend not implemented yet */
export function getDjs(): Promise<{ items: Dj[] }> {
  return Promise.reject(new NotImplementedStubError('GET /v1/club/djs'));
}

/** backend not implemented yet */
export function inviteDj(_input: DjInviteRequest): Promise<Dj> {
  return Promise.reject(new NotImplementedStubError('POST /v1/club/djs/invite'));
}

/** backend not implemented yet */
export function getEarnings(_period: string): Promise<EarningsSummary> {
  return Promise.reject(new NotImplementedStubError('GET /v1/club/earnings'));
}
