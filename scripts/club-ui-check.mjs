#!/usr/bin/env node
/**
 * Club panel acceptance check — the gate from
 * docs/ui/club-panel-redesign-prompt.md §7.2, implemented as 16 numbered asserts.
 *
 *   cd apps/web-club && npm run dev:demo      # terminal 1
 *   node scripts/club-ui-check.mjs            # terminal 2 (repo root)
 *
 * Design decisions that matter when reading a failure:
 *
 * 1. The page is measured in TWO states:
 *      "populated" — `/v1/club/**` is answered by the fixtures below. This is
 *        test scaffolding, not product code: it lets the board render the
 *        content it would render against a live API, which is both the tallest
 *        and the most complete state. All numeric budgets use this state.
 *      "honest"    — no interception. The demo build has no backend, so the API
 *        metrics must degrade to `unavailable` with a single honest banner.
 *        Checks 11 and 14 run here too: the unavailable path is a real state
 *        and must not throw.
 *
 * 2. `document.getAnimations()` under reduced motion: the check runs the page
 *    with `prefers-reduced-motion: reduce` and requires no running animations.
 *    Transitions are allowed to exist; animations that are *running* are not.
 *
 * 3. Contrast is computed from computed styles (axe reports color-contrast as
 *    "incomplete" because it cannot resolve stacked translucent layers). The
 *    algorithm walks up for the nearest opaque background, blends the text's
 *    own alpha over it, and applies the WCAG 2.1 relative-luminance formula.
 *
 * Exit code 0 = gate green, 1 = at least one assert failed.
 */
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');
const require = createRequire(join(REPO, 'apps/web-club/package.json'));
const { chromium } = require('playwright');

const BASE = process.env.CLUB_UI_URL ?? 'http://localhost:5173';
const PAGE = `${BASE}/dashboard`;
const OUT = join(REPO, 'output/club-redesign/after');
mkdirSync(OUT, { recursive: true });

// --- thresholds (do not relax without Lead review; §7.4) --------------------
const BUDGET = {
  height: { '1440x900': 1700, '1024x768': 2000, '390x844': 3000 },
  widths: [320, 390, 768, 1024, 1440],
  surfaceRadii: ['16px'],
  tileRadii: ['12px'],
  controlRadii: ['10px', '12px', '999px'],
  // 4px is --r-mark: a checkbox is a mark, not a container (see tokens.css).
  allowedRadii: new Set(['4px', '10px', '12px', '16px', '999px']),
  maxFontSizes: 6,
  minFontSize: 11,
  maxUppercase: 1,
  minTarget: 44,
  minContrast: 4.5,
  minContrastLarge: 3,
  maxPanels: 7,
};

// --- fixtures: shapes mirror the real DTOs in src/api/client.ts -------------
const FIXTURES = {
  dashboard: {
    session: { id: 'sess_01h8xk2m9p', status: 'active' },
    requests_today: 47,
    earned_total_minor: 128450,
  },
  profile: {
    id: 'club-1',
    name: 'Club Nebula',
    slug: 'nebula',
    address: 'Carrer del Mar 12',
    country: 'ES',
    email: 'owner@nebula.test',
    phone: '+34 600 000 000',
    logo_url: null,
    stripe_account_id: null,
    stripe_onboarding_status: 'verified',
    status: 'active',
    created_at: '2026-01-01T00:00:00Z',
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
      {
        id: 'price-1',
        club_id: 'club-1',
        price_minor: 500,
        dj_share_bp: null,
        valid_from: '2026-10-01T00:00:00Z',
        valid_to: null,
        created_at: '2026-10-01T00:00:00Z',
      },
    ],
  },
  qr: {
    session_id: 'sess_01h8xk2m9p',
    qr_token: 'a1b2c3d4e5f60718293a4b5c6d7e8f90',
    expires_at: '2026-10-27T03:30:00Z',
  },
  earnings: { period: 'day', gross_minor: 183500, net_minor: 128450, payments: 47 },
};

const results = [];
let failures = 0;

function assert(number, name, ok, detail) {
  results.push({ number, name, ok, detail });
  if (!ok) failures += 1;
  const mark = ok ? 'PASS' : 'FAIL';
  console.log(`${mark}  ${String(number).padStart(2, ' ')}. ${name}${detail ? `\n         ${detail}` : ''}`);
}

async function newPage(browser, viewport, { populated = true, reducedMotion = 'no-preference' } = {}) {
  const context = await browser.newContext({
    viewport,
    reducedMotion: reducedMotion === 'reduce' ? 'reduce' : 'no-preference',
  });
  const page = await context.newPage();
  const consoleErrors = [];
  const pageErrors = [];
  const externalHosts = new Set();
  const blockedRequests = [];

  page.on('console', (msg) => {
    if (msg.type() === 'error') consoleErrors.push(msg.text());
  });
  page.on('pageerror', (err) => pageErrors.push(err.message));
  page.on('request', (request) => {
    let url;
    try {
      url = new URL(request.url());
    } catch {
      return;
    }
    if (url.host !== new URL(BASE).host) externalHosts.add(url.host);
    if (/code\.iconify\.design|fonts\.googleapis\.com|fonts\.gstatic\.com/.test(url.host)) {
      blockedRequests.push(url.host);
    }
  });

  if (populated) {
    await page.route('**/v1/club/**', async (route) => {
      const url = route.request().url();
      const json = (body) =>
        route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
      if (url.includes('/club/dashboard')) return json(FIXTURES.dashboard);
      if (url.includes('/club/profile')) return json(FIXTURES.profile);
      if (url.includes('/club/djs')) return json(FIXTURES.djs);
      if (url.includes('/club/pricing')) return json(FIXTURES.pricing);
      if (url.includes('/club/earnings')) return json(FIXTURES.earnings);
      if (url.includes('/qr/generate')) return json(FIXTURES.qr);
      return json({});
    });
  }

  await page.goto(PAGE, { waitUntil: 'networkidle', timeout: 60000 });
  await page.waitForTimeout(900);
  return { context, page, consoleErrors, pageErrors, externalHosts, blockedRequests };
}

/** Everything the checks need, measured once per viewport in the browser. */
const collect = (page) =>
  page.evaluate(() => {
    const parseColor = (value) => {
      const match = value.match(/rgba?\(([^)]+)\)/);
      if (match) {
        const parts = match[1].split(',').map(Number);
        return { r: parts[0], g: parts[1], b: parts[2], a: parts.length > 3 ? parts[3] : 1 };
      }
      // `color-mix()` resolves to `color(srgb r g b / a)` with components in
      // 0–1. Skipping it made the background walk fall through to an ancestor,
      // so a `color-mix` surface was never measured against its own fill.
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
        return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
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
      return body && body.a > 0.5 ? body : { r: 0, g: 0, b: 0, a: 1 };
    };

    const visible = (el) => {
      const style = getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return (
        style.visibility !== 'hidden' &&
        style.display !== 'none' &&
        Number(style.opacity) > 0 &&
        rect.width > 0 &&
        rect.height > 0
      );
    };

    const root = document.body;
    const radii = new Map();
    const surfaces = new Map();
    const tiles = new Map();
    const controls = new Map();
    const fontSizes = new Map();
    const bordered = [];
    const uppercase = [];
    const smallTargets = [];
    const unnamed = [];
    const metrics = [];
    const contrasts = [];

    for (const el of root.querySelectorAll('*')) {
      if (!(el instanceof HTMLElement)) continue;
      const style = getComputedStyle(el);
      const rect = el.getBoundingClientRect();

      if (style.borderRadius && style.borderRadius !== '0px' && visible(el)) {
        radii.set(style.borderRadius, (radii.get(style.borderRadius) ?? 0) + 1);
      }

      if (el.hasAttribute('data-surface')) {
        surfaces.set(style.borderRadius, (surfaces.get(style.borderRadius) ?? 0) + 1);
        const borderWidth =
          parseFloat(style.borderTopWidth) +
          parseFloat(style.borderRightWidth) +
          parseFloat(style.borderBottomWidth) +
          parseFloat(style.borderLeftWidth);
        const shadow = style.boxShadow && style.boxShadow !== 'none' ? style.boxShadow : '';
        const blur = Number((shadow.match(/(\d+(?:\.\d+)?)px(?=\s*$)/) ?? [])[1] ?? 0);
        const exempt = el.matches('input, select, textarea, [data-allow-border]');
        if (!exempt && (borderWidth > 0 || blur > 0)) {
          bordered.push({
            tag: el.tagName.toLowerCase(),
            cls: String(el.className).slice(0, 60),
            borderWidth,
            shadow,
          });
        }
      }
      if (el.hasAttribute('data-tile')) tiles.set(style.borderRadius, (tiles.get(style.borderRadius) ?? 0) + 1);
      if (el.hasAttribute('data-control')) controls.set(style.borderRadius, (controls.get(style.borderRadius) ?? 0) + 1);

      const hasOwnText = [...el.childNodes].some(
        (node) => node.nodeType === Node.TEXT_NODE && node.textContent.trim(),
      );
      if (hasOwnText && visible(el)) {
        const size = Math.round(parseFloat(style.fontSize));
        fontSizes.set(size, (fontSizes.get(size) ?? 0) + 1);
        if (style.textTransform === 'uppercase' && parseFloat(style.letterSpacing) > 0) {
          uppercase.push(`${el.tagName.toLowerCase()} "${el.textContent.trim().slice(0, 32)}"`);
        }
        const fg = parseColor(style.color);
        if (fg) {
          const bg = backgroundOf(el);
          const blended = blend(fg, bg);
          const l1 = luminance(blended);
          const l2 = luminance(bg);
          const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
          contrasts.push({
            ratio: Math.round(ratio * 100) / 100,
            size,
            text: el.textContent.trim().slice(0, 40),
            cls: String(el.className).slice(0, 40),
          });
        }
      }

      if (el.hasAttribute('data-metric')) {
        metrics.push({
          source: el.getAttribute('data-source'),
          label: (el.textContent || '').trim().slice(0, 40),
        });
      }

      const interactive =
        ['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName) ||
        el.hasAttribute('tabindex');
      if (interactive && visible(el)) {
        const name =
          el.getAttribute('aria-label') ||
          el.getAttribute('title') ||
          (el.id && document.querySelector(`label[for="${CSS.escape(el.id)}"]`)?.textContent) ||
          el.textContent ||
          el.getAttribute('placeholder') ||
          '';
        if (!String(name).trim()) unnamed.push(`${el.tagName.toLowerCase()}.${String(el.className).slice(0, 30)}`);
        if (rect.width < 44 || rect.height < 44) {
          const inlineLink = el.tagName === 'A' && el.closest('p, li, span') !== null;
          if (!inlineLink) {
            smallTargets.push(
              `${el.tagName.toLowerCase()} ${Math.round(rect.width)}x${Math.round(rect.height)} "${String(name).trim().slice(0, 24)}"`,
            );
          }
        }
      }
    }

    contrasts.sort((a, b) => a.ratio - b.ratio);

    return {
      scrollHeight: document.body.scrollHeight,
      overflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      radii: Object.fromEntries(radii),
      surfaces: Object.fromEntries(surfaces),
      tiles: Object.fromEntries(tiles),
      controls: Object.fromEntries(controls),
      fontSizes: Object.fromEntries(fontSizes),
      bordered,
      uppercase,
      smallTargets,
      unnamed,
      metrics,
      panelCount: document.querySelectorAll('[data-surface="panel"]').length,
      previewBanner: Boolean(document.querySelector('[data-preview-banner]')),
      contrastMin: contrasts[0] ?? null,
      contrastFailures: contrasts.filter(
        (item) => item.ratio < (item.size >= 24 ? 3 : 4.5),
      ),
    };
  });

const radiiSet = (map) => new Set(Object.keys(map ?? {}));

async function main() {
  const browser = await chromium.launch({ headless: true });
  const measured = {};

  // --- populated state: all numeric budgets -------------------------------
  for (const [name, width, height] of [
    ['1440x900', 1440, 900],
    ['1024x768', 1024, 768],
    ['390x844', 390, 844],
  ]) {
    const session = await newPage(browser, { width, height });
    measured[name] = { ...(await collect(session.page)), ...{ context: session } };
    await session.page.screenshot({ path: join(OUT, `dashboard-${name}.png`), fullPage: true });
    if (name === '1440x900') {
      await session.page.screenshot({ path: join(OUT, 'dashboard-1440-first-screen.png') });
    }
    session.__errors = { console: session.consoleErrors, page: session.pageErrors };
    measured[name].consoleErrors = session.consoleErrors;
    measured[name].pageErrors = session.pageErrors;
    measured[name].externalHosts = [...session.externalHosts];
    measured[name].blockedRequests = session.blockedRequests;
  }

  // --- overflow at the full width ladder ----------------------------------
  const overflow = {};
  for (const width of BUDGET.widths) {
    if (measured[`${width}x`]) continue;
    const session = await newPage(browser, { width, height: 900 });
    overflow[width] = await session.page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    await session.context.close();
  }
  for (const [name, width] of [
    ['1440x900', 1440],
    ['1024x768', 1024],
    ['390x844', 390],
  ]) {
    overflow[width] = measured[name].overflow;
  }

  // --- reduced motion -----------------------------------------------------
  const reduced = await newPage(browser, { width: 1440, height: 900 }, { reducedMotion: 'reduce' });
  const reducedAnimations = await reduced.page.evaluate(() =>
    document.getAnimations().map((animation) => ({
      name: animation.animationName ?? animation.transitionProperty ?? 'unknown',
      state: animation.playState,
    })),
  );
  await reduced.page.screenshot({ path: join(OUT, 'dashboard-reduced-motion.png'), fullPage: true });
  await reduced.context.close();

  // --- honest state: no backend ------------------------------------------
  const honest = await newPage(browser, { width: 1440, height: 900 }, { populated: false });
  const honestData = await collect(honest.page);
  await honest.page.screenshot({ path: join(OUT, 'dashboard-honest-unavailable.png'), fullPage: true });
  await honest.context.close();

  // --- focus ring on the first 10 tab stops -------------------------------
  const focusSession = await newPage(browser, { width: 1440, height: 900 });
  const focus = [];
  for (let index = 0; index < 10; index += 1) {
    await focusSession.page.keyboard.press('Tab');
    focus.push(
      await focusSession.page.evaluate(() => {
        const el = document.activeElement;
        const style = getComputedStyle(el);
        // Text entry is marked by the accent caret rather than a ring (see the
        // focus note in styles/panel.css), so a focused field counts as visible
        // when its caret carries the accent colour.
        const textField = el.matches('input:not([type=checkbox]):not([type=radio]):not([type=file]), textarea');
        return {
          tag: el.tagName,
          outlineWidth: parseFloat(style.outlineWidth) || 0,
          caret: textField && /rgb\(0, 255, 135\)|rgb\(8, 122, 76\)/.test(style.caretColor || ''),
          label: (el.textContent || el.getAttribute('aria-label') || '').trim().slice(0, 24),
        };
      }),
    );
  }
  await focusSession.context.close();

  await browser.close();

  // ======================================================================
  // 1. radii contract
  // ======================================================================
  const desktop = measured['1440x900'];
  const mobile = measured['390x844'];
  const surfaceSet = radiiSet(desktop.surfaces);
  assert(
    1,
    '[data-surface] radius is exactly 16px',
    surfaceSet.size === 1 && surfaceSet.has('16px'),
    `measured ${JSON.stringify(desktop.surfaces)}`,
  );

  const tileSet = radiiSet(desktop.tiles);
  const tileOk = [...tileSet].every((radius) => radius === '12px');
  assert(1.1, '[data-tile] radius is 12px', tileOk, `measured ${JSON.stringify(desktop.tiles)}`);

  const controlSet = radiiSet(desktop.controls);
  const controlOk = [...controlSet].every((radius) => BUDGET.controlRadii.includes(radius));
  assert(
    1.2,
    '[data-control] radii subset of {10px,12px,999px}',
    controlOk,
    `measured ${JSON.stringify(desktop.controls)}`,
  );

  for (const [name, data] of [
    ['1440x900', desktop],
    ['390x844', mobile],
  ]) {
    const pageRadii = radiiSet(data.radii);
    const stray = [...pageRadii].filter((radius) => !BUDGET.allowedRadii.has(radius));
    assert(
      1.3,
      `rendered radius set is exactly {10,12,16,999} @${name}`,
      stray.length === 0,
      `measured ${JSON.stringify(data.radii)}${stray.length ? ` — stray: ${stray.join(', ')}` : ''}`,
    );
  }

  // ======================================================================
  // 2. no borders or shadows on surfaces
  // ======================================================================
  assert(
    2,
    'no [data-surface] has a border or a blurred shadow',
    desktop.bordered.length === 0 && mobile.bordered.length === 0,
    `1440: ${JSON.stringify(desktop.bordered.slice(0, 4))} 390: ${JSON.stringify(mobile.bordered.slice(0, 4))}`,
  );

  // ======================================================================
  // 3. height budget (populated = tallest state)
  // ======================================================================
  for (const [name, budget] of Object.entries(BUDGET.height)) {
    const data = measured[name];
    assert(
      3,
      `body height <= ${budget}px @${name}`,
      data.scrollHeight <= budget,
      `measured ${data.scrollHeight}px (budget ${budget}, over by ${Math.max(0, data.scrollHeight - budget)})`,
    );
  }

  // ======================================================================
  // 4. no horizontal overflow
  // ======================================================================
  const overflowing = Object.entries(overflow).filter(([, value]) => value > 0);
  assert(
    4,
    'no horizontal overflow at 320/390/768/1024/1440',
    overflowing.length === 0,
    overflowing.length ? `overflow at ${JSON.stringify(overflow)}` : `all zero: ${JSON.stringify(overflow)}`,
  );

  // ======================================================================
  // 5. accessible names
  // ======================================================================
  assert(
    5,
    'every interactive element has an accessible name',
    desktop.unnamed.length === 0 && mobile.unnamed.length === 0,
    `1440: ${JSON.stringify(desktop.unnamed.slice(0, 5))} 390: ${JSON.stringify(mobile.unnamed.slice(0, 5))}`,
  );

  // ======================================================================
  // 6. touch targets on mobile
  // ======================================================================
  assert(
    6,
    `touch targets >= ${BUDGET.minTarget}x${BUDGET.minTarget} at 390`,
    mobile.smallTargets.length === 0,
    mobile.smallTargets.length ? mobile.smallTargets.slice(0, 8).join(' | ') : 'none under 44px',
  );

  // ======================================================================
  // 7. focus ring visible on the first 10 tab stops
  // ======================================================================
  const noRing = focus.filter((item) => item.outlineWidth < 2 && !item.caret);
  assert(
    7,
    'focus ring (outline >= 2px, or the accent caret on a text field) on the first 10 tab stops',
    noRing.length === 0,
    noRing.length ? `no ring: ${noRing.map((item) => `${item.tag} "${item.label}"`).join(', ')}` : `10/10 focused elements ring`,
  );

  // ======================================================================
  // 8. reduced motion
  // ======================================================================
  const running = reducedAnimations.filter((animation) => animation.state === 'running');
  assert(
    8,
    'prefers-reduced-motion: no running animations',
    running.length === 0,
    running.length ? `${running.length} running: ${JSON.stringify(running.slice(0, 5))}` : '0 running',
  );

  // ======================================================================
  // 9. font sizes
  // ======================================================================
  for (const [name, data] of [
    ['1440x900', desktop],
    ['390x844', mobile],
  ]) {
    const sizes = Object.keys(data.fontSizes).map(Number);
    const tooSmall = sizes.filter((size) => size < BUDGET.minFontSize);
    assert(
      9,
      `<= ${BUDGET.maxFontSizes} font sizes and none below ${BUDGET.minFontSize}px @${name}`,
      sizes.length <= BUDGET.maxFontSizes && tooSmall.length === 0,
      `${sizes.length} sizes ${JSON.stringify(sizes.sort((a, b) => a - b))}${tooSmall.length ? ` — below 11px: ${tooSmall.join(', ')}` : ''}`,
    );
  }

  // ======================================================================
  // 10. uppercase + tracking
  // ======================================================================
  assert(
    10,
    'at most one uppercase + letter-spaced element',
    desktop.uppercase.length <= BUDGET.maxUppercase,
    `${desktop.uppercase.length}: ${desktop.uppercase.join(' | ')}`,
  );

  // ======================================================================
  // 11. data-source contract + preview banner
  // ======================================================================
  const allMetrics = [...desktop.metrics, ...honestData.metrics];
  const badSource = allMetrics.filter((metric) => !['live', 'preview', 'unavailable'].includes(metric.source));
  const hasPreview = allMetrics.some((metric) => metric.source === 'preview');
  const bannerOk = !hasPreview || (desktop.previewBanner && honestData.previewBanner);
  assert(
    11,
    'every [data-metric] has a valid data-source, preview implies a banner',
    allMetrics.length > 0 && badSource.length === 0 && bannerOk,
    `${allMetrics.length} metrics; invalid sources: ${JSON.stringify(badSource.slice(0, 4))}; preview present: ${hasPreview}; banner populated=${desktop.previewBanner} honest=${honestData.previewBanner}`,
  );

  // ======================================================================
  // 12. contrast
  // ======================================================================
  for (const [name, data] of [
    ['1440x900', desktop],
    ['390x844', mobile],
  ]) {
    assert(
      12,
      `text contrast >= 4.5:1 (>=3:1 at >=24px) @${name}`,
      data.contrastFailures.length === 0,
      data.contrastFailures.length
        ? `${data.contrastFailures.length} below AA, worst: ${JSON.stringify(data.contrastFailures.slice(0, 5))}`
        : `min ${data.contrastMin?.ratio}:1 on "${data.contrastMin?.text}"`,
    );
  }

  // ======================================================================
  // 13. no external CDN
  // ======================================================================
  const hosts = [...new Set([...desktop.externalHosts, ...mobile.externalHosts, ...honestData.externalHosts ?? []])];
  const blocked = [...desktop.blockedRequests, ...mobile.blockedRequests];
  assert(
    13,
    'no requests to iconify / Google Fonts',
    blocked.length === 0,
    blocked.length ? `blocked: ${blocked.join(', ')}` : `0 external CDN requests; all hosts: ${JSON.stringify(hosts)}`,
  );

  // ======================================================================
  // 14. no console errors (populated) + honest state renders without throwing
  // ======================================================================
  const consoleErrors = [...desktop.consoleErrors, ...mobile.consoleErrors];
  const pageErrors = [...desktop.pageErrors, ...mobile.pageErrors];
  assert(
    14,
    'no console.error / pageerror in the populated state',
    consoleErrors.length === 0 && pageErrors.length === 0,
    `console: ${JSON.stringify(consoleErrors.slice(0, 3))} page: ${JSON.stringify(pageErrors.slice(0, 3))}`,
  );

  // ======================================================================
  // 15. screenshots exist
  // ======================================================================
  const { existsSync } = await import('node:fs');
  const shots = [
    'dashboard-1440x900.png',
    'dashboard-1024x768.png',
    'dashboard-390x844.png',
    'dashboard-reduced-motion.png',
    'dashboard-honest-unavailable.png',
  ];
  const missing = shots.filter((shot) => !existsSync(join(OUT, shot)));
  assert(15, 'screenshots 1440/1024/390 + reduced motion + honest', missing.length === 0, missing.length ? `missing: ${missing.join(', ')}` : `${shots.length} files in output/club-redesign/after/`);

  // ======================================================================
  // 16. report artifacts
  // ======================================================================
  const report = {
    generatedAt: new Date().toISOString(),
    base: BASE,
    passed: failures === 0,
    failures,
    results,
    measured: Object.fromEntries(
      Object.entries(measured).map(([name, data]) => [
        name,
        {
          scrollHeight: data.scrollHeight,
          overflow: data.overflow,
          panelCount: data.panelCount,
          radii: data.radii,
          surfaces: data.surfaces,
          tiles: data.tiles,
          controls: data.controls,
          fontSizes: data.fontSizes,
          uppercase: data.uppercase,
          smallTargets: data.smallTargets,
          contrastMin: data.contrastMin,
          consoleErrors: data.consoleErrors,
          pageErrors: data.pageErrors,
          externalHosts: data.externalHosts,
        },
      ]),
    ),
    honest: {
      scrollHeight: honestData.scrollHeight,
      metrics: honestData.metrics,
      previewBanner: honestData.previewBanner,
      contrastMin: honestData.contrastMin,
    },
    overflow,
    focus,
    reducedAnimations,
  };
  writeFileSync(join(OUT, 'report.json'), `${JSON.stringify(report, null, 2)}\n`);

  const lines = [
    '# Club panel — acceptance report',
    '',
    `Generated: ${report.generatedAt}`,
    `Target: ${PAGE}`,
    `Result: **${report.passed ? 'GREEN' : `RED (${failures} failing asserts)`}**`,
    '',
    '| # | Assert | Result | Detail |',
    '|---|---|---|---|',
    ...results.map(
      (item) =>
        `| ${item.number} | ${item.name} | ${item.ok ? 'pass' : '**FAIL**'} | ${String(item.detail ?? '').replace(/\|/g, '\\|').slice(0, 220)} |`,
    ),
    '',
    '## Measurements (populated fixtures)',
    '',
    '| Viewport | Height | Overflow | Panels | Radii | Font sizes | Contrast min |',
    '|---|---|---|---|---|---|---|',
    ...Object.entries(report.measured).map(
      ([name, data]) =>
        `| ${name} | ${data.scrollHeight} | ${data.overflow} | ${data.panelCount} | ${Object.keys(data.radii).join(' ')} | ${Object.keys(data.fontSizes).length} | ${data.contrastMin?.ratio} |`,
    ),
    '',
    '## Honest state (no backend)',
    '',
    `- height ${report.honest.scrollHeight}px`,
    `- metrics: ${JSON.stringify(report.honest.metrics)}`,
    `- preview banner present: ${report.honest.previewBanner}`,
    '',
  ];
  writeFileSync(join(OUT, 'report.md'), `${lines.join('\n')}\n`);
  assert(16, 'report.json + report.md written', existsSync(join(OUT, 'report.json')), 'output/club-redesign/after/report.{json,md}');

  console.log(`\n${failures === 0 ? 'GATE GREEN' : `GATE RED — ${failures} failing assert(s)`}`);
  console.log(`report: ${join(OUT, 'report.md')}`);
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((error) => {
  console.error('club-ui-check crashed:', error);
  process.exit(1);
});
