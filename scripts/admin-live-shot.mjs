#!/usr/bin/env node
/**
 * Admin live screenshots — the verification tool for the operator screens that
 * only exist in a production build.
 *
 *   node scripts/admin-live-shot.mjs --page /admin/mail --size 390x844 --theme dark
 *   node scripts/admin-live-shot.mjs                     # every live route, both themes
 *
 * Why this exists
 * ---------------
 * `apps/web-admin/src/pages/*` has two route families. The control center
 * (`/admin/`, `/admin/manage/*`) mounts in demo mode and is what
 * `panel-shot.mjs` / `panel-audit.mjs` address. The live operator screens
 * (`/admin/mail`, `/admin/payments`, `/admin/payouts`, …) mount ONLY in a
 * production build and sit behind a mandatory 2FA Bearer session, so in demo
 * mode every one of them redirects to the control center. They cannot be
 * screenshotted by the demo tools; this script is the tool that can see them.
 *
 * It does not change product routing, auth or API code. Everything below is
 * test scaffolding:
 *
 *   1. it runs against the PRODUCTION dev server (`npm run dev` in
 *      apps/web-admin, http://localhost:5174/admin/),
 *   2. it intercepts `/v1/**` and plays the backend for the login handshake
 *      (`POST /v1/auth/admin/login` → TotpChallenge, `POST /v1/auth/admin/2fa`
 *      → TokenPair),
 *   3. it drives the REAL login form — email, password, then the six digit OTP
 *      boxes — so the session is created by the product's own auth path,
 *   4. it answers every `/v1/admin/**` read with one or two realistic rows,
 *      shaped exactly like the DTOs in `apps/web-admin/src/api/client.ts`,
 *   5. it screenshots each requested route, full page, into
 *      `output/mobile-review-admin-live/`.
 *
 * The routes reach the session through a deep link: the harness opens
 * `/admin/<screen>` while unauthenticated, the app redirects to `/admin/login`
 * with `state.from`, and after the OTP step the app navigates back to the
 * screen that was asked for. That is the shipped flow, not a shortcut.
 *
 * Flags
 *   --page   route path (repeatable; `/mail` and `/admin/mail` both accepted)
 *            default: every live admin route
 *   --size   WIDTHxHEIGHT (repeatable; default 390x844)
 *   --theme  dark | light | both                       (default both)
 *   --base   origin of the production dev server       (default http://localhost:5174)
 *   --out    output directory                          (default output/mobile-review-admin-live)
 *   --click  CSS selector to click after the page settles, before the shot
 *            (used to open a drawer / dialog, e.g. --click 'tbody tr')
 *   --label  filename suffix for that opened state (e.g. --label detail)
 *   --viewport  capture the viewport instead of the full page
 *   --help   print this text
 */
import { createRequire } from 'node:module';
import { mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');
const require = createRequire(join(REPO, 'apps/web-club/package.json'));
const { chromium } = require('playwright');

const LIVE_ROUTES = [
  '/admin/mail',
  '/admin/api-keys',
  '/admin/payments',
  '/admin/payouts',
  '/admin/audit-log',
  '/admin/clubs',
  '/admin/djs',
  '/admin/guests',
  '/admin/incidents',
  '/admin/platform-settings',
  '/admin/dashboard',
];

const HELP = `Admin live screenshots (production dev server, port 5174).

  node scripts/admin-live-shot.mjs [flags]

  --page PATH        route to shoot; repeatable. "/mail" and "/admin/mail" are
                     equivalent. Default: ${LIVE_ROUTES.join(', ')}
  --size WxH         viewport; repeatable. Default 390x844.
  --theme T          dark | light | both. Default both.
  --base URL         dev-server origin. Default http://localhost:5174
  --out DIR          output directory. Default output/mobile-review-admin-live
  --click SELECTOR   click one element after the page settles (opens a drawer,
                     a dialog or a menu before the shot)
  --label NAME       suffix for the output file, so an opened state does not
                     overwrite the plain page shot (e.g. --label detail)
  --viewport         shoot the viewport instead of the full page
  --help             this text

The production dev server must already be running:
  cd apps/web-admin && npm run dev
`;

const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log(HELP);
  process.exit(0);
}
const flag = (name, fallback) => {
  const index = args.indexOf(`--${name}`);
  return index >= 0 ? args[index + 1] : fallback;
};
const has = (name) => args.includes(`--${name}`);

/** Every value of a repeatable flag, in command-line order. */
const flagAll = (name) => {
  const values = [];
  args.forEach((value, index) => {
    if (value === `--${name}` && args[index + 1]) values.push(args[index + 1]);
  });
  return values;
};

const normalizeRoute = (value) => {
  const path = value.startsWith('/') ? value : `/${value}`;
  return path === '/admin' || path.startsWith('/admin/') ? path : `/admin${path}`;
};

const pages = (flagAll('page').length ? flagAll('page') : LIVE_ROUTES).map(normalizeRoute);

const sizes = flagAll('size').map((value) => {
  const [width, height] = value.split('x').map(Number);
  if (!width || !height) throw new Error(`--size expects WIDTHxHEIGHT, got "${value}"`);
  return { width, height };
});
if (sizes.length === 0) sizes.push({ width: 390, height: 844 });

const themes = flag('theme', 'both') === 'both' ? ['dark', 'light'] : [flag('theme', 'dark')];
const base = flag('base', 'http://localhost:5174').replace(/\/$/, '');
const outDir = resolve(REPO, flag('out', 'output/mobile-review-admin-live'));
const clickSelector = flag('click', null);
/** Filename suffix for a shot of an opened state (`--label detail`), so a
 *  drawer shot never overwrites the page shot it was opened from. */
const shotLabel = flag('label', '');
const fullPage = !has('viewport');

/** The account the fixtures accept. Nothing validates it; the login is intercepted. */
const CREDENTIALS = { email: 'ops@dropit.music', password: 'stand-passphrase-1', totp: '042913' };

// ---------------------------------------------------------------------------
// Fixtures — shaped after apps/web-admin/src/api/client.ts and api/developer.ts
// ---------------------------------------------------------------------------
// Every row carries every field the page (or the client's mapper) reads, so a
// screen renders its populated state instead of an honest empty one. Values are
// deliberately long where the real system has long values (UUID ids, e-mail
// addresses, provider errors): that is what overflows a phone card.

const TOKENS = {
  access_token: 'fixture-access-token',
  refresh_token: 'fixture-refresh-token',
  token_type: 'Bearer',
  expires_in: 900,
};

const MAIL_LIST = {
  items: [
    {
      id: '7f1c2b40-9a11-4d2e-8f03-1c5b6a7d8e90',
      to: 'nora.vex.booking@club-nebula.test',
      to_name: 'Nora Vex',
      subject: 'Your DROP IT DJ invite — Club Nebula',
      template: 'dj_invite',
      status: 'delivered',
      attempts: 1,
      max_attempts: 5,
      provider_id: 're_4kQ2mN8pX1',
      last_error: null,
      created_at: '2026-10-27T18:00:00Z',
      sent_at: '2026-10-27T18:00:04Z',
      delivered_at: '2026-10-27T18:00:09Z',
      aggregate_type: 'dj',
      aggregate_id: '3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01',
    },
    {
      id: '1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d',
      to: 'guest.2841@nebula.test',
      to_name: null,
      subject: 'Verify your guest account',
      template: 'guest_verify',
      status: 'queued',
      attempts: 0,
      max_attempts: 5,
      provider_id: null,
      last_error: null,
      created_at: '2026-10-27T17:58:12Z',
      sent_at: null,
      delivered_at: null,
      aggregate_type: 'guest_account',
      aggregate_id: '8b7a6c5d-4e3f-4a2b-9c1d-0e9f8a7b6c5d',
    },
    {
      id: '2b3c4d5e-6f7a-4b8c-9d0e-1f2a3b4c5d6e',
      to: 'payouts@club-nebula.test',
      to_name: 'Club Nebula',
      subject: 'Payout paid — €1,284.50 EUR',
      template: 'payout_paid',
      status: 'bounced',
      attempts: 5,
      max_attempts: 5,
      provider_id: 're_9xT3vB6qL2',
      last_error: '550 5.1.1 The email account that you tried to reach does not exist.',
      created_at: '2026-10-27T17:30:00Z',
      sent_at: '2026-10-27T17:30:02Z',
      delivered_at: null,
      aggregate_type: 'payout',
      aggregate_id: '5d4c3b2a-1f0e-4d9c-8b7a-6f5e4d3c2b1a',
    },
    {
      id: '3c4d5e6f-7a8b-4c9d-0e1f-2a3b4c5d6e7f',
      to: 'ines.duarte@nebula.test',
      to_name: 'Ines Duarte',
      subject: 'Your DROP IT DJ invite — Club Nebula',
      template: 'dj_invite',
      status: 'delivery_delayed',
      attempts: 2,
      max_attempts: 5,
      provider_id: 're_1pQ7wE4rT8',
      last_error: null,
      created_at: '2026-10-27T16:12:00Z',
      sent_at: '2026-10-27T16:12:03Z',
      delivered_at: null,
      aggregate_type: 'dj',
      aggregate_id: '6e5d4c3b-2a1f-4e0d-9c8b-7a6f5e4d3c2b',
    },
    {
      id: '4d5e6f7a-8b9c-4d0e-9f1a-2b3c4d5e6f7a',
      to: 'bounce@nebula.test',
      to_name: null,
      subject: 'Verify your guest account',
      template: 'guest_verify',
      status: 'failed',
      attempts: 5,
      max_attempts: 5,
      provider_id: null,
      last_error: 'resend 422 validation_error: domain nebula.test is not verified',
      created_at: '2026-10-27T15:40:00Z',
      sent_at: null,
      delivered_at: null,
      aggregate_type: 'guest_account',
      aggregate_id: '7f6e5d4c-3b2a-4f1e-8d9c-7b6a5f4e3d2c',
    },
  ],
  next_cursor: 'cursor_2',
};

const MAIL_DETAIL = {
  ...MAIL_LIST.items[0],
  html: '<!doctype html><html><body style="margin:0;padding:20px;font-family:system-ui,sans-serif"><h1 style="font-size:16px;margin:0 0 8px">You are invited</h1><p style="margin:0;font-size:13px">Club Nebula invited you to DJ.</p></body></html>',
  text: 'Club Nebula invited you to DJ.',
  vars: { club_name: 'Club Nebula', dj_name: 'Nora Vex' },
  events: [
    { id: 'evt-1', type: 'email.sent', received_at: '2026-10-27T18:00:04Z' },
    { id: 'evt-2', type: 'email.delivered', received_at: '2026-10-27T18:00:09Z' },
  ],
};

const API_KEYS = {
  items: [
    {
      id: 'key_01h8xk2m9p4q7r3s6t9v0w1x',
      club_id: '3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01',
      club_name: 'Club Nebula — Barcelona Eixample flagship venue',
      name: 'Ticketing bridge (production)',
      prefix: 'dpk_live_7f1c2b40',
      environment: 'live',
      scopes: ['requests:read', 'requests:write', 'sessions:read', 'earnings:read', 'webhooks:manage'],
      status: 'active',
      rate_limit_per_min: 120,
      allowed_cidrs: ['203.0.113.0/24'],
      expires_at: '2027-04-01T00:00:00Z',
      created_at: '2026-03-01T09:00:00Z',
      last_used_at: '2026-10-27T23:12:00Z',
      revoked_at: null,
      rotated_from: null,
      expired: false,
    },
    {
      id: 'key_01h8xk2n0q5r8s4t7u0w2y',
      club_id: 'd4c3b2a1-0f9e-4d8c-9b7a-6f5e4d3c2b1a',
      club_name: 'Studio 9 Lisbon',
      name: 'Door tablet (test)',
      prefix: 'dpk_test_1a2b3c4d',
      environment: 'test',
      scopes: ['requests:read'],
      status: 'revoked',
      rate_limit_per_min: 60,
      allowed_cidrs: [],
      expires_at: null,
      created_at: '2026-02-14T11:30:00Z',
      last_used_at: null,
      revoked_at: '2026-10-20T08:15:00Z',
      rotated_from: null,
      expired: false,
    },
    {
      id: 'key_01h8xk2p1r6s9t5u8v1x3z',
      club_id: '3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01',
      club_name: 'Club Nebula — Barcelona Eixample flagship venue',
      name: 'Reporting warehouse sync',
      prefix: 'dpk_live_9a8b7c6d',
      environment: 'live',
      scopes: ['requests:read', 'earnings:read'],
      status: 'active',
      rate_limit_per_min: 30,
      allowed_cidrs: [],
      expires_at: '2026-09-01T00:00:00Z',
      created_at: '2025-09-01T09:00:00Z',
      last_used_at: '2026-08-30T22:00:00Z',
      revoked_at: null,
      rotated_from: null,
      expired: true,
    },
  ],
};

const KEY_USAGE = {
  summary: { calls_24h: 1842, errors_24h: 7, calls_30d: 41208, last_call_at: '2026-10-27T23:12:00Z' },
  items: [
    { at: '2026-10-27T23:12:00Z', method: 'POST', path: '/v1/club/requests/req_01h8xk2m9p4q7r3s6t9v/accept', status: 200 },
    { at: '2026-10-27T23:11:40Z', method: 'GET', path: '/v1/club/requests?status=pending&limit=50', status: 200 },
    { at: '2026-10-27T23:10:02Z', method: 'POST', path: '/v1/club/webhooks/deliveries/8b7a6c5d-4e3f-4a2b-9c1d-0e9f8a7b6c5d/retry', status: 429 },
  ],
};

const PAYOUTS = {
  items: [
    {
      id: '8f3c1a72-5d64-4b9e-a1c0-3f2e7d6c5b4a',
      payee_type: 'club',
      payee_id: '3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01',
      amount_minor: 128450,
      status: 'paid',
      attempts: 1,
      is_final: true,
    },
    {
      id: 'a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d',
      payee_type: 'dj',
      payee_id: '6e5d4c3b-2a1f-4e0d-9c8b-7a6f5e4d3c2b',
      amount_minor: 47200,
      status: 'pending',
      attempts: 0,
      is_final: false,
    },
  ],
};

const PAYMENTS = {
  items: [
    { id: 'pay_01h8xk2m9p4q7r3s6t9v0w', request_id: 'req_01h8xk2m9p4q7r3s6t9v0w', amount_minor: 500, status: 'captured', created_at: '2026-10-27T22:14:00Z' },
    { id: 'pay_02h9yl3n0q5r8s4t7u0w1x', request_id: 'req_02h9yl3n0q5r8s4t7u0w1x', amount_minor: 2500, status: 'partially_refunded', created_at: '2026-10-27T21:58:00Z' },
    { id: 'pay_03h0zm4p1r6s9t5u8v1x2y', request_id: 'req_03h0zm4p1r6s9t5u8v1x2y', amount_minor: 1000, status: 'failed', created_at: '2026-10-27T21:40:00Z' },
  ],
};

const AUDIT = {
  items: [
    {
      id: 'aud_01h8xk2m9p4q7r3s6t9v',
      actor_type: 'admin',
      actor_id: 'a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d',
      action: 'club.status_changed',
      entity: 'club:3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01',
      at: '2026-10-27T19:03:00Z',
    },
    {
      id: 'aud_01h8xk2n0q5r8s4t7u0w',
      actor_type: 'dj',
      actor_id: '6e5d4c3b-2a1f-4e0d-9c8b-7a6f5e4d3c2b',
      action: 'request.accepted',
      entity: 'request:req_01h8xk2m9p4q7r3s6t9v0w',
      at: '2026-10-27T18:41:00Z',
    },
    {
      id: 'aud_01h8xk2p1r6s9t5u8v1x',
      actor_type: 'admin',
      actor_id: 'a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d',
      action: 'platform_settings.updated',
      entity: 'platform_settings:1',
      at: '2026-10-27T09:12:00Z',
    },
  ],
};

const CLUBS = {
  items: [
    {
      id: '3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01',
      name: 'Club Nebula',
      city: 'Barcelona',
      email: 'owner@club-nebula.test',
      status: 'active',
    },
    {
      id: 'd4c3b2a1-0f9e-4d8c-9b7a-6f5e4d3c2b1a',
      name: 'Studio 9 Lisbon',
      city: 'Lisbon',
      email: 'ops@studio9.test',
      status: 'suspended',
    },
  ],
};

const DJS = {
  items: [
    {
      id: 'dj_01h8xk2m9p4q7r3s6t9v',
      club_id: '3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01',
      email: 'nora.vex.booking@club-nebula.test',
      display_name: 'Nora Vex',
      status: 'active',
    },
    {
      id: 'dj_02h9yl3n0q5r8s4t7u0w',
      club_id: null,
      email: 'kaito.mori.residency@booking-agency.example',
      display_name: 'Kaito Mori',
      status: 'suspended',
    },
  ],
};

const GUESTS = {
  items: [
    {
      id: 'gst_01h8xk2m9p4q7r3s6t9v',
      display_name: 'Marta Puig',
      email: 'marta.puig.guest.account@example.test',
      created_at: '2026-09-14T20:02:00Z',
      requests_count: 17,
      last_seen_at: '2026-10-27T23:51:00Z',
      banned: false,
    },
    {
      id: 'gst_02h9yl3n0q5r8s4t7u0w',
      display_name: null,
      email: 'device.7c1f9a2b4e6d8f0a1b2c3d4e@example.test',
      created_at: '2026-10-02T22:18:00Z',
      requests_count: 3,
      last_seen_at: '2026-10-26T01:07:00Z',
      banned: true,
    },
  ],
};

const INCIDENTS = {
  items: [
    {
      id: 'inc_01h8xk2m9p4q7r3s6t9v',
      kind: 'payout_failed',
      message:
        'Stripe transfer to acct_1P8fK2LmN4oPqRs failed: account closed (code: account_closed). Retries exhausted after 5 attempts; the payout stays open for the next cycle.',
    },
    {
      id: 'inc_02h9yl3n0q5r8s4t7u0w',
      kind: 'reconciliation_discrepancy',
      message:
        'Reconciliation run rec_01h8xk2m9p found 2 captured payments without a matching ledger entry (2026-10-26).',
    },
  ],
};

const PLATFORM_SETTINGS = {
  commission_mode: 'percent',
  commission_percent_bp: 1200,
  commission_fixed_minor: 0,
  min_price_minor: 300,
  max_price_minor: 5000,
  dj_share_of_remainder_bp: 6000,
  currency: 'EUR',
  runtime_mode: 'auto',
  updated_at: '2026-10-27T09:12:00Z',
};

const DASHBOARD = {
  active_sessions: 14,
  requests_today: 327,
  requests_per_minute: 4,
  turnover_today_minor: 128450,
  currency: 'EUR',
  open_incidents: 2,
};

const SCOPE_CATALOGUE = {
  scopes: [
    { id: 'requests:read', title: 'Read requests', description: 'List and read song requests.', group: 'requests', risk: 'low', docs_section: 'requests' },
    { id: 'requests:write', title: 'Write requests', description: 'Accept, decline and cancel requests.', group: 'requests', risk: 'medium', docs_section: 'requests' },
  ],
  presets: [],
};

/**
 * One fixture for one request. Path matching is by suffix, in the order the
 * pages call them; `null` means "not a route this harness knows", which the
 * caller answers with an empty list instead of pretending.
 */
function fixtureFor(method, url) {
  const path = url.pathname;
  const json = (body) => ({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });

  // Auth handshake (unauthenticated paths).
  if (path === '/v1/auth/admin/login') return json({ mfa_required: true });
  if (path === '/v1/auth/admin/2fa') return json(TOKENS);
  if (path === '/v1/auth/refresh') return json(TOKENS);

  // The deployment strip is shell chrome, not an operator screen. Answering
  // `normal` keeps a stand-only band out of a production-panel review shot.
  if (path === '/v1/status') return json({ mode: 'normal', source: 'admin', open_signup: false, payments: 'live' });

  if (path === '/v1/admin/mail/stats') {
    return json({
      window_hours: 24,
      queued: 3,
      sending: 1,
      sent: 0,
      delivered: 118,
      bounced: 2,
      complained: 0,
      failed: 1,
      suppressed: 2,
      total: 125,
    });
  }
  if (path === '/v1/admin/mail/suppressions') {
    return json({
      items: [
        { email: 'bounce@nebula.test', reason: 'bounce', detail: '550 5.1.1 recipient does not exist', created_at: '2026-10-27T15:41:00Z' },
        { email: 'complaint@nebula.test', reason: 'complaint', detail: null, created_at: '2026-10-26T09:12:00Z' },
      ],
    });
  }
  if (/^\/v1\/admin\/mail\/[^/]+\/resend$/.test(path)) return json({ id: MAIL_LIST.items[0].id, status: 'queued' });
  if (/^\/v1\/admin\/mail\/[^/]+$/.test(path)) return json(MAIL_DETAIL);
  if (path === '/v1/admin/mail') return json(MAIL_LIST);

  if (/^\/v1\/admin\/api-keys\/[^/]+\/usage$/.test(path)) return json(KEY_USAGE);
  if (/^\/v1\/admin\/api-keys\/[^/]+\/revoke$/.test(path)) return { status: 204, body: '' };
  if (path === '/v1/admin/api-keys/scopes') return json(SCOPE_CATALOGUE);
  if (path === '/v1/admin/api-keys') return json(API_KEYS);

  if (path === '/v1/admin/payouts/run') return json({ payouts_created: 2, total_minor: 175650 });
  if (path === '/v1/admin/reconciliation/run') return json({ run_id: 'rec_01h8xk2m9p', status: 'ok' });
  if (path === '/v1/admin/payouts') return json(PAYOUTS);

  if (/^\/v1\/admin\/incidents\/[^/]+\/resolve$/.test(path)) return json({ resolved: true });
  if (path === '/v1/admin/incidents') return json(INCIDENTS);

  if (path === '/v1/admin/payments') return json(PAYMENTS);
  if (path === '/v1/admin/audit-log') return json(AUDIT);
  if (path === '/v1/admin/clubs') {
    if (method === 'POST') return json({ club_id: CLUBS.items[0].id, name: 'New Club', qr_master_token: 'qr_01h8xk2m9p4q7r3s6t9v' });
    return json(CLUBS);
  }
  if (/^\/v1\/admin\/clubs\/[^/]+$/.test(path)) return json({ id: CLUBS.items[0].id, status: 'active' });
  if (path === '/v1/admin/djs') return json(DJS);
  if (path === '/v1/admin/guests') return json(GUESTS);
  if (path === '/v1/admin/platform-settings') return json(PLATFORM_SETTINGS);
  if (path === '/v1/admin/dashboard/now') return json(DASHBOARD);

  // An endpoint this harness does not model: answer as an empty list so the
  // page renders its empty state instead of hanging on a request that never
  // resolves.
  if (method === 'GET') return json({ items: [] });
  return json({ ok: true });
}

// ---------------------------------------------------------------------------
// Browser work
// ---------------------------------------------------------------------------
mkdirSync(outDir, { recursive: true });

const browser = await chromium.launch({
  headless: true,
  args: ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'],
});

const written = [];
const failures = [];

/** The real login form: e-mail, password, then the six OTP boxes. */
async function signIn(page) {
  await page.waitForSelector('#email', { timeout: 30000 });
  await page.fill('#email', CREDENTIALS.email);
  await page.fill('#password', CREDENTIALS.password);
  await page.click('form button[type="submit"]');
  await page.waitForSelector('.auth-otp-input', { timeout: 30000 });
  await page.locator('.auth-otp-input').first().click();
  // The boxes advance as digits arrive (one `onInput` per keystroke), so a
  // single typed string fills all six exactly as a person would.
  await page.keyboard.type(CREDENTIALS.totp, { delay: 40 });
  await page.waitForTimeout(80);
  await page.click('form button[type="submit"]');
  await page.waitForURL((url) => !url.pathname.endsWith('/login'), { timeout: 30000 });
  await page.waitForSelector('.panel-page-head', { timeout: 30000 });
}

/** Let the data of the screen land: no in-flight requests, fonts, one beat. */
async function settle(page) {
  await page.waitForLoadState('networkidle').catch(() => {});
  await page.evaluate(() => document.fonts?.ready ?? true).catch(() => {});
  await page.waitForTimeout(250);
}

/**
 * Route-specific preparation that a user would do by hand. Only /admin/guests
 * needs it: the guest list is scoped to one club, so the screen opens empty
 * until a club is picked.
 */
async function prepare(page, route) {
  if (route !== '/admin/guests') return;
  await page.click('#guest-club');
  await page.waitForSelector('.field-select__menu li[role="option"]', { timeout: 10000 });
  // Option 0 is "All clubs" (empty value, no list). Option 1 is the first real
  // club, which is the populated state this shot exists for.
  await page.locator('.field-select__menu li[role="option"]').nth(1).click();
  await settle(page);
}

/**
 * The phone-width measurements the demo-mode audit (`script/panel-audit.mjs`)
 * cannot take, because it cannot reach these routes: does the document scroll
 * sideways, does a stacked table still overflow its card, does a sheet body
 * scroll sideways. Every value must be zero; anything else is reported per
 * shot and turns the run red.
 */
async function measure(page) {
  return page.evaluate(() => {
    const overflow = document.documentElement.scrollWidth - document.documentElement.clientWidth;
    const tables = [];
    for (const table of document.querySelectorAll('table.data-table--stack')) {
      const wrap = table.parentElement;
      if (!wrap) continue;
      const available = Math.ceil(wrap.getBoundingClientRect().width);
      if (table.scrollWidth > available + 1) {
        tables.push(`${table.getAttribute('aria-label') ?? 'table'} ${table.scrollWidth}px in ${available}px`);
      }
    }
    const sheets = [];
    for (const body of document.querySelectorAll('.admin-drawer-body, .panel-modal-body')) {
      if (body.scrollWidth > body.clientWidth + 1) {
        sheets.push(`${body.className.split(' ')[0]} ${body.scrollWidth}px in ${body.clientWidth}px`);
      }
    }
    return { overflow, tables, sheets };
  });
}

const slug = (route) => route.replace(/[^a-z0-9]+/gi, '-').replace(/^-|-$/g, '') || 'root';

for (const theme of themes) {
  for (const size of sizes) {
    for (const route of pages) {
      const context = await browser.newContext({
        viewport: { width: size.width, height: size.height },
        deviceScaleFactor: 1,
        reducedMotion: 'reduce',
      });
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror', (error) => errors.push(`pageerror: ${error.message}`));
      page.on('console', (message) => {
        if (message.type() === 'error') errors.push(message.text());
      });

      await page.addInitScript((value) => {
        try {
          window.localStorage.setItem('dropit-theme', value);
        } catch {
          /* ignore */
        }
      }, theme);

      await page.route('**/v1/**', async (route_) => {
        const request = route_.request();
        const url = new URL(request.url());
        try {
          const answer = fixtureFor(request.method(), url);
          if ('body' in answer && answer.status === 204) {
            return await route_.fulfill({ status: 204 });
          }
          return await route_.fulfill(answer);
        } catch {
          return route_.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
        }
      });

      const label = `${route} [${theme} ${size.width}x${size.height}]`;
      try {
        await page.goto(`${base}${route}`, { waitUntil: 'domcontentloaded', timeout: 60000 });
        await signIn(page);
        await settle(page);
        await prepare(page, route);
        if (clickSelector) {
          await page.click(clickSelector);
          await settle(page);
        }

        const width = await measure(page);
        const suffix = shotLabel ? `-${slug(shotLabel)}` : '';
        const file = join(outDir, `${slug(route)}-${theme}-${size.width}x${size.height}${suffix}.png`);
        await page.screenshot({ path: file, fullPage });
        written.push(file);
        const clean = width.overflow === 0 && width.tables.length === 0 && width.sheets.length === 0;
        console.log(`written: ${file}${clean ? ' — fits' : ` — OVERFLOW ${JSON.stringify(width)}`}`);

        if (!clean) {
          failures.push(`${label} overflow: ${JSON.stringify(width)}`);
        }

        const realErrors = errors.filter((text) => !/favicon|vite/i.test(text));
        if (realErrors.length) {
          failures.push(`${label} console: ${realErrors.slice(0, 3).join(' | ')}`);
        }
      } catch (error) {
        failures.push(`${label} failed: ${error instanceof Error ? error.message : String(error)}`);
      }
      await context.close();
    }
  }
}

await browser.close();

if (failures.length) {
  console.error(`\n${failures.length} screen(s) did not render cleanly:`);
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exitCode = 1;
} else {
  console.log(`\n${written.length} screenshot(s) written to ${outDir}`);
}
