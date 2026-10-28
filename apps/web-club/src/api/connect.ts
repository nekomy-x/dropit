/**
 * API surface for the public "Connect" onboarding flow (club owners).
 *
 * BACKEND STATUS (see crates/gateway-api + docs/openapi.yaml): there is no
 * public self-serve signup yet — `POST /v1/admin/clubs` is admin-only and
 * requires an admin bearer token, so a landing visitor cannot call it.
 * The onboarding endpoint below is the contract the flow is built against;
 * until it lands the call resolves to a 501 `NotImplementedStubError`, and
 * the UI degrades to "черновик сохранён, менеджер свяжется" instead of
 * pretending the application reached the backend.
 *
 * TODO(club-onboarding-endpoint): when `POST /v1/club/onboarding/apply`
 * ships, drop the stub branch in `submitConnectApplication` — the request
 * body is already snake_case and matches the planned DTO.
 */
import { ApiRequestError, NotImplementedStubError, getProfile, isAuthenticated } from './client';
import type { ConnectDraft } from '../connect/types';

export interface ConnectApplicationRequest {
  club: {
    name: string;
    city: string;
    address: string;
    website: string;
    contact: string;
    genres: string[];
  };
  venue: {
    name: string;
    type: string;
    address: string;
    city: string;
    district: string;
    capacity: number;
    seats: number | null;
    area_m2: number | null;
  };
  qr: {
    price_minor: number;
    zones: string[];
  };
}

export interface ConnectApplicationResponse {
  application_id: string;
  status: 'pending_moderation';
  submitted_at: string;
}

/** camelCase draft → snake_case request body (the shape the DTO expects). */
export function toApplicationRequest(draft: ConnectDraft): ConnectApplicationRequest {
  const seats = draft.venue.seats.trim() === '' ? null : Number(draft.venue.seats);
  const area = draft.venue.area.trim() === '' ? null : Number(draft.venue.area);

  return {
    club: {
      name: draft.club.name.trim(),
      city: draft.club.city.trim(),
      address: draft.club.address.trim(),
      website: draft.club.website.trim(),
      contact: draft.club.contact.trim(),
      genres: draft.club.genres,
    },
    venue: {
      name: draft.venue.venueName.trim(),
      type: draft.venue.venueType,
      address: draft.venue.address.trim(),
      city: draft.venue.city.trim(),
      district: draft.venue.district.trim(),
      capacity: Number(draft.venue.capacity) || 0,
      seats,
      area_m2: area,
    },
    qr: {
      price_minor: draft.qr.priceMinor,
      zones: draft.qr.zones,
    },
  };
}

/** Local id used when the backend cannot take the application yet. */
function localApplicationId(): string {
  const suffix = Math.random().toString(36).slice(2, 6).toUpperCase();
  return `LOCAL-${suffix}`;
}

/**
 * POST /v1/club/onboarding/apply — public, no auth.
 *
 * Throws `NotImplementedStubError` while the endpoint is missing (404/501),
 * so the caller can show the honest "draft kept locally" state.
 */
export async function submitConnectApplication(
  draft: ConnectDraft,
): Promise<ConnectApplicationResponse> {
  const body = toApplicationRequest(draft);

  let response: Response;
  try {
    response = await fetch('/v1/club/onboarding/apply', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      body: JSON.stringify(body),
    });
  } catch {
    // Network/proxy failure (e.g. no gateway running) — same UX as a stub.
    throw new NotImplementedStubError('POST /v1/club/onboarding/apply');
  }

  if (response.status === 404 || response.status === 501) {
    throw new NotImplementedStubError('POST /v1/club/onboarding/apply');
  }

  if (!response.ok) {
    let payload = { code: `http_${response.status}`, message: response.statusText };
    try {
      const parsed = (await response.json()) as { code?: string; message?: string };
      if (parsed?.code) payload = { code: parsed.code, message: parsed.message ?? payload.message };
    } catch {
      // keep the fallback envelope
    }
    throw new ApiRequestError(response.status, payload);
  }

  return (await response.json()) as ConnectApplicationResponse;
}

/** Application id shown on the success screen when the call could not be made. */
export const offlineApplicationId = localApplicationId;

/**
 * Prefills the wizard from the club profile when the visitor already has an
 * active panel session (a logged-in owner re-running the flow). Best effort:
 * any failure is swallowed — the form simply stays empty.
 */
export async function prefillFromPanel(): Promise<{
  name: string;
  address: string;
  city: string;
  email: string;
} | null> {
  if (!isAuthenticated()) return null;
  try {
    const profile = await getProfile();
    return {
      name: profile.name,
      address: profile.address,
      city: profile.country,
      email: profile.email,
    };
  } catch {
    return null;
  }
}
