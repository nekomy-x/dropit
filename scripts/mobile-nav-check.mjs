#!/usr/bin/env node
/**
 * Mobile navigation gate — the acceptance check for the burger + drawer shell.
 *
 *   node scripts/mobile-nav-check.mjs                    # club + admin
 *   node scripts/mobile-nav-check.mjs --app admin        # one app
 *   node scripts/mobile-nav-check.mjs --club-base http://localhost:5173
 *
 * The bottom bar (`panel-mobile-nav`, five abbreviated destinations) is gone
 * from both web panels. Below 1024px the rail is replaced by a burger button in
 * the top bar that opens `packages/ui/nav-drawer.tsx` — a native `<dialog>`
 * edge sheet that carries the rail's own groups, labels, order and badges.
 * This gate measures the rendered result, per app, in BOTH themes, at 1440x900
 * (rail) and 390x844 (burger + drawer):
 *
 *   1. desktop: `.panel-rail` visible, `.panel-nav-trigger` not visible
 *   2. no `.panel-mobile-nav` / `.panel-mobile-link` left in the DOM
 *   3. no horizontal overflow at 320 / 390 / 768 / 1024 / 1440
 *   4. mobile: rail hidden, exactly one visible burger
 *   5. the burger has an accessible name and `aria-haspopup="dialog"`
 *   6. the burger opens `dialog.panel-nav-drawer[open]`, `aria-expanded` flips
 *   7. the drawer lists every destination the desktop rail lists — same hrefs,
 *      same labels, same count per group label; anything missing is named
 *   8. the drawer is pinned inside the viewport (left >= 0, right <= width,
 *      height == viewport height) and its scrim both resolves and closes it
 *   9. every interactive element inside the drawer has a >= 44x44 hit area
 *  10. Escape closes the drawer and returns focus to the burger
 *  11. a destination click closes the drawer and navigates
 *  12. body scroll is locked while open and restored after close
 *  13. no console.error / pageerror during the open/close cycles
 *  14. drawer label contrast >= 4.5:1 against its own computed background
 *  15. the radius set inside the drawer stays in the token contract
 *  16. under `prefers-reduced-motion: reduce` nothing is left running
 *  17. the four screenshots exist at 390x844 (`closed|open` × `light|dark`)
 *
 * Design decisions worth knowing when reading a failure:
 *
 * 1. Mobile contexts run with `reducedMotion: 'reduce'`, like `panel-audit.mjs`.
 *    The sheet's entrance animation lives inside
 *    `@media (prefers-reduced-motion: no-preference)`, so geometry is measured
 *    in its settled state and assert 16 is the real reduced-motion question,
 *    not a race against a 180ms animation.
 * 2. Geometry is read from the rendered DOM (`getBoundingClientRect()`), not
 *    from source, so a CSS regression fails the gate.
 * 3. Hit areas allow a 0.5px epsilon: `min-height: 44px` is exact in a
 *    device-pixel-ratio-1 context, but subpixel layout can report 43.99.
 * 4. Contrast is computed the same way as `panel-audit.mjs`: walk up for the
 *    nearest opaque background, blend the text's own alpha over it, apply the
 *    WCAG 2.1 relative-luminance formula. The helper is copied, not imported,
 *    so each gate stays runnable on its own.
 * 5. Both dev servers must be running (`npm run dev:demo` in each app). An
 *    unreachable port prints the URL and exits 1 before a browser launches —
 *    a gate that hangs on a missing server is worse than one that is red.
 *
 * Exit code 0 = every assert passed for both apps in both themes, 1 = at least
 * one failed. Failing screenshots and a report land in `output/mobile-nav/`.
 */
import { createRequire } from 'node:module';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
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

const APPS = {
  club: {
    base: flag('club-base', process.env.CLUB_BASE ?? 'http://localhost:5173'),
    // The club staff shell. `/dj` is a second shell with its own rail and is
    // out of scope for this gate.
    route: '/dashboard',
    start: 'cd apps/web-club && npm run dev:demo',
  },
  admin: {
    // `apps/web-admin` is served under the `/admin/` base.
    base: flag('admin-base', process.env.ADMIN_BASE ?? 'http://localhost:5174'),
    route: '/admin/',
    start: 'cd apps/web-admin && npm run dev:demo',
  },
};

const THEMES = ['dark', 'light'];
const DESKTOP = { width: 1440, height: 900 };
const MOBILE = { width: 390, height: 844 };
// The overflow ladder: the widths the contract names. Below the 1024px rail
// breakpoint the phone height is the honest one to measure at.
const WIDTHS = [320, 390, 768, 1024, 1440];
// 4px is --r-mark (a checkbox is a mark, not a container), 999px is --r-chip.
const ALLOWED_RADII = new Set(['0px', '4px', '8px', '10px', '12px', '16px', '999px']);
// --hit-mobile: a thumb target, measured in-page by assert 9.
const MIN_TARGET = 44;
const EPSILON = 0.5;

// Test scaffolding, not product code: the shapes mirror the real DTOs, so the
// shell and its screens render the populated state the gate measures.
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
    ],
  },
  pricing: { items: [{ id: 'price-1', club_id: 'club-1', price_minor: 500, dj_share_bp: null, valid_from: '2026-10-01T00:00:00Z', valid_to: null, created_at: '2026-10-01T00:00:00Z' }] },
  earnings: { period: 'day', gross_minor: 183500, net_minor: 128450, payments: 47 },
};

// The 17 asserts, named once so a blocked assert still prints a readable line.
const ASSERT_NAMES = {
  1: 'desktop: rail visible, burger hidden',
  2: 'no bottom navigation bar left in the DOM',
  3: 'no horizontal overflow at 320/390/768/1024/1440',
  4: 'mobile: rail hidden, exactly one visible burger',
  5: 'burger has an accessible name and aria-haspopup="dialog"',
  6: 'burger opens dialog.panel-nav-drawer and aria-expanded flips to true',
  7: 'drawer lists every rail destination, same count per group',
  8: 'drawer is inside the viewport and the scrim covers the page',
  9: 'every interactive element in the drawer is >= 44x44',
  10: 'Escape closes the drawer and returns focus to the burger',
  11: 'clicking a destination closes the drawer and navigates',
  12: 'body scroll locked while open, restored after close',
  13: 'no console.error / pageerror during open/close cycles',
  14: 'drawer label contrast >= 4.5:1',
  15: 'radius set inside the drawer is within the token contract',
  16: 'reduced motion leaves no animation running',
  17: 'screenshots closed/open × light/dark at 390x844',
};

const results = [];
let failures = 0;
const record = (number, name, ok, detail, tag) => {
  results.push({ number, name, tag, ok, detail });
  if (!ok) failures += 1;
  const head = number === null ? `--. ${name}` : `${String(number).padStart(2, ' ')}. ${ASSERT_NAMES[number]}`;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${head} — ${tag}${detail ? `\n         ${detail}` : ''}`);
};
const assert = (number, ok, detail, tag) => record(number, ASSERT_NAMES[number], ok, detail, tag);
/** Infrastructure failure: the assert could not even be attempted. */
const blocked = (reason, tag) => record(null, `page loaded (${reason})`, false, reason, tag);

/* ------------------------------------------------------------------------- */
/* Preflight: fail fast and readably when a dev server is not listening.     */
/* ------------------------------------------------------------------------- */
const targets = Object.entries(APPS).filter(([name]) => !onlyApp || onlyApp === name);
if (targets.length === 0) {
  console.error(`unknown app "${onlyApp}" — expected one of: ${Object.keys(APPS).join(', ')}`);
  process.exit(1);
}

const unreachable = [];
for (const [name, app] of targets) {
  const url = `${app.base}${app.route}`;
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(4000), redirect: 'follow' });
    if (!response.ok) unreachable.push(`${url} → HTTP ${response.status}`);
  } catch (error) {
    const reason = error instanceof Error ? (error.cause?.code ?? error.name) : String(error);
    unreachable.push(`${url} → ${reason}`);
  }
}
if (unreachable.length > 0) {
  console.error('mobile-nav-check: dev server not reachable\n');
  for (const line of unreachable) console.error(`  ${line}`);
  console.error('\nstart the servers first:');
  for (const app of targets) console.error(`  ${app[1].start}   # ${app[1].base}`);
  console.error('\nthe gate needs both panels running; it does not start them.');
  process.exit(1);
}

/* ------------------------------------------------------------------------- */
/* In-page measurement helpers.                                              */
/* ------------------------------------------------------------------------- */

/** Everything assert 1/2/4/5/12 need from the shell itself. */
const readShell = (page) => page.evaluate(() => {
  const visible = (el) => {
    if (!el) return false;
    const style = getComputedStyle(el);
    const rect = el.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && Number(style.opacity) > 0 && rect.width > 0 && rect.height > 0;
  };
  const rail = document.querySelector('.panel-rail');
  const triggers = [...document.querySelectorAll('.panel-nav-trigger')];
  const burger = triggers.find(visible) ?? null;
  const dialog = document.querySelector('dialog.panel-nav-drawer');
  return {
    path: location.pathname,
    rail: { present: Boolean(rail), visible: visible(rail) },
    triggers: { total: triggers.length, visible: triggers.filter(visible).length },
    burger: burger ? {
      label: (burger.getAttribute('aria-label') ?? '').trim(),
      title: (burger.getAttribute('title') ?? '').trim(),
      haspopup: burger.getAttribute('aria-haspopup'),
      expanded: burger.getAttribute('aria-expanded'),
      tag: burger.tagName.toLowerCase(),
    } : null,
    mobileNav: document.querySelectorAll('.panel-mobile-nav').length,
    mobileLinks: document.querySelectorAll('.panel-mobile-link').length,
    bodyOverflow: getComputedStyle(document.body).overflow,
    bodyInlineOverflow: document.body.style.overflow,
    drawerOpen: Boolean(dialog && dialog.open),
  };
});

/** The desktop rail's destination list, grouped exactly as the DOM groups it. */
const readRail = (page) => page.evaluate(() => {
  const visible = (el) => {
    const style = getComputedStyle(el);
    const rect = el.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && Number(style.opacity) > 0 && rect.width > 0 && rect.height > 0;
  };
  const item = (link) => ({
    href: link.getAttribute('href'),
    // `data-nav-dest` is the contract's stable identity for a destination; the
    // rails use routing links, so it is optional and the href is the fallback.
    dest: link.getAttribute('data-nav-dest'),
    label: (link.textContent ?? '').trim().replace(/\s+/g, ' '),
    tag: link.tagName.toLowerCase(),
  });
  return [...document.querySelectorAll('.panel-rail .panel-nav-group')]
    .map((group) => ({
      label: (group.querySelector('.panel-nav-label')?.textContent ?? '').trim(),
      items: [...group.querySelectorAll('.panel-nav-link')].filter(visible).map(item),
    }));
});

/**
 * Everything the open drawer must answer: geometry, destinations, contrast,
 * radii, hit areas, scrim and running animations.
 */
const measureDrawer = (page, minTarget) => page.evaluate((min) => {
  const parseColor = (value) => {
    const match = value.match(/rgba?\(([^)]+)\)/);
    if (match) {
      const parts = match[1].split(',').map(Number);
      return { r: parts[0], g: parts[1], b: parts[2], a: parts.length > 3 ? parts[3] : 1 };
    }
    // `color-mix()` resolves to the modern `color(srgb r g b / a)` form, with
    // components in 0–1 — the same branch panel-audit.mjs needs, copied so a
    // `color-mix` surface is measured rather than silently skipped.
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
  const visible = (el) => {
    const style = getComputedStyle(el);
    const rect = el.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && Number(style.opacity) > 0 && rect.width > 0 && rect.height > 0;
  };

  const dialog = document.querySelector('dialog.panel-nav-drawer');
  if (!dialog) return { present: false };

  // Contrast walks up for the nearest opaque background, as in panel-audit.mjs.
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

  const box = (el) => {
    if (!el) return null;
    const rect = el.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom };
  };

  const rect = dialog.getBoundingClientRect();
  const sheet = dialog.querySelector('.panel-nav-sheet');
  const trigger = document.querySelector('.panel-nav-trigger');

  const destination = (link) => ({
    href: link.getAttribute('href'),
    dest: link.getAttribute('data-nav-dest'),
    label: (link.textContent ?? '').trim().replace(/\s+/g, ' '),
    tag: link.tagName.toLowerCase(),
    closesOnClick: link.closest('[data-nav-close]') !== null,
  });

  const groups = [...dialog.querySelectorAll('.panel-nav-group')].map((group) => ({
    label: (group.querySelector('.panel-nav-label')?.textContent ?? '').trim(),
    items: [...group.querySelectorAll('.panel-nav-link')].filter(visible).map(destination),
  }));

  const radii = new Map();
  const contrasts = [];
  const smallTargets = [];

  for (const el of dialog.querySelectorAll('*')) {
    if (!(el instanceof HTMLElement) || !visible(el)) continue;
    const style = getComputedStyle(el);
    const elBox = el.getBoundingClientRect();

    if (style.borderRadius && style.borderRadius !== '0px') {
      radii.set(style.borderRadius, (radii.get(style.borderRadius) ?? 0) + 1);
    }

    const ownText = [...el.childNodes].some((node) => node.nodeType === Node.TEXT_NODE && node.textContent.trim());
    if (ownText) {
      const size = Math.round(parseFloat(style.fontSize));
      const fg = parseColor(style.color);
      if (fg) {
        const bg = backgroundOf(el);
        const fgRgb = blend(fg, bg);
        const l1 = luminance(fgRgb);
        const l2 = luminance(bg);
        const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
        contrasts.push({
          ratio: Math.round(ratio * 100) / 100,
          size,
          text: el.textContent.trim().slice(0, 36),
          cls: String(el.className).slice(0, 40),
        });
      }
    }

    const interactive = ['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName) || el.hasAttribute('tabindex');
    // 0.5px of slack: `min-height: 44px` is exact at DPR 1, but subpixel
    // layout can report 43.99 for a rule that does hit the token.
    if (interactive && (elBox.width < min - 0.5 || elBox.height < min - 0.5)) {
      const name = el.getAttribute('aria-label') || el.textContent.trim() || el.getAttribute('placeholder') || '';
      smallTargets.push(`${el.tagName.toLowerCase()} ${Math.round(elBox.width)}x${Math.round(elBox.height)} "${String(name).trim().slice(0, 24)}"`);
    }
  }

  contrasts.sort((a, b) => a.ratio - b.ratio);

  let backdrop = null;
  try {
    const css = getComputedStyle(dialog, '::backdrop').backgroundColor;
    const color = parseColor(css);
    backdrop = color ? { css, r: color.r, g: color.g, b: color.b, a: color.a } : { css, a: 0 };
  } catch {
    backdrop = null;
  }

  // The Web Animations API exposes `playState`; `state` is not a property of
  // Animation, so filtering on it silently matched nothing and the assert could
  // never fail. Read the real property and treat a paused/finished animation as
  // settled; a CSS animation that never stops (`infinite`) is "running".
  const running = document.getAnimations()
    .filter((animation) => animation.playState === 'running')
    .map((animation) => ({
      type: animation.constructor.name,
      name: animation.animationName ?? animation.transitionProperty ?? '',
      target: String(animation.effect?.target?.className ?? '').slice(0, 40),
    }));

  return {
    present: true,
    open: dialog.open,
    expanded: trigger?.getAttribute('aria-expanded') ?? null,
    rect: box(dialog),
    sheet: box(sheet),
    viewport: { width: innerWidth, height: innerHeight },
    backdrop,
    bodyOverflow: getComputedStyle(document.body).overflow,
    bodyInlineOverflow: document.body.style.overflow,
    groups,
    radii: Object.fromEntries(radii),
    contrastMin: contrasts[0] ?? null,
    contrastFailures: contrasts.filter((item) => item.ratio < (item.size >= 24 ? 3 : 4.5)),
    smallTargets,
    running,
  };
});

/* ------------------------------------------------------------------------- */
/* Playwright plumbing.                                                      */
/* ------------------------------------------------------------------------- */

const installClubFixtures = async (page) => {
  await page.route('**/v1/club/**', async (route) => {
    const url = route.request().url();
    const json = (body) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
    if (url.includes('/club/dashboard')) return json(FIXTURES.dashboard);
    if (url.includes('/club/profile')) return json(FIXTURES.profile);
    if (url.includes('/club/djs')) return json(FIXTURES.djs);
    if (url.includes('/club/pricing')) return json(FIXTURES.pricing);
    if (url.includes('/club/earnings')) return json(FIXTURES.earnings);
    return json({});
  });
};

/** Click the burger and wait for the dialog to be genuinely open. */
const openDrawer = async (page) => {
  const trigger = page.locator('.panel-nav-trigger').first();
  try {
    if ((await trigger.count()) === 0) return false;
    await trigger.click({ timeout: 4000 });
    await page.waitForSelector('dialog.panel-nav-drawer[open]', { timeout: 4000 });
    await page.waitForTimeout(150);
    return true;
  } catch {
    return false;
  }
};

/** The drawer's destination key: its declared dest, else its href. */
const destinationKey = (item) => item.dest ?? item.href ?? `button:${item.tag}`;

/** Wait until no open drawer is left, tolerating an in-flight navigation. */
const closeDrawer = async (page) => {
  for (let attempt = 0; attempt < 24; attempt += 1) {
    try {
      const open = await page.evaluate(() => Boolean(document.querySelector('dialog.panel-nav-drawer[open]')));
      if (!open) {
        await page.waitForTimeout(120);
        return true;
      }
    } catch {
      // The destination click started a navigation: the execution context is
      // being replaced, so ask the next document rather than failing.
    }
    await page.waitForTimeout(100);
  }
  return false;
};

/**
 * Assert 7: the drawer must carry every destination the rail carries, in the
 * same groups, with the same count, under the same labels.
 *
 * The comparison is a multiset on the destination key, because the admin rail
 * lists `/platform-settings` twice (Economics and Integrations) and a set would
 * let one of them disappear unnoticed. Labels are compared separately: the
 * design contract is that a destination cannot read differently in the drawer
 * than in the rail.
 */
const compareDestinations = (railGroups, drawerGroups) => {
  const missing = [];
  const extra = [];
  const countMismatch = [];
  const relabelled = [];

  const counts = (items) => {
    const map = new Map();
    for (const item of items) map.set(destinationKey(item), (map.get(destinationKey(item)) ?? 0) + 1);
    return map;
  };

  for (const railGroup of railGroups) {
    const drawerGroup = drawerGroups.find((group) => group.label === railGroup.label);
    if (!drawerGroup) {
      missing.push(`group "${railGroup.label}" is absent from the drawer (it carries ${railGroup.items.length} destination(s))`);
      continue;
    }
    const railCounts = counts(railGroup.items);
    const drawerCounts = counts(drawerGroup.items);
    for (const [key, count] of railCounts) {
      const found = drawerCounts.get(key) ?? 0;
      if (found < count) {
        const item = railGroup.items.find((candidate) => destinationKey(candidate) === key);
        missing.push(`${railGroup.label} › ${item.label}${item.href ? ` → ${item.href}` : ''}${count > 1 ? ` (x${count - found} missing)` : ''}`);
      }
    }
    for (const [key, count] of drawerCounts) {
      if (count > (railCounts.get(key) ?? 0)) {
        const item = drawerGroup.items.find((candidate) => destinationKey(candidate) === key);
        extra.push(`${railGroup.label} › ${item.label} → ${item.href ?? key}`);
      }
    }
    if (drawerGroup.items.length !== railGroup.items.length) {
      countMismatch.push(`"${railGroup.label}": rail ${railGroup.items.length} vs drawer ${drawerGroup.items.length}`);
    }
    // Pair destinations by key and by order of occurrence, then compare labels.
    const used = new Set();
    for (const railItem of railGroup.items) {
      const key = destinationKey(railItem);
      const match = drawerGroup.items.find((candidate, index) => !used.has(index) && destinationKey(candidate) === key);
      if (!match) continue;
      used.add(drawerGroup.items.indexOf(match));
      if (match.label !== railItem.label) relabelled.push(`"${railItem.label}" → "${match.label}" (${railItem.href ?? key})`);
    }
  }

  const unknownGroups = drawerGroups.filter((group) => !railGroups.some((rail) => rail.label === group.label)).map((group) => group.label);
  const ok = missing.length === 0 && extra.length === 0 && countMismatch.length === 0 && unknownGroups.length === 0 && relabelled.length === 0;
  const total = railGroups.reduce((sum, group) => sum + group.items.length, 0);
  const detail = ok
    ? `${railGroups.length} groups, ${total} destinations — identical in the drawer`
    : [
      missing.length ? `missing on mobile: ${missing.join(' | ')}` : '',
      extra.length ? `not in the rail: ${extra.join(' | ')}` : '',
      countMismatch.length ? `count differs: ${countMismatch.join(' | ')}` : '',
      relabelled.length ? `label differs from the rail: ${relabelled.join(' | ')}` : '',
      unknownGroups.length ? `group label not in the rail: ${unknownGroups.join(' | ')}` : '',
    ].filter(Boolean).join('\n         ');
  return { ok, detail };
};

const browser = await chromium.launch();

try {
  for (const [appName, app] of targets) {
    const outDir = join(REPO, 'output/mobile-nav', appName);
    mkdirSync(outDir, { recursive: true });

    for (const theme of THEMES) {
      const tag = `${appName} [${theme}]`;
      const context = await browser.newContext({
        viewport: DESKTOP,
        deviceScaleFactor: 1,
        // The sheet's entrance animation is inside `no-preference`, so this is
        // both the settled-geometry context and the assert-16 context.
        reducedMotion: 'reduce',
      });
      const page = await context.newPage();
      const consoleErrors = [];
      page.on('console', (message) => { if (message.type() === 'error') consoleErrors.push(message.text()); });
      page.on('pageerror', (error) => consoleErrors.push(`pageerror: ${error.message}`));

      await page.addInitScript((value) => {
        try { window.localStorage.setItem('dropit-theme', value); } catch { /* ignore */ }
      }, theme);

      if (appName === 'club') await installClubFixtures(page);

      try {
        await page.goto(`${app.base}${app.route}`, { waitUntil: 'networkidle', timeout: 30000 });
        await page.waitForTimeout(400);

        /* ---------------------------------------------------------------- */
        /* Desktop: the rail, and the bar that must be gone.                */
        /* ---------------------------------------------------------------- */
        const desktop = await readShell(page);

        assert(1, desktop.rail.visible && desktop.triggers.visible === 0, [
          `rail visible=${desktop.rail.visible}`,
          `trigger visible=${desktop.triggers.visible} of ${desktop.triggers.total}`,
        ].join(', '), tag);

        assert(2, desktop.mobileNav === 0 && desktop.mobileLinks === 0, [
          `.panel-mobile-nav=${desktop.mobileNav}`,
          `.panel-mobile-link=${desktop.mobileLinks}`,
        ].join(', '), tag);

        // Overflow ladder. 3 resizes the page, so it runs before the rail
        // baseline is read; the ladder ends at 1440.
        const overflows = {};
        for (const width of WIDTHS) {
          await page.setViewportSize({ width, height: width <= 768 ? MOBILE.height : DESKTOP.height });
          await page.waitForTimeout(180);
          overflows[width] = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
        }
        const overflowing = Object.entries(overflows).filter(([, value]) => value > 0);
        assert(3, overflowing.length === 0, JSON.stringify(overflows), tag);

        const railGroups = await readRail(page);

        /* ---------------------------------------------------------------- */
        /* Mobile: the burger, the drawer, and the contract around them.    */
        /* ---------------------------------------------------------------- */
        await page.setViewportSize(MOBILE);
        // Scope assert 13 to this page load: "during open/close cycles".
        consoleErrors.length = 0;
        await page.goto(`${app.base}${app.route}`, { waitUntil: 'networkidle', timeout: 30000 });
        await page.waitForTimeout(400);

        const mobile = await readShell(page);
        const overflowBefore = mobile.bodyOverflow;

        try {
          await page.screenshot({ path: join(outDir, `closed-${theme}.png`) });
        } catch { /* assert 17 checks the file list, not this call */ }

        assert(4, mobile.rail.visible === false && mobile.triggers.visible === 1, [
          `rail visible=${mobile.rail.visible}`,
          `visible triggers=${mobile.triggers.visible} of ${mobile.triggers.total}`,
        ].join(', '), tag);

        assert(5, Boolean(mobile.burger) && mobile.burger.label !== '' && mobile.burger.haspopup === 'dialog', mobile.burger
          ? `name="${mobile.burger.label}" aria-haspopup="${mobile.burger.haspopup}" aria-expanded="${mobile.burger.expanded}"`
          : 'no visible .panel-nav-trigger to read', tag);

        const opened = await openDrawer(page);
        const drawer = opened ? await measureDrawer(page, MIN_TARGET) : null;

        assert(6, Boolean(drawer && drawer.open && drawer.expanded === 'true' && mobile.burger?.expanded === 'false'),
          drawer
            ? `dialog open=${drawer.open}, aria-expanded ${mobile.burger?.expanded ?? 'n/a'} → ${drawer.expanded}`
            : 'no dialog.panel-nav-drawer[open] appeared after clicking the burger', tag);

        if (drawer) {
          try {
            await page.screenshot({ path: join(outDir, `open-${theme}.png`) });
          } catch { /* assert 17 checks the file list, not this call */ }
        }

        if (!drawer) {
          // Keep the run readable and non-crashing: everything downstream needs
          // an open drawer, so each assert reports why it could not run.
          for (const number of [7, 8, 9, 10, 11, 12, 13, 14, 15, 16]) {
            assert(number, false, 'blocked: the drawer did not open', tag);
          }
        } else {
          /* 7. destination parity with the desktop rail ------------------ */
          const parity = compareDestinations(railGroups, drawer.groups);
          const noBaseline = railGroups.reduce((sum, group) => sum + group.items.length, 0) === 0;
          assert(7, parity.ok && !noBaseline, noBaseline
            ? 'desktop rail exposed no destinations to compare against'
            : parity.detail, tag);

          /* 8. geometry + scrim ------------------------------------------ */
          const viewport = drawer.viewport;
          const geometry = [];
          const withinViewport = (label, box) => {
            const inside = box !== null
              && box.x >= -EPSILON
              && box.right <= viewport.width + EPSILON
              && box.y >= -EPSILON
              && box.bottom <= viewport.height + EPSILON;
            geometry.push(`${label} ${Math.round(box.x)},${Math.round(box.y)} ${Math.round(box.width)}x${Math.round(box.height)} (viewport ${viewport.width}x${viewport.height})`);
            return inside;
          };
          const dialogInside = withinViewport('drawer', drawer.rect);
          const sheetInside = drawer.sheet ? withinViewport('sheet', drawer.sheet) : false;
          const fullHeight = Math.abs(drawer.rect.height - viewport.height) <= 1;
          const scrim = drawer.backdrop !== null && drawer.backdrop.a > 0;

          // The scrim "covers the page" when a point outside the sheet belongs
          // to the dialog, not to the page behind it. Probe first and only then
          // click: a click that landed through the scrim would navigate the
          // page and fail this gate for the wrong reason.
          const probes = [
            { x: viewport.width - 24, y: Math.round(viewport.height / 2) },
            { x: Math.round((drawer.rect.right + viewport.width) / 2), y: 40 },
          ];
          const probeHits = await page.evaluate((points) => points.map((point) => {
            const el = document.elementFromPoint(point.x, point.y);
            return { ...point, hit: el ? `${el.tagName.toLowerCase()}.${String(el.className).slice(0, 30)}` : null, onDialog: el?.classList.contains('panel-nav-drawer') ?? false };
          }), probes);
          const scrimCovers = probeHits.every((probe) => probe.onDialog);
          let scrimClosed = false;
          if (scrimCovers) {
            await page.mouse.click(probes[0].x, probes[0].y);
            scrimClosed = await closeDrawer(page);
          }
          assert(8, dialogInside && sheetInside && fullHeight && scrim && scrimCovers && scrimClosed, [
            ...geometry,
            `full height=${fullHeight}`,
            `::backdrop=${drawer.backdrop?.css ?? 'unresolved'} alpha=${drawer.backdrop?.a ?? 0}`,
            `elementFromPoint outside the sheet: ${probeHits.map((probe) => probe.hit ?? 'none').join(' / ')}`,
            scrimClosed ? 'tap outside closes it' : 'tap outside did NOT close it',
          ].join(', '), tag);

          /* 9. touch targets --------------------------------------------- */
          assert(9, drawer.smallTargets.length === 0, drawer.smallTargets.slice(0, 6).join(' | ') || 'all interactive elements >= 44x44', tag);

          /* 14. contrast -------------------------------------------------- */
          assert(14, drawer.contrastFailures.length === 0, drawer.contrastFailures.length
            ? `${drawer.contrastFailures.length} below AA, worst: ${JSON.stringify(drawer.contrastFailures.slice(0, 3))}`
            : `min ${drawer.contrastMin?.ratio}:1 on "${drawer.contrastMin?.text}" (${drawer.contrastMin?.size}px)`, tag);

          /* 15. radius contract ------------------------------------------ */
          const strayRadii = Object.keys(drawer.radii).filter((radius) => !ALLOWED_RADII.has(radius));
          assert(15, strayRadii.length === 0, strayRadii.length ? `stray: ${strayRadii.join(', ')}` : JSON.stringify(drawer.radii), tag);

          /* 16. reduced motion ------------------------------------------- */
          assert(16, drawer.running.length === 0, drawer.running.length
            ? `${drawer.running.length} running: ${JSON.stringify(drawer.running.slice(0, 4))}`
            : '0 running animations', tag);

          /* 12. body scroll lock + restore ------------------------------- */
          // Assert 8 may already have closed it; if not, close it now so the
          // restore half is measured on a genuinely closed drawer.
          if (!scrimClosed) await page.keyboard.press('Escape').catch(() => {});
          await closeDrawer(page);
          const afterClose = await page.evaluate(() => getComputedStyle(document.body).overflow);
          assert(12, drawer.bodyOverflow === 'hidden' && afterClose === overflowBefore, [
            `open: computed overflow=${drawer.bodyOverflow} (inline "${drawer.bodyInlineOverflow}")`,
            `closed: computed overflow=${afterClose}, baseline=${overflowBefore}`,
          ].join(', '), tag);

          /* 10. Escape closes and returns focus --------------------------- */
          const reopenedForEscape = await openDrawer(page);
          let escapeClosed = false;
          let focusReturned = false;
          if (reopenedForEscape) {
            await page.keyboard.press('Escape');
            escapeClosed = await closeDrawer(page);
            focusReturned = await page.evaluate(() => document.activeElement?.classList.contains('panel-nav-trigger') ?? false);
          }
          assert(10, reopenedForEscape && escapeClosed && focusReturned,
            !reopenedForEscape ? 'drawer did not reopen for the Escape check'
              : `closed=${escapeClosed}, focus on burger=${focusReturned}`, tag);

          /* 11. a destination closes the drawer and navigates ------------- */
          const reopenedForNav = await openDrawer(page);
          let navigation = null;
          if (reopenedForNav) {
            const before = new URL(page.url());
            // Mark the current document. A router navigation keeps this marker;
            // a full page reload loses it. Without the distinction a drawer that
            // never closes would still "pass" here, because reloading the page
            // removes the dialog and changes the URL by itself.
            await page.evaluate(() => { window.__navGateMarker = 'same-document'; });
            const target = await page.evaluate(() => {
              const links = [...document.querySelectorAll('dialog.panel-nav-drawer .panel-nav-link')];
              const index = links.findIndex((link) => {
                const href = link.getAttribute('href');
                if (!href) return false;
                return new URL(href, location.href).pathname !== location.pathname;
              });
              const chosen = index >= 0 ? index : 0;
              return links[chosen]
                ? { index: chosen, href: links[chosen].getAttribute('href'), label: links[chosen].textContent.trim().slice(0, 40) }
                : null;
            });
            if (target) {
              await page.locator('dialog.panel-nav-drawer .panel-nav-link').nth(target.index).click({ timeout: 4000 }).catch(() => {});
            }
            const closedAfterNav = await closeDrawer(page);
            const after = new URL(page.url());
            const navigated = before.pathname !== after.pathname;
            // A destination that points at the route already open cannot change
            // the URL; closing that drawer is still the whole contract.
            const alreadyHere = target ? before.pathname === new URL(target.href ?? before.href, before).pathname : false;
            const sameDocument = await page.evaluate(() => window.__navGateMarker === 'same-document').catch(() => false);
            navigation = {
              ok: Boolean(target) && closedAfterNav && sameDocument && (navigated || alreadyHere),
              detail: target
                ? `clicked "${target.label}" → ${target.href}; closed=${closedAfterNav}, ${before.pathname} → ${after.pathname}${sameDocument ? '' : ', but the drawer closed by reloading the page instead of handling the click'}`
                : 'the drawer exposed no .panel-nav-link to click',
            };
          }
          assert(11, Boolean(navigation?.ok), navigation?.detail ?? 'drawer did not reopen for the destination click', tag);

          /* 13. console cleanliness over every open/close cycle ----------- */
          assert(13, consoleErrors.length === 0, consoleErrors.slice(0, 4).join(' | ') || 'clean across the cycles', tag);
        }
      } catch (error) {
        blocked(`page did not load: ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`, tag);
      }

      await context.close();
    }

    /* ------------------------------------------------------------------ */
    /* 17. the four screenshots at 390x844                                */
    /* ------------------------------------------------------------------ */
    const shots = ['closed-light.png', 'closed-dark.png', 'open-light.png', 'open-dark.png'];
    const missing = shots.filter((shot) => !existsSync(join(outDir, shot)));
    assert(17, missing.length === 0, missing.length
      ? `missing: ${missing.join(', ')} in output/mobile-nav/${appName}/`
      : `${shots.length} files in output/mobile-nav/${appName}/`, `${appName} @${MOBILE.width}x${MOBILE.height}`);
  }
} finally {
  await browser.close();
}

/* ------------------------------------------------------------------------- */
/* Artifacts + summary.                                                      */
/* ------------------------------------------------------------------------- */
const outRoot = join(REPO, 'output/mobile-nav');
mkdirSync(outRoot, { recursive: true });
const report = {
  generatedAt: new Date().toISOString(),
  targets: Object.fromEntries(targets.map(([name, app]) => [name, `${app.base}${app.route}`])),
  passed: failures === 0,
  failures,
  checks: results.length,
  results,
};
try {
  writeFileSync(join(outRoot, 'report.json'), `${JSON.stringify(report, null, 2)}\n`);
  const lines = [
    '# Mobile navigation — acceptance report',
    '',
    `Generated: ${report.generatedAt}`,
    `Targets: ${Object.entries(report.targets).map(([name, url]) => `${name} ${url}`).join(' · ')}`,
    `Result: **${report.passed ? 'GREEN' : `RED (${failures} failing check(s))`}** (${report.checks} checks)`,
    '',
    '| # | Assert | Context | Result | Detail |',
    '|---|---|---|---|---|',
    ...results.map((item) => `| ${item.number} | ${item.name} | ${item.tag} | ${item.ok ? 'pass' : '**FAIL**'} | ${String(item.detail ?? '').replace(/\|/g, '\\|').replace(/\n\s*/g, ' ').slice(0, 240)} |`),
    '',
    'Screenshots: `output/mobile-nav/<app>/{closed,open}-{light,dark}.png` at 390x844.',
    '',
  ];
  writeFileSync(join(outRoot, 'report.md'), `${lines.join('\n')}\n`);
  console.log(`\nreport: output/mobile-nav/report.md`);
} catch (error) {
  console.log(`\nreport not written: ${error instanceof Error ? error.message : String(error)}`);
}

const failed = results.filter((item) => !item.ok);
if (failed.length > 0) {
  console.log(`\n${failed.length} failing check(s):`);
  for (const item of failed) console.log(`  ${String(item.number).padStart(2, ' ')}. ${item.name} — ${item.tag}`);
}
console.log(`\n${failures === 0 ? 'MOBILE NAV GREEN' : `MOBILE NAV FAILED — ${failures} failing check(s)`} (${results.length} checks)`);
process.exit(failures === 0 ? 0 : 1);
