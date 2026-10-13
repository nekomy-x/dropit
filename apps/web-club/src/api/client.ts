/**
 * API client for the DROP IT club panel (component C3, phase F7 skeleton).
 *
 * Auth model (tech map Part I §3): httpOnly cookie session (access 15 min /
 * refresh 30 days) + CSRF token. The CSRF token is read from the
 * `<meta name="csrf-token">` tag rendered by the backend on the HTML page.
 *
 * All money values are signed integers in minor units (cents) — floats are
 * forbidden in money handling (ADR-003).
 */

// ---------------------------------------------------------------------------
// Shared API types
// ---------------------------------------------------------------------------

/** Error envelope used by every endpoint: `{code, message, details}` with an i18n key. */
export interface ApiError {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}

/** Cursor pagination envelope used by list endpoints. */
export interface Page<T> {
  items: T[];
  nextCursor: string | null;
}

// ---------------------------------------------------------------------------
// Domain types (per tech map Part II)
// ---------------------------------------------------------------------------

export type ClubStatus = 'active' | 'suspended' | 'blocked';
export type StripeOnboardingStatus = 'pending' | 'verified' | 'restricted';

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
  stripeOnboardingStatus: StripeOnboardingStatus;
  status: ClubStatus;
  createdAt: string;
}

export interface ClubProfilePatch {
  name?: string;
  address?: string;
  country?: string;
  phone?: string;
  logoUrl?: string | null;
}

/** pricing_policy row; `djShareBp: null` means "use platform default". */
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

export type SessionStatus =
  | 'created'
  | 'active'
  | 'paused_acceptance'
  | 'closed';

export interface Session {
  id: string;
  clubId: string;
  djId: string | null;
  status: SessionStatus;
  pricingPolicyId: string;
  openedAt: string;
  closedAt: string | null;
  timezone: string;
}

export interface QrGenerateResponse {
  sessionId: string;
  /** Payload to encode into the printed/on-screen QR code (guest scans it). */
  qrToken: string;
  /** Guest-preview payload: what the guest sees right after scanning. */
  guestPreview: {
    clubName: string;
    djName: string | null;
    styles: string[];
    /** Current price in minor units, shown to the guest before searching. */
    priceMinor: number;
    currency: string;
  };
  active: boolean;
  generatedAt: string;
  expiresAt: string;
}

export type DjStatus = 'active' | 'suspended' | 'blocked';

export interface Dj {
  id: string;
  clubId: string | null;
  email: string;
  displayName: string;
  stripeAccountId: string | null;
  stripeOnboardingStatus: StripeOnboardingStatus;
  status: DjStatus;
  createdAt: string;
}

export interface DjInviteRequest {
  email: string;
  displayName: string;
}

export type PayoutStatus =
  | 'scheduled'
  | 'in_flight'
  | 'paid'
  | 'failed'
  | 'on_hold';

export interface EarningsSummary {
  period: string;
  /** Gross turnover of paid requests for the period, minor units. */
  grossMinor: number;
  /** Club net after the platform commission, minor units. */
  netMinor: number;
  currency: string;
  requestsCount: number;
}

export interface ClubPayout {
  id: string;
  payeeId: string;
  periodStart: string;
  periodEnd: string;
  amountMinor: number;
  minThresholdMinor: number;
  status: PayoutStatus;
  paidAt: string | null;
  createdAt: string;
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

function csrfToken(): string {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="csrf-token"]');
  return meta?.content ?? '';
}

/**
 * fetch wrapper: credentials are sent automatically (httpOnly cookie),
 * JSON bodies, CSRF header on every mutating request.
 */
async function request<T>(
  path: string,
  init: {
    method?: 'GET' | 'POST' | 'PATCH';
    body?: unknown;
    idempotencyKey?: string;
  } = {},
): Promise<T> {
  const { method = 'GET', body, idempotencyKey } = init;

  const headers: Record<string, string> = { Accept: 'application/json' };
  if (body !== undefined) {
    headers['Content-Type'] = 'application/json';
  }
  if (method !== 'GET') {
    const token = csrfToken();
    if (token) headers['X-CSRF-Token'] = token;
  }
  if (idempotencyKey) {
    headers['Idempotency-Key'] = idempotencyKey;
  }

  const response = await fetch(`/v1${path}`, {
    method,
    headers,
    credentials: 'include',
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (!response.ok) {
    // Error envelope `{code, message, details}`; tolerate non-JSON failures
    // (e.g. a proxy 502) so the UI can still show something meaningful.
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
    throw new ApiRequestError(response.status, payload);
  }

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

// ---------------------------------------------------------------------------
// Typed API surface — club contract (tech map Part IV §1)
// ---------------------------------------------------------------------------

export interface LoginRequest {
  email: string;
  password: string;
}

/** POST /v1/auth/club/login — step 1; a TOTP step may follow (F1). */
export function login(payload: LoginRequest): Promise<{ otpRequired: boolean }> {
  return request<{ otpRequired: boolean }>('/auth/club/login', {
    method: 'POST',
    body: payload,
  });
}

/** GET /v1/club/profile */
export function getProfile(): Promise<ClubProfile> {
  return request<ClubProfile>('/club/profile');
}

/** PATCH /v1/club/profile */
export function patchProfile(patch: ClubProfilePatch): Promise<ClubProfile> {
  return request<ClubProfile>('/club/profile', { method: 'PATCH', body: patch });
}

/** GET /v1/club/pricing — tariffs + full history (pricing_policy rows). */
export function getPricing(): Promise<Page<PricingPolicy>> {
  return request<Page<PricingPolicy>>('/club/pricing');
}

/** POST /v1/club/pricing — creates a NEW policy row (Q6: old row gets valid_to). */
export function createPricing(
  input: { priceMinor: number; djShareBp: number | null },
  idempotencyKey: string,
): Promise<PricingPolicy> {
  return request<PricingPolicy>('/club/pricing', {
    method: 'POST',
    body: input,
    idempotencyKey,
  });
}

/** POST /v1/club/sessions — open a new evening session. */
export function createSession(
  input: { djId: string | null; pricingPolicyId: string; timezone: string },
  idempotencyKey: string,
): Promise<Session> {
  return request<Session>('/club/sessions', {
    method: 'POST',
    body: input,
    idempotencyKey,
  });
}

/** POST /v1/club/sessions/:id/qr/generate — rotate QR for the session. */
export function generateSessionQr(sessionId: string): Promise<QrGenerateResponse> {
  return request<QrGenerateResponse>(
    `/club/sessions/${encodeURIComponent(sessionId)}/qr/generate`,
    { method: 'POST' },
  );
}

/** GET /v1/club/djs */
export function getDjs(): Promise<Page<Dj>> {
  return request<Page<Dj>>('/club/djs');
}

/** POST /v1/club/djs/invite */
export function inviteDj(input: DjInviteRequest): Promise<Dj> {
  return request<Dj>('/club/djs/invite', { method: 'POST', body: input });
}

/** GET /v1/club/earnings?period= */
export function getEarnings(period: string): Promise<EarningsSummary> {
  return request<EarningsSummary>(
    `/club/earnings?period=${encodeURIComponent(period)}`,
  );
}
