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
 *   Most endpoints are LIVE (panel_routes.rs); inviteDj is a 501 stub
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
  _idempotencyKey: string,
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

/** GET /v1/club/djs — live. */
export async function getDjs(): Promise<{ items: Dj[] }> {
  const r = await request<{ items: Array<Record<string, unknown>> }>('/club/djs');
  return {
    items: r.items.map((it) => ({
      id: String(it.id),
      clubId: null,
      email: String(it.email ?? ''),
      displayName: String(it.display_name ?? ''),
      stripeAccountId: null,
      stripeOnboardingStatus: 'pending',
      status: ((it.status as Dj['status']) ?? 'active'),
      createdAt: '',
    })),
  };
}

/** backend not implemented yet: приглашение DJ (email + инвайт-флоу) */
export function inviteDj(_input: DjInviteRequest): Promise<Dj> {
  return Promise.reject(new NotImplementedStubError('POST /v1/club/djs/invite'));
}

/** GET /v1/club/earnings?period= — live. */
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
