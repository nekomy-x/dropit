/**
 * Guest-app install facts for the public `/download` page.
 *
 * The page has three honest states per store (docs/ui/hub-download-contract.md
 * §Store links):
 *   1. live   — the store URL is configured at build time (real listing link);
 *   2. direct — `/downloads/dropit-guest.apk` exists (test stand, not a store);
 *   3. soon   — neither, which is the default in this repository.
 *
 * Nothing here reads the network: the URL resolution and the device detection
 * are pure, so both can be exercised in a plain Node check. `navigator` and
 * `document` are only touched behind guards, which keeps the module safe to
 * import outside a browser (build-time scripts, tests).
 */

export type GuestPlatform = 'ios' | 'android' | 'desktop';

/** The two app stores; the platform `desktop` has no store of its own. */
export type GuestStorePlatform = 'ios' | 'android';

export interface GuestStore {
  /** Brand name — deliberately untranslated (see the contract). */
  readonly name: 'App Store' | 'Google Play';
  /** Listing URL when the build was configured with one, otherwise undefined. */
  readonly url?: string;
}

/**
 * Test-stand capability, not a store: `scripts/publish-guest-apk.mjs` copies a
 * built guest APK here (gitignored). The page probes it with HEAD at mount.
 */
export const GUEST_APK_PATH = '/downloads/dropit-guest.apk';

/** The navigator fields detection needs; a snapshot is accepted for checks. */
export interface GuestNavigator {
  userAgent?: string;
  platform?: string;
  maxTouchPoints?: number;
}

function browserNavigator(): GuestNavigator | undefined {
  if (typeof navigator === 'undefined') return undefined;
  return navigator;
}

/**
 * Device class for preselecting the install action.
 *
 * iPhone/iPad/iPod in the UA is iOS. iPadOS 13+ reports a desktop `Macintosh`
 * UA instead, so a Macintosh UA with more than one touch point is also iOS.
 * Android is Android; anything else is desktop, which is where the QR handoff
 * is the honest answer.
 */
export function detectGuestPlatform(source?: GuestNavigator): GuestPlatform {
  const nav = source ?? browserNavigator();
  if (!nav) return 'desktop';
  const userAgent = nav.userAgent ?? '';
  const platform = nav.platform ?? '';
  const touchPoints = nav.maxTouchPoints ?? 0;

  if (/iPhone|iPad|iPod/i.test(userAgent)) return 'ios';
  if (/Android/i.test(userAgent)) return 'android';
  // iPadOS in desktop mode sends the Macintosh UA and `MacIntel` platform; only
  // the touch-point count separates it from a real Mac.
  if (/Macintosh|MacIntel/i.test(`${userAgent} ${platform}`) && touchPoints > 1) return 'ios';
  return 'desktop';
}

/**
 * Reads a build-time store URL. Only an absolute `https:` URL counts: a typo or
 * a placeholder must never render as a broken link on a public page.
 */
function readStoreUrl(value: string | undefined): string | undefined {
  const url = value?.trim();
  return url && /^https:\/\/\S+$/i.test(url) ? url : undefined;
}

/**
 * Vite replaces `import.meta.env` at build time. In a plain Node import it is
 * simply absent, so the optional read keeps this module importable there.
 */
const env = import.meta.env as ImportMetaEnv | undefined;

/** Store names stay in code; URLs come only from the build environment. */
export const GUEST_STORES: Record<GuestStorePlatform, GuestStore> = {
  ios: {
    name: 'App Store',
    url: readStoreUrl(env?.VITE_GUEST_IOS_URL),
  },
  android: {
    name: 'Google Play',
    url: readStoreUrl(env?.VITE_GUEST_ANDROID_URL),
  },
};
