export interface GuestSession {
  test_payments?: boolean;
  session_id: string;
  club_name: string;
  dj_display_name: string | null;
  status: string;
  price_minor?: number;
}
export interface GuestTrack {
  id: string;
  title: string;
  artist: string;
  artwork_url?: string;
}
export interface GuestIntent {
  test_mode?: boolean;
  payment_id: string;
  client_secret: string;
  amount_minor: number;
  currency: string;
  status: string;
}
export interface Checkout {
  key: string;
  sessionId: string;
  roomToken?: string;
  track: GuestTrack;
  requestId?: string;
  intent?: GuestIntent;
  authorized?: boolean;
  confirmed?: boolean;
}

let devicePromise: Promise<string> | undefined;
const DEVICE_KEY = 'dropit.guest.device';
const CHECKOUT_KEY = 'dropit.guest.checkout';

export class GuestApiError extends Error {
  constructor(public code: string, message: string, public status: number) { super(message); }
}

async function request<T>(path: string, body?: unknown, token?: string, key?: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(`/v1${path}`, {
    method: body === undefined ? 'GET' : 'POST',
    credentials: 'omit',
    headers: {
      ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(key ? { 'Idempotency-Key': key } : {}),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: signal ?? AbortSignal.timeout(15000),
  });
  const data = await response.json();
  if (!response.ok) throw new GuestApiError(data.code ?? 'unavailable', data.message ?? 'Service unavailable', response.status);
  return data as T;
}

function device(): Promise<string> {
  if (devicePromise) return devicePromise;
  devicePromise = (async () => {
    // Fail before creating an order if storage is unavailable: a redirect must
    // be able to recover the same device and the same payment.
    const saved = localStorage.getItem(DEVICE_KEY);
    if (saved) return saved;
    const result = await request<{ device_token: string }>('/guest/device', { platform: 'web', app_version: '0.1.0' });
    localStorage.setItem(DEVICE_KEY, result.device_token);
    return result.device_token;
  })().catch((error) => { devicePromise = undefined; throw error; });
  return devicePromise;
}

export async function joinGuest(token: string): Promise<GuestSession> {
  const identity = await device();
  try { return await request('/sessions/join', { qr_token: token }, identity); }
  catch (error) {
    if (!(error instanceof GuestApiError) || error.status !== 401) throw error;
    localStorage.removeItem(DEVICE_KEY);
    devicePromise = undefined;
    return request('/sessions/join', { qr_token: token }, await device());
  }
}

export async function searchGuest(query: string, signal: AbortSignal): Promise<GuestTrack[]> {
  const result = await request<{ items: GuestTrack[] }>(`/catalog/search?q=${encodeURIComponent(query)}&limit=15`, undefined, await device(), undefined, signal);
  return result.items;
}

export function readCheckout(): Checkout | null {
  try {
    const value = JSON.parse(sessionStorage.getItem(CHECKOUT_KEY) ?? 'null') as Checkout | null;
    return value?.key && value.sessionId && value.track?.id ? value : null;
  } catch { return null; }
}
export function saveCheckout(checkout: Checkout) {
  sessionStorage.setItem(CHECKOUT_KEY, JSON.stringify(checkout));
}

export async function prepareCheckout(checkout: Checkout): Promise<Checkout> {
  const identity = await device();
  saveCheckout(checkout);
  if (!checkout.requestId) {
    const created = await request<{ request_id: string; session_id: string }>('/requests', { track_id: checkout.track.id }, identity, checkout.key);
    // The backend derives the session from the live grant. Another tab may
    // have joined another room; never pay for a room different from the UI.
    if (created.session_id !== checkout.sessionId) throw new GuestApiError('room_changed', 'Room changed in another tab. Scan the QR again.', 409);
    checkout = { ...checkout, requestId: created.request_id };
    saveCheckout(checkout);
  }
  if (!checkout.intent?.client_secret) {
    const intent = await request<GuestIntent>(`/requests/${checkout.requestId}/payment`, {}, identity, `payment-${checkout.key}`);
    checkout = { ...checkout, intent };
    saveCheckout(checkout);
  }
  return checkout;
}

export async function confirmCheckout(checkout: Checkout): Promise<Checkout> {
  if (!checkout.intent || !checkout.authorized) throw new Error('Payment is not authorized');
  const method = checkout.intent.test_mode ? 'test-confirm' : 'confirm';
  const result = await request<{ status: string; test_mode?: boolean }>(`/payments/${checkout.intent.payment_id}/${method}`, {}, await device(), `confirm-${checkout.key}`);
  if (checkout.intent.test_mode && (result.test_mode !== true || result.status !== 'captured')) throw new Error('Test payment was not completed');
  if (!['processing', 'captured', 'succeeded'].includes(result.status)) throw new Error('Payment confirmation is pending');
  const confirmed = { ...checkout, confirmed: true };
  saveCheckout(confirmed);
  return confirmed;
}
