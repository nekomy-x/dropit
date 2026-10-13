/**
 * API client for the DROP IT admin panel (component C4, phase F7 skeleton).
 *
 * Auth: httpOnly cookie session + CSRF header (tech map Part I §3, admin
 * role: email + password + mandatory 2FA). Money is always integer minor
 * units; commission/share values are basis points (10000 = 100%).
 */

// ---------------------------------------------------------------------------
// Shared API types
// ---------------------------------------------------------------------------

/** Error envelope `{code, message, details}` with an i18n key. */
export interface ApiError {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}

/** Cursor pagination envelope. */
export interface Page<T> {
  items: T[];
  nextCursor: string | null;
}

// ---------------------------------------------------------------------------
// Domain types
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

export type IncidentStatus = 'open' | 'resolved';

export interface Incident {
  id: string;
  kind: string;
  subject: string;
  details: string;
  status: IncidentStatus;
  createdAt: string;
  resolvedAt: string | null;
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
  requestsPerMinute: number;
  /** Platform turnover today, minor units. */
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

function csrfToken(): string {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="csrf-token"]');
  return meta?.content ?? '';
}

async function request<T>(
  path: string,
  init: {
    method?: 'GET' | 'POST' | 'PATCH';
    body?: unknown;
  } = {},
): Promise<T> {
  const { method = 'GET', body } = init;

  const headers: Record<string, string> = { Accept: 'application/json' };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (method !== 'GET') {
    const token = csrfToken();
    if (token) headers['X-CSRF-Token'] = token;
  }

  const response = await fetch(`/v1${path}`, {
    method,
    headers,
    credentials: 'include',
    body: body === undefined ? undefined : JSON.stringify(body),
  });

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
    throw new ApiRequestError(response.status, payload);
  }

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

// ---------------------------------------------------------------------------
// Typed API surface — admin contract (tech map Part IV §1)
// ---------------------------------------------------------------------------

export interface AdminLoginRequest {
  email: string;
  password: string;
}

/** POST /v1/auth/admin/login — 2FA is mandatory for admins. */
export function login(payload: AdminLoginRequest): Promise<{ otpRequired: boolean }> {
  return request<{ otpRequired: boolean }>('/auth/admin/login', {
    method: 'POST',
    body: payload,
  });
}

/** GET /v1/admin/dashboard/now */
export function getDashboardNow(): Promise<DashboardNow> {
  return request<DashboardNow>('/admin/dashboard/now');
}

// --- Clubs (CRUD /v1/admin/clubs) -------------------------------------------

export function listClubs(): Promise<Page<Club>> {
  return request<Page<Club>>('/admin/clubs');
}

export function createClub(input: ClubCreateInput): Promise<Club> {
  return request<Club>('/admin/clubs', { method: 'POST', body: input });
}

export function getClub(id: string): Promise<Club> {
  return request<Club>(`/admin/clubs/${encodeURIComponent(id)}`);
}

export function patchClub(id: string, patch: ClubPatchInput): Promise<Club> {
  return request<Club>(`/admin/clubs/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    body: patch,
  });
}

/** DELETE /v1/admin/clubs/:id — status-based soft action (suspend/block). */
export function deleteClub(id: string): Promise<void> {
  return request<void>(`/admin/clubs/${encodeURIComponent(id)}`, {
    method: 'POST',
    body: { action: 'delete' },
  });
}

// --- DJs ---------------------------------------------------------------------

export function listDjs(): Promise<Page<AdminDj>> {
  return request<Page<AdminDj>>('/admin/djs');
}

// --- Guests ------------------------------------------------------------------

export function listGuests(): Promise<Page<AdminGuest>> {
  return request<Page<AdminGuest>>('/admin/guests');
}

// --- Payments ------------------------------------------------------------------

export interface PaymentFilters {
  status?: PaymentStatus;
  clubId?: string;
  cursor?: string;
}

/** GET /v1/admin/payments?filters */
export function listPayments(filters: PaymentFilters = {}): Promise<Page<AdminPayment>> {
  const params = new URLSearchParams();
  if (filters.status) params.set('status', filters.status);
  if (filters.clubId) params.set('club_id', filters.clubId);
  if (filters.cursor) params.set('cursor', filters.cursor);
  const query = params.toString();
  return request<Page<AdminPayment>>(`/admin/payments${query ? `?${query}` : ''}`);
}

// --- Incidents -----------------------------------------------------------------

export function listIncidents(): Promise<Page<Incident>> {
  return request<Page<Incident>>('/admin/incidents');
}

/** POST /v1/admin/incidents/:id/resolve */
export function resolveIncident(id: string): Promise<Incident> {
  return request<Incident>(`/admin/incidents/${encodeURIComponent(id)}/resolve`, {
    method: 'POST',
  });
}

// --- Audit log -----------------------------------------------------------------

export function getAuditLog(): Promise<Page<AuditLogEntry>> {
  return request<Page<AuditLogEntry>>('/admin/audit-log');
}

// --- Platform settings -----------------------------------------------------------

/** GET /v1/admin/platform-settings */
export function getPlatformSettings(): Promise<PlatformSettings> {
  return request<PlatformSettings>('/admin/platform-settings');
}

/** PATCH /v1/admin/platform-settings */
export function patchPlatformSettings(
  patch: PlatformSettingsPatch,
): Promise<PlatformSettings> {
  return request<PlatformSettings>('/admin/platform-settings', {
    method: 'PATCH',
    body: patch,
  });
}
