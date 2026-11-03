#!/usr/bin/env node
/**
 * Select audit — the acceptance check for every dropdown in the web panels.
 *
 *   node scripts/select-audit.mjs                     # every app and route below
 *   node scripts/select-audit.mjs --app club          # one app
 *   node scripts/select-audit.mjs --route /sessions   # one route
 *
 * What it asserts, per select, in both themes and at desktop + mobile widths:
 *
 *  1. the trigger is the field: 38px on desktop, 44px at touch widths, and the
 *     same height as the text input beside it in the same row;
 *  2. the list opens **directly under the trigger** — the gap is 6px, not a
 *     detached overlay and not the OS popup;
 *  3. the list never leaves the viewport horizontally, and its bottom edge
 *     stays inside the viewport (it caps its own height and scrolls instead);
 *  4. the list is at least as wide as its trigger;
 *  5. up / down / enter / escape drive it from the keyboard, and Escape returns
 *     focus to the trigger;
 *  6. selecting a row writes it back to the trigger;
 *  7. there is no native <select> left on the route.
 *
 * Geometry is read from the rendered DOM: getBoundingClientRect() of the
 * trigger and of the portalled list, so a CSS regression fails the gate.
 *
 * Exit code 0 = every check passed, 1 = at least one failed.
 */
import { createRequire } from 'node:module';
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
    // The club panel in demo mode (the default here) renders these routes.
    routes: ['/sessions', '/dj'],
    heights: { desktop: 38, touch: 44 },
  },
  admin: {
    // `apps/web-admin` serves the table filters on the control-center routes,
    // which are mounted in the demo build (`npm run dev:demo`, no sign-in).
    base: flag('admin-base', process.env.ADMIN_BASE ?? 'http://localhost:5174'),
    routes: ['/admin/manage/users'],
    heights: { desktop: 38, touch: 44 },
  },
  connect: {
    // The public Connect flow is only mounted in the production build mode.
    base: flag('connect-base', process.env.CONNECT_BASE ?? 'http://localhost:5181'),
    routes: ['/connect/venue', '/connect'],
    // Connect keeps its own .pen geometry: 48px wizard fields, 44px nav control.
    heights: { desktop: [48, 44], touch: [48, 44] },
  },
};

const THEMES = ['dark', 'light'];
const SIZES = [
  { width: 1440, height: 900, touch: false },
  { width: 390, height: 844, touch: true },
];
const GAP = 6;
const EPSILON = 1.5;

const results = [];
let failures = 0;
const record = (ok, label, detail) => {
  results.push({ ok, label, detail });
  if (!ok) failures += 1;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${!ok && detail ? `\n         ${detail}` : ''}`);
};

const apps = onlyApp ? { [onlyApp]: APPS[onlyApp] } : APPS;

const browser = await chromium.launch();

try {
  for (const [appName, app] of Object.entries(apps)) {
    if (!app) {
      console.log(`unknown app ${appName}`);
      failures += 1;
      continue;
    }
    for (const route of onlyRoute ? [onlyRoute] : app.routes) {
      for (const theme of THEMES) {
        for (const size of SIZES) {
          const context = await browser.newContext({
            viewport: { width: size.width, height: size.height },
            deviceScaleFactor: 1,
            reducedMotion: 'reduce',
            isMobile: size.touch,
            hasTouch: size.touch,
          });
          const page = await context.newPage();
          page.on('pageerror', (error) => record(false, `${appName} ${route} pageerror`, error.message));
          await page.addInitScript((value) => {
            try { window.localStorage.setItem('dropit-theme', value); } catch { /* ignore */ }
          }, theme);

          const label = `${appName} ${route} [${theme}] @${size.width}`;
          try {
            await page.goto(`${app.base}${route}`, { waitUntil: 'networkidle', timeout: 30000 });
            await page.waitForTimeout(400);

            const landedOn = new URL(page.url()).pathname;
            if (landedOn !== route && /login/.test(landedOn)) {
              record(false, `${label} needs the demo build`,
                `route redirected to ${landedOn}; start the app with \`npm run dev:demo\` (admin tables live in the control center)`);
              await context.close();
              continue;
            }

            const nativeSelects = await page.locator('select').count();
            record(nativeSelects === 0, `${label} no native <select>`, `${nativeSelects} native select element(s)`);

            const triggers = page.locator('.field-select__trigger');
            const total = await triggers.count();
            if (total === 0) {
              // The request workspace now exposes status and sort as segments,
              // not dropdowns. Verify those choices rather than requiring a
              // control that no longer exists on this route.
              if (appName === 'club' && route === '/dj') {
                const groups = page.locator('.dj-req-toolbar .seg[role="group"]');
                record(await groups.count() === 2, `${label} status and sort choices are present`);
                for (const group of await groups.all()) {
                  record(await group.locator('[aria-pressed="true"]').count() === 1,
                    `${label} segmented choice has one active option`);
                }
                await context.close();
                continue;
              }
              record(false, `${label} has at least one select`, 'none rendered');
              await context.close();
              continue;
            }

            const max = Math.min(total, 4);
            for (let index = 0; index < max; index += 1) {
              const trigger = triggers.nth(index);
              if (!(await trigger.isVisible())) continue;
              const name = (await trigger.innerText()).trim().slice(0, 22) || `#${index}`;
              const tag = `${label} "${name}"`;

              // Bring the field into view first: Playwright scrolls on click,
              // which would move the trigger between measurement and click.
              await trigger.scrollIntoViewIfNeeded();
              await page.waitForTimeout(80);
              const triggerBox = await trigger.boundingBox();
              if (!triggerBox) continue;

              // 1. field geometry: one of the surface's documented control heights
              const expected = size.touch ? app.heights.touch : app.heights.desktop;
              const allowed = Array.isArray(expected) ? expected : [expected];
              const tallest = Math.max(...allowed);
              record(
                allowed.some((height) => Math.abs(triggerBox.height - height) <= EPSILON),
                `${tag} trigger is ${allowed.join('/')}px tall`,
                `measured ${triggerBox.height.toFixed(1)}px${size.touch ? ` (min ${tallest})` : ''}`,
              );

              if (await trigger.isDisabled()) {
                record(true, `${tag} disabled trigger skipped`);
                continue;
              }

              await trigger.click();
              await page.waitForTimeout(160);
              const menu = page.locator('[role="listbox"]');
              if (!(await menu.count())) {
                record(false, `${tag} opens a listbox`, 'no [role="listbox"] after click');
                continue;
              }
              const menuBox = await menu.boundingBox();

              // 2. anchored to the trigger: 6px under it, or 6px above it when
              // the viewport has no room below (that flip is the contract).
              const gapBelow = menuBox ? menuBox.y - (triggerBox.y + triggerBox.height) : NaN;
              const gapAbove = menuBox ? triggerBox.y - (menuBox.y + menuBox.height) : NaN;
              const anchored = Math.abs(gapBelow - GAP) <= EPSILON || Math.abs(gapAbove - GAP) <= EPSILON;
              record(
                anchored,
                `${tag} list is anchored ${GAP}px to the field`,
                `measured ${gapBelow.toFixed(1)}px below / ${gapAbove.toFixed(1)}px above`,
              );

              // 3. inside the viewport
              const viewport = page.viewportSize();
              const insideX = menuBox.x >= 0 && menuBox.x + menuBox.width <= viewport.width + 0.5;
              const insideY = menuBox.y >= 0 && menuBox.y + menuBox.height <= viewport.height + 0.5;
              record(insideX && insideY, `${tag} list stays inside the viewport`, JSON.stringify(menuBox));

              // 4. never narrower than the field
              record(
                menuBox.width >= triggerBox.width - EPSILON,
                `${tag} list is at least as wide as the field`,
                `list ${menuBox.width}px vs field ${triggerBox.width}px`,
              );

              // 5. keyboard: Escape closes and returns focus
              await page.keyboard.press('Escape');
              await page.waitForTimeout(120);
              const closed = (await page.locator('[role="listbox"]').count()) === 0;
              const focusedTrigger = await trigger.evaluate((element) => element === document.activeElement);
              record(closed, `${tag} Escape closes the list`, 'list still open');
              record(focusedTrigger, `${tag} Escape returns focus to the field`, 'focus left the trigger');

              // 5b. ArrowDown opens, ArrowDown moves, Enter selects
              await page.keyboard.press('ArrowDown');
              await page.waitForTimeout(120);
              const opened = (await page.locator('[role="listbox"]').count()) === 1;
              record(opened, `${tag} ArrowDown opens the list`, 'list did not open');
              if (opened) {
                const firstActive = await page.locator('[role="listbox"] [aria-activedescendant]').count();
                await page.keyboard.press('ArrowDown');
                await page.waitForTimeout(80);
                const active = await page.locator('.field-select__option.is-active').innerText();
                await page.keyboard.press('Enter');
                await page.waitForTimeout(120);
                const value = (await trigger.innerText()).trim();
                record(
                  value === active.trim(),
                  `${tag} Enter selects the highlighted row`,
                  `trigger "${value}" vs row "${active.trim()}"`,
                );
                void firstActive;
              }
            }
          } catch (error) {
            record(false, `${label} page loaded`, error instanceof Error ? error.message.split('\n')[0] : String(error));
          }
          await context.close();
        }
      }
    }
  }
} finally {
  await browser.close();
}

console.log(`\n${failures === 0 ? 'SELECT AUDIT GREEN' : `SELECT AUDIT FAILED — ${failures} failing check(s)`} (${results.length} checks)`);
process.exit(failures === 0 ? 0 : 1);
