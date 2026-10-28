/**
 * Types + defaults for the public "Connect" onboarding flow (club owners).
 *
 * The wizard is a 3-step draft that is auto-saved to localStorage and only
 * submitted at the end. Nothing here is secret, so localStorage is allowed —
 * the "no tokens in web storage" rule applies to auth tokens only.
 */

/** Party genres offered in step 1 — mirrors the chips in the .pen design. */
export const GENRES = [
  'Techno',
  'House',
  'Salsa',
  'Bachata',
  'Hip-Hop',
  'Rock',
  'Disco',
  'Afro house',
  'Drum & Bass',
] as const;

export type Genre = (typeof GENRES)[number];

/** Venue types offered in step 2. */
export const VENUE_TYPES = [
  'Ночной клуб',
  'Бар',
  'Клуб-лофт',
  'Ресторан',
  'Фестиваль',
  'Пляжный клуб',
] as const;

export type VenueType = (typeof VENUE_TYPES)[number];

export interface ClubStepData {
  /** Public club name — shown to guests in the room list. */
  name: string;
  city: string;
  address: string;
  website: string;
  /** Contact for the DROP IT manager (email or phone). */
  contact: string;
  genres: Genre[];
}

export interface VenueStepData {
  venueName: string;
  venueType: VenueType;
  address: string;
  city: string;
  district: string;
  /** Maximum guests per night. */
  capacity: string;
  /** Seats (tables). */
  seats: string;
  /** Hall area, m². */
  area: string;
}

export interface QrStepData {
  /** Guest price per request, in euro cents (money is integer minor units). */
  priceMinor: number;
  /** Zones that will get their own QR codes. */
  zones: string[];
  /** Club accepts the tariff + moderation terms. */
  accepted: boolean;
}

export interface ConnectDraft {
  club: ClubStepData;
  venue: VenueStepData;
  qr: QrStepData;
  /** ISO timestamp of the last successful auto-save. */
  savedAt: string | null;
}

export const DEFAULT_DRAFT: ConnectDraft = {
  club: { name: '', city: '', address: '', website: '', contact: '', genres: [] },
  venue: {
    venueName: '',
    venueType: 'Ночной клуб',
    address: '',
    city: '',
    district: '',
    capacity: '',
    seats: '',
    area: '',
  },
  qr: { priceMinor: 200, zones: ['Основной зал'], accepted: false },
  savedAt: null,
};

/** Field-level errors keyed by field name. */
export type FieldErrors<T> = Partial<Record<keyof T, string>>;

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;
const PHONE_RE = /^\+?[\d\s()-]{7,}$/;

/** Step 1 validation — mirrors the checklist card on the right of the form. */
export function validateClub(club: ClubStepData): FieldErrors<ClubStepData> {
  const errors: FieldErrors<ClubStepData> = {};
  if (club.name.trim().length < 2) errors.name = 'Укажите название клуба';
  if (club.city.trim().length < 2) errors.city = 'Укажите город';
  if (club.address.trim().length < 5) errors.address = 'Укажите адрес площадки';
  const site = club.website.trim();
  if (!site) {
    errors.website = 'Укажите сайт или соцсети';
  } else if (!/\./.test(site.replace(/^https?:\/\//, ''))) {
    errors.website = 'Похоже на неполный адрес';
  }
  const contact = club.contact.trim();
  if (!contact) {
    errors.contact = 'Укажите контакт для связи';
  } else if (!EMAIL_RE.test(contact) && !PHONE_RE.test(contact)) {
    errors.contact = 'Нужен e-mail или телефон';
  }
  if (club.genres.length === 0) errors.genres = 'Выберите хотя бы одно направление';
  return errors;
}

/** Step 2 validation. */
export function validateVenue(venue: VenueStepData): FieldErrors<VenueStepData> {
  const errors: FieldErrors<VenueStepData> = {};
  if (venue.venueName.trim().length < 2) errors.venueName = 'Укажите название площадки';
  if (venue.address.trim().length < 5) errors.address = 'Укажите адрес';
  if (venue.city.trim().length < 2) errors.city = 'Укажите город';
  const capacity = Number(venue.capacity);
  if (!venue.capacity.trim() || Number.isNaN(capacity) || capacity <= 0) {
    errors.capacity = 'Вместимость числом';
  } else if (capacity > 20000) {
    errors.capacity = 'Слишком большое значение';
  }
  if (venue.seats.trim() && (Number.isNaN(Number(venue.seats)) || Number(venue.seats) < 0)) {
    errors.seats = 'Число посадочных мест';
  }
  if (venue.area.trim() && (Number.isNaN(Number(venue.area)) || Number(venue.area) <= 0)) {
    errors.area = 'Площадь в м²';
  }
  return errors;
}

/** Step 3 validation. */
export function validateQr(qr: QrStepData): FieldErrors<QrStepData> {
  const errors: FieldErrors<QrStepData> = {};
  if (qr.priceMinor < 100) errors.priceMinor = 'Минимум 1,00 € за запрос';
  if (qr.priceMinor > 5000) errors.priceMinor = 'Максимум 50,00 € за запрос';
  if (qr.zones.length === 0) errors.zones = 'Добавьте хотя бы одну зону';
  if (!qr.accepted) errors.accepted = 'Подтвердите условия тарифа';
  return errors;
}

export function hasErrors(errors: Record<string, string | undefined>): boolean {
  return Object.values(errors).some(Boolean);
}

/** 1 234,50 € — money is integer minor units, formatting only happens here. */
export function formatEuro(minor: number, fractionDigits = 2): string {
  return new Intl.NumberFormat('ru-RU', {
    style: 'currency',
    currency: 'EUR',
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  }).format(minor / 100);
}
