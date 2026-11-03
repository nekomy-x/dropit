/// <reference types="vite/client" />

/**
 * Build-time configuration of the public guest-app download page.
 *
 * Both variables are documented in `apps/web-club/.env.example`. They are
 * optional on purpose: with neither set, `/download` renders the honest
 * "coming soon" state instead of a link to a listing that does not exist.
 */
interface ImportMetaEnv {
  readonly VITE_GUEST_ANDROID_PACKAGE?: string;
  /** Stripe public key for guest web checkout (never the secret API key). */
  readonly VITE_GUEST_STRIPE_PUBLISHABLE_KEY?: string;
  /** Public App Store listing URL (absolute https). */
  readonly VITE_GUEST_IOS_URL?: string;
  /** Public Google Play listing URL (absolute https). */
  readonly VITE_GUEST_ANDROID_URL?: string;
}
