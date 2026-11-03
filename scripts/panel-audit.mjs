#!/usr/bin/env node
/**
 * Panel audit — the same acceptance questions as `club-ui-check.mjs`, asked of
 * EVERY panel in BOTH themes.
 *
 *   node scripts/panel-audit.mjs                     # club + admin + dj
 *   node scripts/panel-audit.mjs --app club --route /dashboard
 *
 * It measures the rendered page (computed styles, not source) and reports
 * failures per route × theme:
 *
 *   1. text contrast: >= 4.5:1, or >= 3:1 at >= 24px
 *   2. horizontal overflow at 320 / 390 / 768 / 1024 / 1440
 *   3. radius set is a subset of {0, 8, 10, 12, 16, 999}
 *   4. at most 6 distinct font sizes, none below 11px
 *   5. touch targets >= 44px below 640px
 *   6. a visible focus ring on the first 8 tab stops
 *   7. no request to an external host (icons/fonts are bundled)
 *   8. no console.error / pageerror
 *
 * Exit code 0 = every route passed in both themes.
 */
import { createRequire } from 'node:module';
import { signInPanelIfNeeded } from './panel-test-auth.mjs';
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
const onlyApp = flag('app', null);
const onlyRoute = flag('route', null);

const APPS = {
  club: {
    base: flag('club-base', 'http://localhost:5173'),
    routes: ['/dashboard', '/sessions', '/djs', '/pricing', '/earnings', '/dj', '/dj/login', '/login', '/register', '/club/invite/preview-token', '/dj/invite/preview-token'],
  },
  admin: {
    base: flag('admin-base', 'http://localhost:5174'),
    routes: [
      // control center (local-first preview), each a real route
      '/admin/', '/admin/manage/users', '/admin/manage/djs', '/admin/manage/clubs',
      '/admin/manage/transactions', '/admin/manage/activity', '/admin/manage/access',
      '/admin/manage/settings',
      // live admin screens backed by the platform API
      '/admin/dashboard', '/admin/clubs', '/admin/djs', '/admin/guests', '/admin/payments',
      '/admin/payouts', '/admin/incidents', '/admin/audit-log', '/admin/platform-settings',
      '/admin/mail', '/admin/onboarding',
    ],
  },
};

const THEMES = ['dark', 'light'];
const WIDTHS = [320, 390, 768, 1024, 1440];
// 4px is --r-mark: a checkbox is a mark, not a container (see tokens.css).
const ALLOWED_RADII = new Set(['0px', '4px', '8px', '10px', '12px', '16px', '999px']);
const FIXTURES = {
  dashboard: { session: { id: 'sess_01h8xk2m9p', status: 'active' }, requests_today: 47, earned_total_minor: 128450 },
  profile: { id: 'club-1', name: 'Club Nebula', slug: 'nebula', address: 'Carrer del Mar 12', country: 'ES', email: 'owner@nebula.test', phone: '+34 600 000 000', logo_url: null, stripe_account_id: null, stripe_onboarding_status: 'verified', status: 'active', created_at: '2026-01-01T00:00:00Z' },
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
  pricing: { items: [{ id: 'price-1', club_id: 'club-1', price_minor: 500, dj_share_bp: null, valid_from: '2026-10-01T00:00:00Z', valid_to: null, created_at: '2026-10-01T00:00:00Z' }] },
  qr: { session_id: 'sess_01h8xk2m9p', qr_token: 'a1b2c3d4e5f60718293a4b5c6d7e8f90', expires_at: '2026-10-27T03:30:00Z' },
  earnings: { period: 'day', gross_minor: 183500, net_minor: 128450, payments: 47 },
  // Closed onboarding (docs/onboarding-contract.md): an approved application
  // reads back its own facts, so the activation screen shows the real rows
  // instead of the 404 state.
  clubInvite: { club_name: 'Barrio Latino', city: 'Madrid', email: 'owner@barrio.test', contact: 'owner@barrio.test', slug: 'barrio-latino', expires_at: '2026-11-05T12:00:00Z' },
  djInvite: { display_name: 'DJ Noche', club_name: 'Barrio Latino', expires_at: '2026-11-05T12:00:00Z' },
  onboarding: {
    list: {
      items: [
        { id: '7f1c2b40-9a11-4d2e-8f03-1c5b6a7d8e90', club_name: 'Barrio Latino', city: 'Madrid', contact: 'owner@barrio.test', price_minor: 500, status: 'pending_moderation', created_at: '2026-10-27T18:00:00Z', reviewed_at: null, club_id: null, invite_state: 'none' },
        { id: '1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d', club_name: 'Vinyl Room', city: 'Barcelona', contact: 'hello@vinyl.test', price_minor: 700, status: 'approved', created_at: '2026-10-26T12:00:00Z', reviewed_at: '2026-10-26T13:00:00Z', club_id: '3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01', invite_state: 'pending' },
      ],
      next_cursor: null,
    },
    detail: {
      id: '7f1c2b40-9a11-4d2e-8f03-1c5b6a7d8e90',
      club: { name: 'Barrio Latino', city: 'Madrid', address: 'Calle Mayor 1', website: 'https://barrio.test', contact: 'owner@barrio.test', genres: ['Techno', 'House'] },
      venue: { name: 'Barrio Latino', type: 'club', address: 'Calle Mayor 1', city: 'Madrid', district: 'Centro', capacity: 400, seats: 120, area_m2: 600 },
      qr: { price_minor: 500, zones: ['Main floor', 'Terrace'] },
      club_name: 'Barrio Latino', city: 'Madrid', contact: 'owner@barrio.test', email: 'owner@barrio.test',
      price_minor: 500, status: 'pending_moderation', reject_reason: null, reviewed_by: null, reviewed_at: null,
      created_at: '2026-10-27T18:00:00Z', source_ip: '203.0.113.7', club_id: null, invite_state: 'none',
    },
  },
  // Mail (docs/mail-infra-contract.md §7). Rows are the exact snake_case DTO
  // the admin list returns, so the screen is measured against real shape.
  mail: {
    stats: {
      window_hours: 24, queued: 3, sending: 1, sent: 0, delivered: 118,
      bounced: 2, complained: 0, failed: 1, suppressed: 2, total: 125,
    },
    list: {
      items: [
        {
          id: '7f1c2b40-9a11-4d2e-8f03-1c5b6a7d8e90', to: 'nora@nebula.test', to_name: 'Nora Vex',
          subject: 'Your DROP IT DJ invite', template: 'dj_invite', status: 'delivered',
          attempts: 1, max_attempts: 5, provider_id: 're_4kQ2mN8pX1', last_error: null,
          created_at: '2026-10-27T18:00:00Z', sent_at: '2026-10-27T18:00:04Z',
          delivered_at: '2026-10-27T18:00:09Z', aggregate_type: 'dj',
          aggregate_id: '3c9e1f70-2b4a-4c8d-9e10-5a6b7c8d9e01',
        },
        {
          id: '1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d', to: 'guest.2841@nebula.test', to_name: null,
          subject: 'Verify your guest account', template: 'guest_verify', status: 'queued',
          attempts: 0, max_attempts: 5, provider_id: null, last_error: null,
          created_at: '2026-10-27T17:58:12Z', sent_at: null, delivered_at: null,
          aggregate_type: 'guest_account', aggregate_id: '8b7a6c5d-4e3f-4a2b-9c1d-0e9f8a7b6c5d',
        },
        {
          id: '2b3c4d5e-6f7a-4b8c-9d0e-1f2a3b4c5d6e', to: 'payouts@club-nebula.test', to_name: 'Club Nebula',
          subject: 'Payout paid — 128.45 EUR', template: 'payout_paid', status: 'bounced',
          attempts: 5, max_attempts: 5, provider_id: 're_9xT3vB6qL2',
          last_error: '550 5.1.1 The email account that you tried to reach does not exist.',
          created_at: '2026-10-27T17:30:00Z', sent_at: '2026-10-27T17:30:02Z',
          delivered_at: null, aggregate_type: 'payout',
          aggregate_id: '5d4c3b2a-1f0e-4d9c-8b7a-6f5e4d3c2b1a',
        },
        {
          id: '3c4d5e6f-7a8b-4c9d-0e1f-2a3b4c5d6e7f', to: 'ines@nebula.test', to_name: 'Ines Duarte',
          subject: 'Your DROP IT DJ invite', template: 'dj_invite', status: 'delivery_delayed',
          attempts: 2, max_attempts: 5, provider_id: 're_1pQ7wE4rT8', last_error: null,
          created_at: '2026-10-27T16:12:00Z', sent_at: '2026-10-27T16:12:03Z',
          delivered_at: null, aggregate_type: 'dj',
          aggregate_id: '6e5d4c3b-2a1f-4e0d-9c8b-7a6f5e4d3c2b',
        },
        {
          id: '4d5e6f7a-8b9c-4d0e-9f1a-2b3c4d5e6f7a', to: 'bounce@nebula.test', to_name: null,
          subject: 'Verify your guest account', template: 'guest_verify', status: 'failed',
          attempts: 5, max_attempts: 5, provider_id: null,
          last_error: 'resend 422 validation_error: domain nebula.test is not verified',
          created_at: '2026-10-27T15:40:00Z', sent_at: null, delivered_at: null,
          aggregate_type: 'guest_account', aggregate_id: '7f6e5d4c-3b2a-4f1e-8d9c-7b6a5f4e3d2c',
        },
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

const results = [];
let failures = 0;
const record = (ok, label, detail) => {
  results.push({ ok, label, detail });
  if (!ok) failures += 1;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail ? `\n         ${detail}` : ''}`);
};

const measure = (page) => page.evaluate(() => {
  const parseColor = (value) => {
    const match = value.match(/rgba?\(([^)]+)\)/);
    if (match) {
      const parts = match[1].split(',').map(Number);
      return { r: parts[0], g: parts[1], b: parts[2], a: parts.length > 3 ? parts[3] : 1 };
    }
    // `color-mix()` resolves to the modern `color(srgb r g b / a)` form, with
    // components in 0–1. Without this branch the walk below skipped every such
    // surface and measured the text against an ancestor instead, so a
    // `color-mix` band was silently excluded from the contrast check rather
    // than measured. The deployment strip is one.
    const srgb = value.match(/color\(srgb\s+([^)]+)\)/);
    if (srgb) {
      const parts = srgb[1].split(/[\s/]+/).filter(Boolean).map(Number);
      return {
        r: Math.round(parts[0] * 255),
        g: Math.round(parts[1] * 255),
        b: Math.round(parts[2] * 255),
        a: parts.length > 3 ? parts[3] : 1,
      };
    }
    return null;
  };
  const luminance = ({ r, g, b }) => {
    const channel = (v) => {
      const s = v / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
  };
  const blend = (fg, bg) => ({
    r: fg.r * fg.a + bg.r * (1 - fg.a),
    g: fg.g * fg.a + bg.g * (1 - fg.a),
    b: fg.b * fg.a + bg.b * (1 - fg.a),
    a: 1,
  });
  const backgroundOf = (el) => {
    let node = el;
    while (node) {
      const color = parseColor(getComputedStyle(node).backgroundColor);
      if (color && color.a > 0.5) return color;
      node = node.parentElement;
    }
    const body = parseColor(getComputedStyle(document.body).backgroundColor);
    return body && body.a > 0.5 ? body : { r: 255, g: 255, b: 255, a: 1 };
  };
  const visible = (el) => {
    const style = getComputedStyle(el);
    const rect = el.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && Number(style.opacity) > 0 && rect.width > 0 && rect.height > 0;
  };

  const radii = new Map();
  const fontSizes = new Map();
  const contrasts = [];
  const smallTargets = [];
  const unnamed = [];

  for (const el of document.body.querySelectorAll('*')) {
    if (!(el instanceof HTMLElement)) continue;
    const style = getComputedStyle(el);
    const rect = el.getBoundingClientRect();
    if (style.borderRadius && style.borderRadius !== '0px' && visible(el)) {
      radii.set(style.borderRadius, (radii.get(style.borderRadius) ?? 0) + 1);
    }
    const ownText = [...el.childNodes].some((node) => node.nodeType === Node.TEXT_NODE && node.textContent.trim());
    if (ownText && visible(el)) {
      const size = Math.round(parseFloat(style.fontSize));
      fontSizes.set(size, (fontSizes.get(size) ?? 0) + 1);
      const fg = parseColor(style.color);
      if (fg) {
        const bg = backgroundOf(el);
        const fgRgb = blend(fg, bg);
        const l1 = luminance(fgRgb);
        const l2 = luminance(bg);
        const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
        contrasts.push({ ratio: Math.round(ratio * 100) / 100, size, text: el.textContent.trim().slice(0, 36), cls: String(el.className).slice(0, 40) });
      }
    }
    // Headings with tabindex=-1 receive programmatic focus after a step
    // change; they are not touch controls. Native/ARIA buttons still count.
    const interactive = ['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName)
      || el.getAttribute('role') === 'button' || (el.hasAttribute('tabindex') && el.tabIndex >= 0);
    if (interactive && visible(el)) {
      const name = el.getAttribute('aria-label')
        || el.getAttribute('title')
        || (el.id ? document.querySelector(`label[for="${CSS.escape(el.id)}"]`)?.textContent : '')
        || el.textContent
        || el.getAttribute('placeholder')
        || '';
      if (!String(name).trim()) unnamed.push(`${el.tagName.toLowerCase()}.${String(el.className).slice(0, 28)}`);
      if (rect.width < 44 || rect.height < 44) {
        const inlineLink = el.tagName === 'A' && el.closest('p, li, span, td, th, label') !== null;
        if (!inlineLink) smallTargets.push(`${el.tagName.toLowerCase()} ${Math.round(rect.width)}x${Math.round(rect.height)} "${String(name).trim().slice(0, 20)}"`);
      }
    }
  }

  contrasts.sort((a, b) => a.ratio - b.ratio);
  return {
    overflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    radii: Object.fromEntries(radii),
    fontSizes: Object.fromEntries(fontSizes),
    contrastMin: contrasts[0] ?? null,
    contrastFailures: contrasts.filter((item) => item.ratio < (item.size >= 24 ? 3 : 4.5)),
    smallTargets,
    unnamed,
    bodyBg: getComputedStyle(document.body).backgroundColor,
    bodyColor: getComputedStyle(document.body).color,
  };
});

// A real browser rasterises WebGL on the GPU; headless Chromium without a
// rasteriser returns a live context that draws nothing, which would make every
// 3D chart screenshot blank. SwiftShader is the software path Chrome itself
// falls back to, so screenshots show what a user with a blocked GPU sees.
const browser = await chromium.launch({
  headless: true,
  args: ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'],
});

for (const [appName, config] of Object.entries(APPS)) {
  if (onlyApp && onlyApp !== appName) continue;
  const routes = onlyRoute ? [onlyRoute] : config.routes;

  for (const route of routes) {
    for (const theme of THEMES) {
      const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
      const page = await context.newPage();
      await page.route('**/v1/status', (route) => route.fulfill({
        json: { mode: 'test', source: 'adaptive', open_signup: true, payments: 'mock' },
      }));
      const consoleErrors = [];
      const externalHosts = new Set();
      const catalogImageHosts = new Set();
      page.on('console', (message) => { if (message.type() === 'error') consoleErrors.push(message.text()); });
      page.on('pageerror', (error) => consoleErrors.push(`pageerror: ${error.message}`));
      page.on('request', (request) => {
        try {
          const url = new URL(request.url());
          if (!['localhost', '127.0.0.1', new URL(config.base).hostname].includes(url.hostname)) {
            // Live catalog artwork is product content; UI fonts, scripts and
            // icons must still be first-party. Never exempt other resource types.
            if (request.resourceType() === 'image' && /^is\d+-ssl\.mzstatic\.com$/.test(url.hostname)) catalogImageHosts.add(url.hostname);
            else externalHosts.add(url.hostname);
          }
        } catch { /* ignore */ }
      });

      await page.addInitScript((value) => {
        try { window.localStorage.setItem('dropit-theme', value); } catch { /* ignore */ }
      }, theme);

      if (appName === 'club') {
        await page.route('**/v1/club/**', async (r) => {
          const url = r.request().url();
          const json = (body) => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
          if (url.includes('/club/onboarding/invite/')) return json(FIXTURES.clubInvite);
          if (url.includes('/club/dashboard')) return json(FIXTURES.dashboard);
          if (url.includes('/club/profile')) return json(FIXTURES.profile);
          if (url.includes('/club/djs')) return json(FIXTURES.djs);
          if (url.includes('/club/pricing')) return json(FIXTURES.pricing);
          if (url.includes('/club/earnings')) return json(FIXTURES.earnings);
          if (url.includes('/qr/generate')) return json(FIXTURES.qr);
          return json({});
        });
        await page.route('**/v1/dj/invite/**', async (r) => {
          r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(FIXTURES.djInvite) });
        });
      }

      if (appName === 'admin') {
        // The mail screen is a live API surface. Fixtures mirror
        // docs/mail-infra-contract.md §7 so the audit measures the populated
        // screen (table, chips, pagination) instead of an error state.
        await page.route('**/v1/admin/mail**', async (r) => {
          const url = r.request().url();
          const json = (body) => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
          if (url.includes('/mail/stats')) return json(FIXTURES.mail.stats);
          if (url.includes('/mail/suppressions')) return json(FIXTURES.mail.suppressions);
          if (/\/mail\/[^/?]+$/.test(url)) return json(FIXTURES.mail.detail);
          return json(FIXTURES.mail.list);
        });
        // Applications queue (docs/onboarding-contract.md). Populated rows so
        // the audit measures the table, the status chips and the approve panel
        // instead of an empty state.
        await page.route('**/v1/admin/onboarding-applications**', async (r) => {
          const url = r.request().url();
          const json = (body) => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
          if (/\/onboarding-applications\/[^/?]+$/.test(url)) return json(FIXTURES.onboarding.detail);
          return json(FIXTURES.onboarding.list);
        });
      }

      const label = `${appName} ${route} [${theme}]`;
      try {
        await page.goto(`${config.base}${route}`, { waitUntil: 'networkidle', timeout: 60000 });
        await signInPanelIfNeeded(page, appName === 'admin' ? 'admin' : route === '/dj' ? 'dj' : 'club', route);
        await page.waitForTimeout(500);
        const data = await measure(page);

        const wrongTheme = await page.evaluate((expected) => document.documentElement.dataset.theme === expected, theme);
        record(wrongTheme, `${label} theme applied`, wrongTheme ? `data-theme=${theme}` : 'attribute missing');

        record(data.contrastFailures.length === 0, `${label} contrast >= AA`,
          data.contrastFailures.length ? `${data.contrastFailures.length} below AA, worst: ${JSON.stringify(data.contrastFailures.slice(0, 3))}` : `min ${data.contrastMin?.ratio}:1 on "${data.contrastMin?.text}"`);

        const strayRadii = Object.keys(data.radii).filter((radius) => !ALLOWED_RADII.has(radius));
        record(strayRadii.length === 0, `${label} radius set`, strayRadii.length ? `stray: ${strayRadii.join(', ')}` : JSON.stringify(data.radii));

        const sizes = Object.keys(data.fontSizes).map(Number).sort((a, b) => a - b);
        const tooSmall = sizes.filter((size) => size < 11);
        record(sizes.length <= 6 && tooSmall.length === 0, `${label} <= 6 font sizes, none < 11px`,
          `${sizes.length} sizes ${JSON.stringify(sizes)}${tooSmall.length ? ` — below: ${tooSmall}` : ''}`);

        record(consoleErrors.length === 0, `${label} no console errors`, consoleErrors.slice(0, 3).join(' | ') || 'clean');
        record(externalHosts.size === 0, `${label} no external UI assets`, [...externalHosts].join(', ') || (catalogImageHosts.size ? `first-party UI; catalog artwork: ${[...catalogImageHosts].join(', ')}` : 'all first-party'));

        // overflow ladder at the current theme
        const overflows = {};
        for (const width of WIDTHS) {
          await page.setViewportSize({ width, height: 900 });
          await page.waitForTimeout(180);
          overflows[width] = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
        }
        const overflowing = Object.entries(overflows).filter(([, value]) => value > 0);
        record(overflowing.length === 0, `${label} no horizontal overflow 320–1440`,
          overflowing.length ? JSON.stringify(overflows) : JSON.stringify(overflows));

        // touch targets + focus ring at 390
        await page.setViewportSize({ width: 390, height: 844 });
        await page.waitForTimeout(200);
        const mobile = await measure(page);
        record(mobile.smallTargets.length === 0, `${label} touch targets >= 44px @390`,
          mobile.smallTargets.slice(0, 5).join(' | ') || 'none under 44px');
        record(mobile.unnamed.length === 0, `${label} accessible names`, mobile.unnamed.slice(0, 4).join(' | ') || 'all named');

        // Tables on a phone. A dense data table is a desktop instrument; below
        // 768px it either becomes a list of labelled rows (`data-table--stack`)
        // or it must declare that it is a genuine matrix that scrolls sideways
        // (`data-mobile="scroll"`). A table that does neither is a 760px grid
        // inside a 326px card: the columns past the fold are silently clipped,
        // which is the exact bug this check exists for. A stacked table must
        // label every value cell, otherwise the value loses its column.
        const tables = await page.evaluate(() => {
          const clipped = [];
          const unlabelled = [];
          let stacked = 0;
          for (const table of document.querySelectorAll('table.data-table')) {
            const rect = table.getBoundingClientRect();
            if (rect.width === 0 || rect.height === 0) continue;
            if (table.classList.contains('data-table--stack')) {
              stacked += 1;
              for (const cell of table.querySelectorAll('tbody td')) {
                if (cell.closest('[hidden]')) continue;
                if (!cell.hasAttribute('data-label') && !cell.classList.contains('cell-primary')) {
                  unlabelled.push(cell.textContent.trim().slice(0, 24) || '(empty cell)');
                }
              }
              continue;
            }
            if (table.getAttribute('data-mobile') === 'scroll') continue;
            const wrap = table.parentElement;
            if (!wrap) continue;
            const available = Math.ceil(wrap.getBoundingClientRect().width);
            if (table.scrollWidth > available + 1) {
              clipped.push(`${table.getAttribute('aria-label') || String(table.className).slice(0, 30)} ${table.scrollWidth}px in ${available}px`);
            }
          }
          return { clipped, unlabelled, stacked };
        });
        record(tables.clipped.length === 0, `${label} mobile tables fit or declare scroll @390`,
          tables.clipped.length ? tables.clipped.slice(0, 4).join(' | ') : `${tables.stacked} stacked, rest fit`);
        record(tables.unlabelled.length === 0, `${label} stacked rows carry column labels @390`,
          tables.unlabelled.slice(0, 5).join(' | ') || 'every value cell labelled');


        // Walk the tab order, but stop at the end of the document rather than
        // pressing a fixed number of times: `body` is `document.activeElement`
        // whenever focus has left the page, and on a page with fewer than eight
        // focusable controls the eighth Tab wraps to `body` and reported a
        // missing ring on a page where every real control rings correctly.
        const focus = [];
        for (let index = 0; index < 8; index += 1) {
          await page.keyboard.press('Tab');
          const result = await page.evaluate(() => {
            const el = document.activeElement;
            if (!el || el === document.body || el === document.documentElement) {
              return { end: true, ok: true, tag: el?.tagName ?? 'null', label: '' };
            }
            const style = getComputedStyle(el);
            // "Focus is visible" is the requirement (WCAG 2.4.7), not "focus is
            // an outline". panel.css marks text entry with the accent caret and
            // gives every other control the 2px ring (see the focus note there),
            // so all three markers count here: outline, an accent border/halo,
            // or an accent caret on a focused field.
            const outlined = (parseFloat(style.outlineWidth) || 0) >= 2 && style.outlineStyle !== 'none';
            const ringed = (style.boxShadow && style.boxShadow !== 'none')
              || style.borderColor === getComputedStyle(document.documentElement).getPropertyValue('--c-accent').trim()
              || /rgb\(0, 255, 135\)|rgb\(8, 122, 76\)/.test(style.borderColor);
            const textField = el.matches('input:not([type=checkbox]):not([type=radio]):not([type=file]), textarea');
            const caret = textField && /rgb\(0, 255, 135\)|rgb\(8, 122, 76\)/.test(style.caretColor || '');
            return { end: false, ok: outlined || Boolean(ringed) || caret, tag: el.tagName, label: (el.textContent || el.getAttribute('aria-label') || '').trim().slice(0, 20) };
          });
          if (result.end) break;
          focus.push(result);
        }
        const noRing = focus.filter((item) => !item.ok);
        record(noRing.length === 0, `${label} focus ring on first 8 tab stops`,
          noRing.length
            ? noRing.map((item) => `${item.tag} "${item.label}"`).join(', ')
            : `${focus.length}/${focus.length} focused elements ring`);
      } catch (error) {
        record(false, `${label} page loaded`, error instanceof Error ? error.message : String(error));
      }
      await context.close();
    }
  }
}

await browser.close();
console.log(`\n${failures === 0 ? 'AUDIT GREEN' : `AUDIT FAILED — ${failures} failing check(s)`} (${results.length} checks)`);
process.exit(failures === 0 ? 0 : 1);
