import { IS_DEMO } from '../app-mode';

/**
 * Destination of the landing "Connect a club" call to action.
 *
 * - Production (the Test Mode stand): open registration at `/register`. The
 *   account is created and signed in with one click, so the visitor lands in a
 *   working panel. The onboarding wizard's submit endpoint is still a 501 stub,
 *   so `/connect/club` was a dead end for a real visitor.
 * - Demo build: keep the onboarding wizard, where the flow is local-only.
 *
 * Labels stay as they are — only the destination changes.
 */
export const CLUB_JOIN_CTA = IS_DEMO ? '/connect/club' : '/register';

/** True when the primary landing CTA leads to the wizard (demo build). */
export const USES_ONBOARDING_WIZARD = IS_DEMO;
