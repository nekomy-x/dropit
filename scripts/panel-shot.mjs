#!/usr/bin/env node
/**
 * Panel screenshots — the verification tool for the light/dark redesign.
 *
 *   node scripts/panel-shot.mjs --app club --page /dashboard --theme light --size 1440x900
 *
 * Flags
 *   --app    club | admin | dj            (default club)
 *   --page   route path                   (default: /dashboard club, /admin/ admin, /dj dj)
 *   --theme  dark | light                 (default: both)
 *   --size   WIDTHxHEIGHT                 (default: 1440x900; repeatable)
 *   --out    directory                    (default: output/panel-review/<app>)
 *   --full   capture the full page instead of the viewport
 *   --api    populated | unavailable      (default: populated fixtures)
 *
 * The fixtures mirror the DTOs the panels actually call, so a screenshot shows
 * the populated state instead of the honest "unavailable" state a demo build
 * with no backend would show. They are test scaffolding, never product code.
 */
import { createRequire } from 'node:module';
import { mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');
const require = createRequire(join(REPO, 'apps/web-club/package.json'));
const { chromium } = require('playwright');

const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const index = args.indexOf(`--${name}`);
  return index >= 0 ? args[index + 1] : fallback;
};
const has = (name) => args.includes(`--${name}`);

const app = flag('app', 'club');
const sizes = args.reduce((acc, value, index) => {
  if (value === '--size' && args[index + 1]) {
    const [w, h] = args[index + 1].split('x').map(Number);
    if (w && h) acc.push({ width: w, height: h });
  }
  return acc;
}, []);
if (sizes.length === 0) sizes.push({ width: 1440, height: 900 });

const themes = flag('theme', 'both') === 'both' ? ['dark', 'light'] : [flag('theme', 'dark')];
const pages = (() => {
  const single = flag('page', null);
  if (single) {
    // The admin app is served under vite `base: '/admin/'`, so a route given
    // without the prefix (--page /mail) would 404 on the dev server. Accept
    // both spellings and address the real path.
    if (app === 'admin' && single !== '/admin' && !single.startsWith('/admin/')) {
      return [`/admin${single.startsWith('/') ? '' : '/'}${single}`];
    }
    return [single];
  }
  return app === 'admin' ? ['/admin/'] : app === 'dj' ? ['/dj'] : ['/dashboard'];
})();
const api = flag('api', 'populated');
const outDir = resolve(REPO, flag('out', `output/panel-review/${app}`));
const base = flag('base', app === 'admin' ? 'http://localhost:5174' : 'http://localhost:5173');

const FIXTURES = {
  dashboard: { session: { id: 'sess_01h8xk2m9p', status: 'active' }, requests_today: 47, earned_total_minor: 128450 },
  profile: {
    id: 'club-1', name: 'Club Nebula', slug: 'nebula', address: 'Carrer del Mar 12', country: 'ES',
    email: 'owner@nebula.test', phone: '+34 600 000 000', logo_url: null, stripe_account_id: null,
    stripe_onboarding_status: 'verified', status: 'active', created_at: '2026-01-01T00:00:00Z',
  },
  djs: {
    server_time: '2026-10-27T20:00:00Z',
    presence_ttl_seconds: 90,
    items: [
      {
        id: 'dj-1', display_name: 'Nora Vex', email: 'nora@nebula.test', status: 'active',
        note: 'Brings her own controller; vinyl only after 02:00.',
        last_seen_at: '2026-10-27T19:59:40Z', presence_age_seconds: 20,
        online: true, on_shift: true, open_session_id: 'sess_01h8xk2m9p',
        removed: false, removed_at: null, invite_expires_at: null,
        has_password: true, created_at: '2026-01-01T00:00:00Z',
      },
      {
        id: 'dj-2', display_name: 'Kaito Mori', email: 'kaito@nebula.test', status: 'suspended',
        note: null,
        last_seen_at: '2026-10-27T19:35:00Z', presence_age_seconds: 1500,
        online: false, on_shift: false, open_session_id: null,
        removed: false, removed_at: null, invite_expires_at: null,
        has_password: true, created_at: '2026-01-01T00:00:00Z',
      },
      {
        id: 'dj-3', display_name: 'Ines Duarte', email: 'ines@nebula.test', status: 'invited',
        note: null,
        last_seen_at: null, presence_age_seconds: null,
        online: false, on_shift: false, open_session_id: null,
        removed: false, removed_at: null, invite_expires_at: '2026-11-03T20:00:00Z',
        has_password: false, created_at: '2026-10-27T00:00:00Z',
      },
      {
        id: 'dj-4', display_name: 'Marco Silva', email: 'marco@nebula.test', status: 'suspended',
        note: 'Left the club in September.',
        last_seen_at: '2026-09-20T23:00:00Z', presence_age_seconds: 3196800,
        online: false, on_shift: false, open_session_id: null,
        removed: true, removed_at: '2026-09-30T10:00:00Z', invite_expires_at: null,
        has_password: true, created_at: '2026-02-01T00:00:00Z',
      },
    ],
  },
  pricing: {
    items: [
      { id: 'price-1', club_id: 'club-1', price_minor: 500, dj_share_bp: null, valid_from: '2026-10-01T00:00:00Z', valid_to: null, created_at: '2026-10-01T00:00:00Z' },
      { id: 'price-0', club_id: 'club-1', price_minor: 300, dj_share_bp: null, valid_from: '2026-08-01T00:00:00Z', valid_to: '2026-10-01T00:00:00Z', created_at: '2026-08-01T00:00:00Z' },
    ],
  },
  qr: { session_id: 'sess_01h8xk2m9p', qr_token: 'a1b2c3d4e5f60718293a4b5c6d7e8f90', expires_at: '2026-10-27T03:30:00Z' },
  earnings: { period: 'day', gross_minor: 183500, net_minor: 128450, payments: 47 },
  // Admin mail (docs/mail-infra-contract.md §7): the same snake_case DTO the
  // list/stats/suppressions routes return, so an `--app admin --page /mail`
  // shot shows the populated screen instead of a demo build's error state.
  mail: {
    stats: {
      window_hours: 24, queued: 3, sending: 1, sent: 0, delivered: 118,
      bounced: 2, complained: 0, failed: 1, suppressed: 2, total: 125,
    },
    list: {
      items: [
        { id: '7f1c2b40-9a11-4d2e-8f03-1c5b6a7d8e90', to: 'nora@nebula.test', to_name: 'Nora Vex', subject: 'Your DROP IT DJ invite', template: 'dj_invite', status: 'delivered', attempts: 1, max_attempts: 5, provider_id: 're_4kQ2mN8pX1', last_error: null, created_at: '2026-10-27T18:00:00Z', sent_at: '2026-10-27T18:00:04Z', delivered_at: '2026-10-27T18:00:09Z', aggregate_type: 'dj', aggregate_id: '3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01' },
        { id: '1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d', to: 'guest.2841@nebula.test', to_name: null, subject: 'Verify your guest account', template: 'guest_verify', status: 'queued', attempts: 0, max_attempts: 5, provider_id: null, last_error: null, created_at: '2026-10-27T17:58:12Z', sent_at: null, delivered_at: null, aggregate_type: 'guest_account', aggregate_id: '8b7a6c5d-4e3f-4a2b-9c1d-0e9f8a7b6c5d' },
        { id: '2b3c4d5e-6f7a-4b8c-9d0e-1f2a3b4c5d6e', to: 'payouts@club-nebula.test', to_name: 'Club Nebula', subject: 'Payout paid — 128.45 EUR', template: 'payout_paid', status: 'bounced', attempts: 5, max_attempts: 5, provider_id: 're_9xT3vB6qL2', last_error: '550 5.1.1 The email account that you tried to reach does not exist.', created_at: '2026-10-27T17:30:00Z', sent_at: '2026-10-27T17:30:02Z', delivered_at: null, aggregate_type: 'payout', aggregate_id: '5d4c3b2a-1f0e-4d9c-8b7a-6f5e4d3c2b1a' },
        { id: '3c4d5e6f-7a8b-4c9d-0e1f-2a3b4c5d6e7f', to: 'ines@nebula.test', to_name: 'Ines Duarte', subject: 'Your DROP IT DJ invite', template: 'dj_invite', status: 'delivery_delayed', attempts: 2, max_attempts: 5, provider_id: 're_1pQ7wE4rT8', last_error: null, created_at: '2026-10-27T16:12:00Z', sent_at: '2026-10-27T16:12:03Z', delivered_at: null, aggregate_type: 'dj', aggregate_id: '6e5d4c3b-2a1f-4e0d-9c8b-7a6f5e4d3c2b' },
        { id: '4d5e6f7a-8b9c-4d0e-9f1a-2b3c4d5e6f7a', to: 'bounce@nebula.test', to_name: null, subject: 'Verify your guest account', template: 'guest_verify', status: 'failed', attempts: 5, max_attempts: 5, provider_id: null, last_error: 'resend 422 validation_error: domain nebula.test is not verified', created_at: '2026-10-27T15:40:00Z', sent_at: null, delivered_at: null, aggregate_type: 'guest_account', aggregate_id: '7f6e5d4c-3b2a-4f1e-8d9c-7b6a5f4e3d2c' },
      ],
      next_cursor: 'cursor_2',
    },
    detail: {
      id: '7f1c2b40-9a11-4d2e-8f03-1c5b6a7d8e90', to: 'nora@nebula.test', to_name: 'Nora Vex',
      subject: 'Your DROP IT DJ invite', template: 'dj_invite', status: 'delivered',
      attempts: 1, max_attempts: 5, provider_id: 're_4kQ2mN8pX1', last_error: null,
      created_at: '2026-10-27T18:00:00Z', sent_at: '2026-10-27T18:00:04Z',
      delivered_at: '2026-10-27T18:00:09Z', aggregate_type: 'dj',
      aggregate_id: '3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01',
      html: '<!doctype html><html><body style="margin:0;padding:20px;font-family:system-ui,sans-serif"><h1 style="font-size:16px;margin:0 0 8px">You are invited</h1><p style="margin:0;font-size:13px">Club Nebula invited you to DJ.</p></body></html>',
      text: 'Club Nebula invited you to DJ.',
      vars: { club_name: 'Club Nebula', dj_name: 'Nora Vex' },
      events: [
        { id: 'evt-1', type: 'email.sent', received_at: '2026-10-27T18:00:04Z' },
        { id: 'evt-2', type: 'email.delivered', received_at: '2026-10-27T18:00:09Z' },
      ],
    },
    suppressions: {
      items: [
        { email: 'bounce@nebula.test', reason: 'bounce', detail: '550 5.1.1 recipient does not exist', created_at: '2026-10-27T15:41:00Z' },
        { email: 'complaint@nebula.test', reason: 'complaint', detail: null, created_at: '2026-10-26T09:12:00Z' },
      ],
    },
  },
};

mkdirSync(outDir, { recursive: true });

// A real browser rasterises WebGL on the GPU; headless Chromium without a
// rasteriser returns a live context that draws nothing, which would make every
// 3D chart screenshot blank. SwiftShader is the software path Chrome itself
// falls back to, so screenshots show what a user with a blocked GPU sees.
const browser = await chromium.launch({
  headless: true,
  args: ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'],
});
const written = [];

for (const page$ of pages) {
  for (const theme of themes) {
    for (const size of sizes) {
      const context = await browser.newContext({ viewport: size, deviceScaleFactor: 1, reducedMotion: 'reduce' });
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror', (error) => errors.push(error.message));
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

      if (api === 'populated') {
        await page.route('**/v1/club/**', async (route) => {
          const url = route.request().url();
          const json = (body) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
          if (url.includes('/club/dashboard')) return json(FIXTURES.dashboard);
          if (url.includes('/club/profile')) return json(FIXTURES.profile);
          if (url.includes('/club/djs')) return json(FIXTURES.djs);
          if (url.includes('/club/pricing')) return json(FIXTURES.pricing);
          if (url.includes('/club/earnings')) return json(FIXTURES.earnings);
          if (url.includes('/qr/generate')) return json(FIXTURES.qr);
          return json({});
        });

        await page.route('**/v1/admin/mail**', async (route) => {
          const url = route.request().url();
          const json = (body) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
          if (url.includes('/mail/stats')) return json(FIXTURES.mail.stats);
          if (url.includes('/mail/suppressions')) return json(FIXTURES.mail.suppressions);
          if (/\/mail\/[^/?]+$/.test(url)) return json(FIXTURES.mail.detail);
          return json(FIXTURES.mail.list);
        });
      }

      const url = `${base}${page$}`;
      await page.goto(url, { waitUntil: 'networkidle', timeout: 60000 });
      // The 3D chart is a lazy chunk + a WebGL first frame; give it a beat so
      // screenshots show the chart rather than an empty plot box.
      await page.waitForTimeout(1800);
      const name = `${page$.replace(/[^a-z0-9]+/gi, '-').replace(/^-|-$/g, '') || 'root'}-${theme}-${size.width}x${size.height}.png`;
      const file = join(outDir, name);
      await page.screenshot({ path: file, fullPage: has('full') });
      written.push(file);
      if (errors.length) console.log(`console errors on ${url} (${theme}): ${errors.slice(0, 4).join(' | ')}`);
      await context.close();
    }
  }
}

await browser.close();
for (const file of written) console.log(file);
