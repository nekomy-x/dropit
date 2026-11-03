import { IS_DEMO } from '../app-mode';

/**
 * Destination of the landing "Connect a club" call to action.
 *
 * - Production: the club application at `/register`. The visitor submits an
 *   application; the account is created only after approval, from the one-time
 *   invitation link (docs/onboarding-contract.md). The onboarding wizard's
 *   submit endpoint is still a 501 stub, so `/connect/club` was a dead end for
 *   a real visitor.
 * - Demo build: keep the onboarding wizard, where the flow is local-only.
 *
 * The primary CTA keeps its label — only the destination changes.
 */
export const CLUB_JOIN_CTA = IS_DEMO ? '/connect/club' : '/register';

/** True when the primary landing CTA leads to the wizard (demo build). */
export const USES_ONBOARDING_WIZARD = IS_DEMO;
