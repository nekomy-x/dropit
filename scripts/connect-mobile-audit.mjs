#!/usr/bin/env node
/**
 * Connect mobile audit — the invariant gate for the public `/connect` landing on
 * a phone.
 *
 *   node scripts/connect-mobile-audit.mjs
 *   node scripts/connect-mobile-audit.mjs --base http://150.241.68.57
 *   node scripts/connect-mobile-audit.mjs --base http://localhost:5173 --json output/mobile-fix/after.json
 *
 * Flags
 *   --base URL    origin to audit (default http://localhost:5173). `/connect` is
 *                 appended unless the URL already ends with it.
 *   --json PATH   write the full structured result (measurements included).
 *   --quiet       print the matrix and summary only, no per-run detail.
 *
 * Why this exists
 * ---------------
 * The landing breaks in ways a desktop screenshot cannot show: the header logo
 * is squeezed until it paints over the language switch, the drawer loses its
 * sign-in row, the hero keeps a fixed height that leaves a photo-only dead zone,
 * and the type scale drops to 8px on a phone. Those are measured, not eyeballed.
 *
 * The audit asserts invariants, never the current numbers:
 *
 *   1. header   — the logo link, the language Select trigger and the burger do
 *                 not overlap; the logo link is at least as wide as the lockup
 *                 it paints; the painted lockup does not collide with either
 *                 neighbour; no header child leaves the viewport.
 *   2. drawer   — open by the burger: its background is opaque (alpha >= 0.99),
 *                 the "Sign in" row exists with a non-zero box, and every row
 *                 inside it is at least 44px tall.
 *   3. targets  — at <= 640px every interactive element keeps a 44x44 target
 *                 (docs/ui/PANEL-DESIGN.md: "every control that is reachable by
 *                 thumb keeps a 44px target down to 320px"). Only links inside
 *                 running prose are exempt (the WCAG 2.5.8 inline exception);
 *                 a nav or footer row is a control and must pass.
 *   4. type     — at phone widths no visible text computes below 11px.
 *   5. overflow — documentElement.scrollWidth equals the viewport width at
 *                 320/360/390/414/768. The `.ticker-track` marquee is a clipped
 *                 animation and is not an offender.
 *   6. hero     — the gap between the hero's actions row and the hero's bottom
 *                 edge stays under 80px at phone widths (the fixed 580px
 *                 min-height used to leave 90-230px of empty photo there).
 *   7. clip     — no element clips its own text (scrollWidth > clientWidth)
 *                 except `.connect-panel-url`, whose ellipsis is intentional.
 *   8. nav mode — below the desktop breakpoint the burger is the one navigation
 *                 control; the inline nav must not appear beside it.
 *
 * Two states are measured at every width: the closed page and the drawer opened
 * by the burger. 1024 is included as a regression width: it is still burger
 * territory (the desktop nav starts at 1280) and it is where the old markup
 * first showed the inline nav again.
 *
 * Exit code 0 = every applicable check passed, 1 = at least one failed (a page
 * that does not load fails every check for that run), 2 = the browser could not
 * be launched at all.
 */
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');
const require = createRequire(join(REPO, 'apps/web-club/package.json'));
const { chromium } = require('playwright');

const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const index = args.indexOf(`--${name}`);
  return index >= 0 && args[index + 1] ? args[index + 1] : fallback;
};
const has = (name) => args.includes(`--${name}`);

const BASE = flag('base', 'http://localhost:5173').replace(/\/+$/, '');
const JSON_PATH = flag('json', null);
const QUIET = has('quiet');

const HEIGHT = 844;
// Phone widths. The design rule under test ("a 44px target down to 320px") is a
// phone rule, and 768 is the widest mobile-layout width in the matrix.
const PHONE_WIDTHS = [320, 360, 390, 414];
const TABLET_WIDTHS = [768];
// The desktop nav starts at 1280px; 1024 must still be the burger, so it is
// audited for regressions rather than treated as a phone.
const REGRESSION_WIDTHS = [1024];
const WIDTHS = [...PHONE_WIDTHS, ...TABLET_WIDTHS, ...REGRESSION_WIDTHS];
const LOCALES = ['ru-RU', 'en-US'];
const PHONE_LIMIT = 640;          // touch-target / type rules stop here
const MIN_TARGET = 44;            // docs/ui/PANEL-DESIGN.md §7
const MIN_FONT = 11;              // docs/ui/PANEL-DESIGN.md §4.6
const MAX_HERO_GAP = 80;          // px of photo-only space below the hero CTAs
const MIN_DRAWER_ALPHA = 0.99;    // the drawer must not show the page through it
const DESKTOP_NAV_WIDTH = 1280;   // min-[1280px]:flex in ConnectNav.tsx
const SETTLE_MS = 300;

const PAGE_URL = (() => {
  try {
    const url = new URL(BASE);
    return /\/connect\/?$/.test(url.pathname) ? BASE : `${BASE}/connect`;
  } catch {
    return `${BASE}/connect`;
  }
})();

const CHECKS = [
  { id: 'header', title: 'header: no logo/select/burger collision, logo not squeezed, inside viewport' },
  { id: 'drawer', title: 'drawer: opaque, sign-in visible, rows >= 44px' },
  { id: 'targets', title: 'tap targets >= 44x44 at <= 640px' },
  { id: 'type', title: 'no visible text below 11px at phone widths' },
  { id: 'overflow', title: 'no horizontal page overflow' },
  { id: 'hero', title: 'hero actions row sits < 80px above the hero bottom' },
  { id: 'clip', title: 'no clipped text (except .connect-panel-url)' },
  { id: 'nav', title: 'burger is the only navigation control below 1280px' },
];

/* ------------------------------------------------------------------------- *
 * Measurements, all taken inside the page.
 *
 * The helper block is stringified into the page by `evaluate`, so it must stay
 * self-contained: no imports, no Node globals.
 * ------------------------------------------------------------------------- */

const PAGE_HELPERS = () => {
  const rectOf = (el) => {
    if (!el) return null;
    const r = el.getBoundingClientRect();
    const round = (n) => Math.round(n * 100) / 100;
    return { x: round(r.x), y: round(r.y), width: round(r.width), height: round(r.height), right: round(r.right), bottom: round(r.bottom) };
  };

  const rendered = (el) => {
    if (!el || el.nodeType !== 1) return false;
    const style = getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse') return false;
    const box = el.getBoundingClientRect();
    if (box.width <= 0 || box.height <= 0) return false;
    let node = el;
    while (node && node.nodeType === 1) {
      const cs = getComputedStyle(node);
      if (Number(cs.opacity) === 0) return false;
      node = node.parentElement;
    }
    return true;
  };

  const label = (el) => {
    if (!el) return '?';
    const bits = [];
    let node = el;
    for (let depth = 0; depth < 3 && node && node.nodeType === 1 && node !== document.body; depth += 1) {
      const classes = String(node.getAttribute('class') || '').split(/\s+/).filter(Boolean).slice(0, 2);
      bits.unshift(`${node.tagName.toLowerCase()}${node.id ? `#${node.id}` : ''}${classes.length ? `.${classes.join('.')}` : ''}`);
      node = node.parentElement;
    }
    return bits.join('>');
  };

  // innerText skips `display:none` copies (the responsive CTA duplicates its
  // label for phone and desktop), which textContent would concatenate.
  const textOf = (el, limit = 56) => String(
    el?.getAttribute?.('aria-label')
    || (el?.innerText !== undefined ? el.innerText : el?.textContent)
    || '',
  )
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, limit);

  const section = (el) => {
    const found = el?.closest?.('section[id], section, header, footer');
    if (!found) return 'page';
    return `${found.tagName.toLowerCase()}${found.id ? `#${found.id}` : ''}`;
  };

  const clipsAt = (el) => {
    const cs = getComputedStyle(el);
    return cs.overflowX === 'hidden' || cs.overflowX === 'clip';
  };

  const hasClippingAncestor = (el) => {
    let node = el.parentElement;
    while (node && node !== document.documentElement) {
      if (clipsAt(node)) return true;
      node = node.parentElement;
    }
    return false;
  };

  const alphaOf = (color) => {
    if (!color || color === 'transparent') return 0;
    const match = color.match(/rgba?\(([^)]+)\)/i);
    if (!match) return 1;
    const parts = match[1].split(/[\s,/]+/).filter(Boolean).map(Number);
    return parts.length >= 4 ? parts[3] : 1;
  };

  const effectiveAlpha = (el, baseAlpha) => {
    let alpha = baseAlpha;
    let node = el;
    while (node && node.nodeType === 1) {
      alpha *= Number(getComputedStyle(node).opacity);
      node = node.parentElement;
    }
    return Math.round(alpha * 1000) / 1000;
  };

  const interactives = (root) => [...root.querySelectorAll(
    'a[href], button, input:not([type="hidden"]), select, textarea, [role="button"], [role="link"], [role="tab"], [role="switch"], [role="checkbox"], [tabindex]:not([tabindex="-1"])',
  )].filter((el) => !el.disabled && !el.closest('[aria-hidden="true"]') && rendered(el));

  // WCAG 2.5.8 (inline): a link that sits inside a running paragraph is
  // measured by its text run, not by a 44px hit area, and no amount of padding
  // can change that. A link in a nav list is NOT this case: each row is a thumb
  // control and has to reach 44px, which is exactly how a footer link column
  // fails on a phone. Only genuine prose is exempt, so the audit stays honest
  // about list rows.
  const isInlineTextLink = (el) => el.tagName === 'A'
    && getComputedStyle(el).display === 'inline'
    && Boolean(el.closest('p, figcaption, dd, blockquote'))
    && !el.closest('nav, ul, ol, li, footer');

  return { rectOf, rendered, label, textOf, section, clipsAt, hasClippingAncestor, alphaOf, effectiveAlpha, interactives, isInlineTextLink };
};

/** Closed-page measurement. */
async function measurePage(page) {
  return page.evaluate(({ helpersSource }) => {
    // eslint-disable-next-line no-new-func
    const H = new Function(`return (${helpersSource})()`)();
    const { rectOf, rendered, label, textOf, section, clipsAt, hasClippingAncestor } = H;

    // Chromium's mobile emulation grows window.innerWidth to fit overflowing
    // content, which would hide the very overflow this audit looks for. The
    // document element's clientWidth is the layout viewport and stays put.
    const viewportWidth = document.documentElement.clientWidth;

    const header = document.querySelector('.connect-linear-main > header') || document.querySelector('header');
    const burger = header ? [...header.querySelectorAll('button[aria-controls]')].find((el) => rendered(el)) || null : null;
    const drawer = (() => {
      const id = burger?.getAttribute('aria-controls');
      return id ? document.getElementById(id) : null;
    })();

    const logoLink = (() => {
      if (!header) return null;
      const brand = header.querySelector('.brand-logo');
      const containing = brand?.closest('a');
      if (containing) return containing;
      return [...header.querySelectorAll('a')].find((el) => rendered(el)) || header.querySelector('a');
    })();
    const brandLogo = header?.querySelector('.brand-logo') || null;
    const selectTrigger = header?.querySelector('.connect-select-nav .field-select__trigger')
      || [...(header?.querySelectorAll('.field-select__trigger') || [])].find((el) => rendered(el) && !drawer?.contains(el))
      || null;

    const headerOverflow = header
      ? [...header.querySelectorAll('*')]
        .filter((el) => rendered(el) && !drawer?.contains(el))
        .filter((el) => {
          const box = el.getBoundingClientRect();
          return box.right > viewportWidth + 1 || box.left < -1;
        })
        .slice(0, 6)
        .map((el) => ({ selector: label(el), text: textOf(el, 32), box: rectOf(el) }))
      : [];

    const desktopNav = header
      ? [...header.querySelectorAll('nav')].find((nav) => !drawer?.contains(nav) && rendered(nav)) || header.querySelector('nav')
      : null;
    const navLinkCount = desktopNav
      ? [...desktopNav.querySelectorAll('a')].filter((a) => rendered(a)).length
      : 0;

    // The header sign-in label, used later to find the drawer's sign-in row
    // without hardcoding a translation. The desktop copy is display:none on a
    // phone but still carries the text.
    const headerSignIn = header
      ? [...header.querySelectorAll('a[href]')]
        .filter((a) => {
          try { return new URL(a.href).pathname === '/login'; } catch { return false; }
        })
        .find((a) => !a.closest('.connect-panel-menu, .connect-panel-menu-list, .connect-nav-drawer, [role="dialog"]') && textOf(a))
      : null;

    // 1. header collision -----------------------------------------------------
    const headerParts = [
      ['logo', logoLink],
      ['language select', selectTrigger],
      ['burger', burger],
    ].filter(([, el]) => el && rendered(el)).map(([name, el]) => ({ name, box: rectOf(el) }));

    const overlaps = [];
    for (let i = 0; i < headerParts.length; i += 1) {
      for (let j = i + 1; j < headerParts.length; j += 1) {
        const a = headerParts[i];
        const b = headerParts[j];
        const w = Math.min(a.box.right, b.box.right) - Math.max(a.box.x, b.box.x);
        const h = Math.min(a.box.bottom, b.box.bottom) - Math.max(a.box.y, b.box.y);
        if (w > 0.5 && h > 0.5) overlaps.push({ pair: `${a.name} / ${b.name}`, width: Math.round(w), height: Math.round(h) });
      }
    }
    // The painted lockup can overflow the link that owns it; the visible
    // collision is between the pixels, so compare the painted box as well.
    const paintedOverlaps = [];
    if (brandLogo && rendered(brandLogo)) {
      const box = rectOf(brandLogo);
      for (const part of headerParts) {
        if (part.name === 'logo') continue;
        const w = Math.min(box.right, part.box.right) - Math.max(box.x, part.box.x);
        const h = Math.min(box.bottom, part.box.bottom) - Math.max(box.y, part.box.y);
        if (w > 0.5 && h > 0.5) paintedOverlaps.push({ pair: `painted logo / ${part.name}`, width: Math.round(w), height: Math.round(h) });
      }
    }

    const headerBox = rectOf(header);
    const headerOffViewport = headerBox && (headerBox.right > viewportWidth + 1 || headerBox.x < -1)
      ? { selector: label(header), box: headerBox }
      : null;

    // 2. hero dead space ------------------------------------------------------
    const hero = document.querySelector('#hero, section.connect-hero, .connect-hero, .sprint-hero');
    const heroActions = hero
      ? hero.querySelector('.sprint-hero-actions, [class*="hero-actions"], [class*="actions"]')
        || hero.querySelector('.connect-btn-primary, .connect-btn-ghost')?.parentElement
      : null;
    const heroBox = rectOf(hero);
    const actionsBox = rectOf(heroActions);

    // 3. tap targets ----------------------------------------------------------
    const tapTargets = [];
    const tapExempt = [];
    for (const el of H.interactives(document.body)) {
      const box = el.getBoundingClientRect();
      const tooSmall = box.width < 44 - 0.5 || box.height < 44 - 0.5;
      if (!tooSmall) continue;
      const entry = {
        selector: label(el),
        text: textOf(el, 40),
        tag: el.tagName.toLowerCase(),
        width: Math.round(box.width),
        height: Math.round(box.height),
        section: section(el),
      };
      if (H.isInlineTextLink(el)) tapExempt.push(entry);
      else tapTargets.push(entry);
    }

    // 4. tiny type ------------------------------------------------------------
    const tinyType = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const seenType = new Set();
    let textNode;
    while ((textNode = walker.nextNode())) {
      if (!textNode.textContent.trim()) continue;
      const el = textNode.parentElement;
      if (!el || el.closest('svg, style, script, noscript, template') || !rendered(el)) continue;
      if (seenType.has(el)) continue;
      if (el.closest('.sr-only, [class*="sr-only"]')) continue;
      const size = Math.round(parseFloat(getComputedStyle(el).fontSize) * 100) / 100;
      if (size >= 11 - 0.01) continue;
      seenType.add(el);
      tinyType.push({
        text: textNode.textContent.replace(/\s+/g, ' ').trim().slice(0, 48),
        size,
        selector: label(el),
        section: section(el),
        ariaHidden: Boolean(el.closest('[aria-hidden="true"]')),
      });
      if (tinyType.length >= 300) break;
    }

    // 5. horizontal overflow --------------------------------------------------
    const scrollWidth = document.documentElement.scrollWidth;
    const overflowOffenders = [];
    for (const el of document.body.querySelectorAll('*')) {
      if (el.closest('svg, .ticker-band, .ticker-track')) continue;
      if (!rendered(el)) continue;
      const box = el.getBoundingClientRect();
      if (box.right <= viewportWidth + 1 && box.left >= -1) continue;
      if (hasClippingAncestor(el)) continue;
      overflowOffenders.push({ selector: label(el), text: textOf(el, 32), box: rectOf(el) });
      if (overflowOffenders.length >= 20) break;
    }

    // 6. clipped text ---------------------------------------------------------
    const clippedText = [];
    for (const el of document.body.querySelectorAll('*')) {
      if (el.closest('svg, .ticker-band, .ticker-track, .connect-panel-url')) continue;
      const ownTextNodes = [...el.childNodes].filter((n) => n.nodeType === 3 && n.textContent.trim());
      if (ownTextNodes.length === 0) continue;
      if (!rendered(el)) continue;
      const style = getComputedStyle(el);
      if (clipsAt(el)) {
        if (el.scrollWidth > el.clientWidth + 1 && el.clientWidth > 0) {
          clippedText.push({
            selector: label(el),
            text: ownTextNodes.map((n) => n.textContent.trim()).join(' ').replace(/\s+/g, ' ').slice(0, 48),
            scrollWidth: el.scrollWidth,
            clientWidth: el.clientWidth,
            reason: 'own overflow',
            section: section(el),
          });
        }
        continue;
      }
      // A clipping ancestor can cut the text even when the element itself does
      // not declare overflow. Compare the text runs to that ancestor's box.
      let ancestor = el.parentElement;
      let clipBox = null;
      while (ancestor && ancestor !== document.documentElement) {
        if (clipsAt(ancestor)) {
          const box = ancestor.getBoundingClientRect();
          if (box.width > 1 && box.height > 1) clipBox = { ancestor, box };
          break;
        }
        ancestor = ancestor.parentElement;
      }
      if (!clipBox) continue;
      let cut = false;
      for (const node of ownTextNodes) {
        const range = document.createRange();
        range.selectNodeContents(node);
        for (const rect of range.getClientRects()) {
          if (rect.right > clipBox.box.right + 1 || rect.left < clipBox.box.left - 1) {
            cut = true;
            break;
          }
        }
        if (cut) break;
      }
      if (cut) {
        clippedText.push({
          selector: label(el),
          text: ownTextNodes.map((n) => n.textContent.trim()).join(' ').replace(/\s+/g, ' ').slice(0, 48),
          reason: `clipped by ${label(clipBox.ancestor)}`,
          section: section(el),
        });
      }
    }

    return {
      viewport: { innerWidth: window.innerWidth, clientWidth: viewportWidth, scrollWidth, innerHeight: window.innerHeight },
      lang: document.documentElement.lang,
      title: document.title,
      header: {
        found: Boolean(header),
        box: headerBox,
        logoLink: logoLink ? { box: rectOf(logoLink), visible: rendered(logoLink), text: textOf(logoLink, 24) } : null,
        brandLogo: brandLogo ? { box: rectOf(brandLogo), visible: rendered(brandLogo) } : null,
        select: selectTrigger ? { box: rectOf(selectTrigger), visible: rendered(selectTrigger), text: textOf(selectTrigger, 24) } : null,
        burger: burger ? { box: rectOf(burger), visible: rendered(burger), controls: burger.getAttribute('aria-controls'), label: burger.getAttribute('aria-label') } : null,
        headerSignInText: textOf(headerSignIn, 32),
        nav: desktopNav ? { box: rectOf(desktopNav), visible: rendered(desktopNav), linkCount: navLinkCount } : null,
        overlaps,
        paintedOverlaps,
        offViewport: headerOffViewport,
        overflowingChildren: headerOverflow,
      },
      hero: {
        found: Boolean(hero),
        box: heroBox,
        actions: actionsBox,
        gap: heroBox && actionsBox ? Math.round((heroBox.bottom - actionsBox.bottom) * 10) / 10 : null,
      },
      overflow: { scrollWidth, clientWidth: viewportWidth, delta: scrollWidth - viewportWidth, offenders: overflowOffenders },
      tapTargets,
      tapExempt,
      tinyType,
      clippedText,
    };
  }, { helpersSource: String(PAGE_HELPERS) });
}

/** Drawer-open measurement. */
async function measureDrawer(page, drawerId, headerSignInText) {
  return page.evaluate(({ helpersSource, drawerId: id, signInText }) => {
    // eslint-disable-next-line no-new-func
    const H = new Function(`return (${helpersSource})()`)();
    const { rectOf, rendered, label, textOf, alphaOf, effectiveAlpha } = H;
    const drawer = id ? document.getElementById(id) : document.querySelector('[role="dialog"], .connect-nav-drawer, [class*="drawer"]');
    if (!drawer) return { found: false };

    const style = getComputedStyle(drawer);
    const alpha = effectiveAlpha(drawer, alphaOf(style.backgroundColor));

    const rows = [...drawer.querySelectorAll('a[href], button, [role="button"], select')]
      .filter((el) => rendered(el))
      .map((el) => {
        const box = el.getBoundingClientRect();
        return {
          selector: label(el),
          text: textOf(el, 40),
          height: Math.round(box.height),
          heightRaw: Math.round(box.height * 100) / 100,
          width: Math.round(box.width),
        };
      });

    const loginLinks = [...drawer.querySelectorAll('a[href]')].filter((a) => {
      try { return new URL(a.href).pathname === '/login'; } catch { return false; }
    });
    const signIn = loginLinks.find((a) => textOf(a) === signInText)
      || loginLinks.find((a) => !a.closest('[class*="panel"]'))
      || loginLinks[0]
      || null;

    const signInBox = signIn ? signIn.getBoundingClientRect() : null;
    return {
      found: true,
      box: rectOf(drawer),
      background: style.backgroundColor,
      alpha,
      opacity: Number(style.opacity),
      role: drawer.getAttribute('role'),
      ariaModal: drawer.getAttribute('aria-modal'),
      rows,
      signIn: signIn
        ? {
          found: true,
          text: textOf(signIn, 40),
          box: rectOf(signIn),
          visible: rendered(signIn) && signInBox.width > 0 && signInBox.height > 0,
          display: getComputedStyle(signIn).display,
        }
        : { found: false, text: signInText },
      bodyOverflow: document.body.style.overflow,
    };
  }, { helpersSource: String(PAGE_HELPERS), drawerId, signInText: headerSignInText || '' });
}

/* ------------------------------------------------------------------------- *
 * Checks. Each returns { status, details } where status is pass | fail | skip.
 * ------------------------------------------------------------------------- */

const pass = () => ({ status: 'pass', details: [] });
const fail = (details) => ({ status: 'fail', details: Array.isArray(details) ? details : [details] });
const skip = (reason) => ({ status: 'skip', details: [reason] });

function checkHeader(m) {
  const h = m.header;
  if (!h.found) return fail('no <header> found');
  const details = [];

  if (h.logoLink && h.brandLogo && h.logoLink.visible && h.brandLogo.visible) {
    const short = h.brandLogo.box.width - h.logoLink.box.width;
    if (short > 0.5) {
      details.push(
        `logo link is ${h.logoLink.box.width}x${h.logoLink.box.height} but paints ${h.brandLogo.box.width}x${h.brandLogo.box.height} (${Math.round(short)}px narrower — the flex item squeezed)`,
      );
    }
  }

  for (const overlap of h.overlaps) details.push(`${overlap.pair} overlap ${overlap.width}x${overlap.height}px`);
  for (const overlap of h.paintedOverlaps) details.push(`${overlap.pair} overlap ${overlap.width}x${overlap.height}px (painted logo escapes its link)`);

  const limit = m.viewport.clientWidth;
  if (h.offViewport) details.push(`header itself leaves the viewport (${JSON.stringify(h.offViewport.box)})`);
  for (const child of h.overflowingChildren) {
    details.push(`${child.selector} "${child.text}" reaches x=${Math.round(child.box.right)} (viewport ${limit})`);
  }

  return details.length ? fail(details) : pass();
}

function checkDrawer(m, width) {
  const d = m.drawer;
  if (!d) return skip('no burger at this width');
  if (!d.found) return fail('the burger did not open a drawer');
  const details = [];

  if (d.alpha < MIN_DRAWER_ALPHA) {
    details.push(`background ${d.background} is translucent (effective alpha ${d.alpha} < ${MIN_DRAWER_ALPHA})`);
  }
  if (!d.signIn.found) {
    details.push(`no "Sign in" link in the drawer (searched /login, label "${d.signIn.text || '?'}")`);
  } else if (!d.signIn.visible) {
    details.push(`"Sign in" link "${d.signIn.text}" is not visible (display:${d.signIn.display}, box ${d.signIn.box.width}x${d.signIn.box.height})`);
  }
  const shortRows = d.rows.filter((row) => (row.heightRaw ?? row.height) < MIN_TARGET - 0.5);
  for (const row of shortRows.slice(0, 10)) {
    details.push(`row "${row.text}" is ${row.width}x${row.height} (< 44px tall)`);
  }
  if (shortRows.length > 10) details.push(`… and ${shortRows.length - 10} more short rows`);

  return details.length ? fail(details) : pass();
}

function checkTargets(m) {
  const list = m.tapTargets;
  if (!list.length) return pass();
  // Group by area: a phone fix has to triage a header control differently from
  // a footer link column, and 30 flat lines hide that difference.
  const groups = new Map();
  for (const target of list) {
    if (!groups.has(target.section)) groups.set(target.section, []);
    groups.get(target.section).push(target);
  }
  const details = [`${list.length} offenders across ${groups.size} area${groups.size === 1 ? '' : 's'}`];
  for (const [area, items] of groups) {
    const samples = items.slice(0, 2)
      .map((t) => `${t.tag} ${t.width}x${t.height} "${t.text}" [${t.selector}]`)
      .join('; ');
    details.push(`${area} ${items.length}: ${samples}${items.length > 2 ? `; … +${items.length - 2}` : ''}`);
  }
  if (m.tapExempt.length) details.push(`(${m.tapExempt.length} inline prose links exempt per WCAG 2.5.8)`);
  return fail(details);
}

function checkType(m) {
  const list = m.tinyType;
  if (!list.length) return pass();
  const bySize = new Map();
  for (const item of list) bySize.set(item.size, (bySize.get(item.size) ?? 0) + 1);
  const histogram = [...bySize.entries()].sort((a, b) => a[0] - b[0]).map(([size, count]) => `${size}px × ${count}`).join(', ');
  const details = [`${list.length} text nodes below ${MIN_FONT}px: ${histogram}`];
  for (const item of list.slice(0, 8)) details.push(`${item.size}px "${item.text}" [${item.selector}]`);
  if (list.length > 8) details.push(`… and ${list.length - 8} more (see --json)`);
  return fail(details);
}

function checkOverflow(m) {
  const o = m.overflow;
  if (o.delta <= 0) return pass();
  const details = [`page scrollWidth ${o.scrollWidth} > viewport ${m.viewport.clientWidth} (+${o.delta}px)`];
  for (const offender of o.offenders.slice(0, 6)) details.push(`${offender.selector} "${offender.text}" right=${Math.round(offender.box.right)}`);
  return fail(details);
}

function checkHero(m) {
  const hero = m.hero;
  if (!hero.found) return skip('no hero section on this page');
  if (!hero.actions) return fail('hero actions row not found');
  if (hero.gap === null) return fail('hero actions row has no box');
  if (hero.gap < 0) return fail(`hero actions row overflows the hero bottom by ${Math.abs(hero.gap)}px`);
  if (hero.gap >= MAX_HERO_GAP) return fail(`hero bottom is ${hero.gap}px below the actions row (max ${MAX_HERO_GAP}px)`);
  return pass();
}

function checkClip(m) {
  const list = m.clippedText;
  if (!list.length) return pass();
  const details = list.slice(0, 8).map((c) => `"${c.text}" [${c.selector}] ${c.reason}`);
  if (list.length > 8) details.push(`… and ${list.length - 8} more (see --json)`);
  return fail(details);
}

function checkNav(m, width) {
  const h = m.header;
  if (!h.found) return fail('no <header> found');
  const navVisible = Boolean(h.nav?.visible);
  const burgerVisible = Boolean(h.burger?.visible);
  if (width >= DESKTOP_NAV_WIDTH) {
    if (!navVisible) return fail('desktop navigation is not visible at this width');
    if (burgerVisible) return fail('burger is still visible beside the desktop nav');
    return pass();
  }
  if (!burgerVisible) return fail('no burger below the desktop breakpoint');
  if (navVisible) {
    return fail(`inline nav (${h.nav.linkCount} links) is visible beside the burger below ${DESKTOP_NAV_WIDTH}px`);
  }
  return pass();
}

function runChecks(closed, drawer, width) {
  return {
    header: checkHeader(closed),
    drawer: checkDrawer({ ...closed, drawer }, width),
    targets: width <= PHONE_LIMIT ? checkTargets(closed) : skip('phone rule (<= 640px)'),
    type: width <= PHONE_LIMIT ? checkType(closed) : skip('phone rule (<= 640px)'),
    overflow: checkOverflow(closed),
    hero: width <= PHONE_LIMIT ? checkHero(closed) : skip('phone rule (<= 640px)'),
    clip: checkClip(closed),
    nav: checkNav(closed, width),
  };
}

/* ------------------------------------------------------------------------- *
 * Report
 * ------------------------------------------------------------------------- */

const STATUS_GLYPH = { pass: 'PASS', fail: 'FAIL', skip: '–' };

function cell(check) {
  if (!check) return STATUS_GLYPH.skip;
  if (check.status === 'skip') return STATUS_GLYPH.skip;
  return check.status === 'pass' ? 'PASS' : 'FAIL';
}

function cellWithCount(check, counts) {
  if (!check || check.status === 'skip') return STATUS_GLYPH.skip;
  if (check.status === 'pass') return 'PASS';
  return counts?.length ? `FAIL(${counts.length})` : 'FAIL';
}

function printReport(runs) {
  console.log('DROP IT /connect — mobile layout audit');
  console.log(`base    ${PAGE_URL}`);
  console.log(`matrix  ${WIDTHS.join(' ')} @ ${HEIGHT}px   locales ${LOCALES.join(' ')}   states page + drawer`);
  console.log('legend  PASS / FAIL(n) = n offenders / – = check does not apply at that width; 1024 = regression width (still burger territory)');
  console.log('');

  const rows = runs.map((run) => {
    const counts = {
      tap: run.measurements.tapTargets ?? [],
      type: run.measurements.tinyType ?? [],
      overflow: run.measurements.overflow?.offenders ?? [],
      clip: run.measurements.clippedText ?? [],
    };
    const heroGap = run.measurements.hero?.gap;
    const heroCell = run.checks.hero.status === 'fail' && typeof heroGap === 'number'
      ? `FAIL(${Math.round(heroGap)})`
      : cell(run.checks.hero);
    return [
      run.width,
      run.locale,
      cell(run.checks.header),
      cell(run.checks.drawer),
      cellWithCount(run.checks.targets, counts.tap),
      cellWithCount(run.checks.type, counts.type),
      cell(run.checks.overflow),
      heroCell,
      cell(run.checks.clip),
      cell(run.checks.nav),
    ].map(String);
  });

  const headerRow = ['WIDTH', 'LOCALE', 'HEADER', 'DRAWER', 'TAP', 'TYPE', 'OVERFLOW', 'HERO', 'CLIP', 'NAV'];
  const widths = headerRow.map((label, index) => Math.max(label.length, ...rows.map((row) => row[index].length)) + 1);
  const line = (row) => row.map((value, index) => value.padEnd(widths[index])).join(' ').trimEnd();
  console.log(line(headerRow));
  for (const row of rows) console.log(line(row));

  // The 768/1024 rows show `–` for the phone rules. Those widths are still
  // measured, and a landing is not healthy at 768 if the type or the targets
  // are the same size as at 320, so surface the numbers without gating them.
  const nonPhone = runs.filter((run) => run.width > PHONE_LIMIT);
  if (nonPhone.length) {
    console.log('');
    console.log('NOT GATED  (phone rules stop at 640px; measured only)');
    for (const width of [...new Set(nonPhone.map((run) => run.width))]) {
      const atWidth = nonPhone.filter((run) => run.width === width);
      const tap = Math.min(...atWidth.map((run) => (run.measurements.tapTargets ?? []).length));
      const type = Math.min(...atWidth.map((run) => (run.measurements.tinyType ?? []).length));
      const gaps = atWidth.map((run) => run.measurements.hero?.gap).filter((gap) => typeof gap === 'number');
      const gapText = gaps.length ? `${Math.round(Math.min(...gaps))}-${Math.round(Math.max(...gaps))}px` : 'n/a';
      console.log(`  ${width}px  tap targets < 44px: ${tap} · text < 11px: ${type} · hero gap: ${gapText}`);
    }
  }

  if (QUIET) return;

  console.log('');
  console.log('FAILURES');
  for (const run of runs) {
    const failing = Object.entries(run.checks).filter(([, check]) => check.status === 'fail');
    if (!failing.length) continue;
    console.log(`${run.width}px ${run.locale}`);
    for (const [id, check] of failing) {
      const [first, ...rest] = check.details;
      const indent = `  ${id.padEnd(9)}`;
      console.log(`${indent}${first}`);
      for (const text of rest) console.log(`${' '.repeat(indent.length)}${text}`);
    }
  }
}

function printSummary(runs, totals) {
  const failedRuns = runs.filter((run) => Object.values(run.checks).some((check) => check.status === 'fail'));
  const failsByCheck = {};
  for (const run of runs) {
    for (const [id, check] of Object.entries(run.checks)) {
      if (check.status === 'fail') failsByCheck[id] = (failsByCheck[id] ?? 0) + 1;
    }
  }
  const failing = CHECKS.filter((check) => failsByCheck[check.id]);
  console.log('');
  console.log(`SUMMARY  ${runs.length} runs · ${failedRuns.length} with failures · ${failing.length ? failing.map((c) => `${c.id} ${failsByCheck[c.id]}`).join(', ') : 'all checks pass'}`);
  if (failing.length) {
    console.log('Failed checks:');
    for (const check of failing) console.log(`  ${check.id.padEnd(9)} ${failsByCheck[check.id]} ×  ${check.title}`);
  }
  console.log(`Result: ${totals.failures ? 'FAIL' : 'PASS'} (${totals.failures} failed check${totals.failures === 1 ? '' : 's'} across ${runs.length} runs)`);
}

/* ------------------------------------------------------------------------- *
 * Main
 * ------------------------------------------------------------------------- */

async function main() {
  const startedAt = new Date().toISOString();
  let browser;
  try {
    browser = await chromium.launch({ headless: true });
  } catch (error) {
    console.error(`could not launch chromium: ${error.message}`);
    process.exitCode = 2;
    return;
  }

  const runs = [];
  const consoleErrors = [];

  try {
    for (const locale of LOCALES) {
      for (const width of WIDTHS) {
        const context = await browser.newContext({
          viewport: { width, height: HEIGHT },
          deviceScaleFactor: 1,
          isMobile: true,
          hasTouch: true,
          locale,
          extraHTTPHeaders: { 'Accept-Language': locale },
        });
        const page = await context.newPage();
        page.on('pageerror', (error) => consoleErrors.push({ width, locale, type: 'pageerror', text: String(error).slice(0, 240) }));

        const run = {
          width,
          locale,
          url: PAGE_URL,
          phone: width <= PHONE_LIMIT,
          checks: {},
          measurements: {},
        };

        try {
          await page.goto(PAGE_URL, { waitUntil: 'load', timeout: 60000 });
          await page.waitForSelector('.connect-hero', { timeout: 20000 });
          await page.evaluate(() => document.fonts?.ready);
          await page.waitForTimeout(SETTLE_MS);

          const closed = await measurePage(page, width);
          run.measurements = { ...closed, configuredWidth: width };

          // Open the drawer (the burger exists below 1280px).
          let drawer = null;
          if (closed.header.burger?.visible) {
            try {
              await page.locator('header button[aria-controls]').first().click({ timeout: 5000 });
              await page.waitForFunction((id) => Boolean(id && document.getElementById(id)), closed.header.burger.controls, { timeout: 5000 });
              await page.waitForTimeout(SETTLE_MS);
              drawer = await measureDrawer(page, closed.header.burger.controls, closed.header.headerSignInText);
            } catch (error) {
              drawer = { found: false, error: String(error).split('\n')[0] };
            }
          }
          run.measurements.drawer = drawer;
          run.checks = runChecks(closed, drawer, width);

          const failed = Object.values(run.checks).filter((check) => check.status === 'fail').length;
          if (!QUIET) console.error(`measured ${width}px ${locale} — ${failed ? `${failed} failing` : 'ok'}`);
        } catch (error) {
          const message = String(error).split('\n')[0];
          run.checks = Object.fromEntries(CHECKS.map((check) => [check.id, fail(`page did not load: ${message}`)]));
          run.measurements = { loadError: message };
          if (!QUIET) console.error(`failed to measure ${width}px ${locale}: ${message}`);
        }

        runs.push(run);
        await context.close();
      }
    }
  } finally {
    await browser.close();
  }

  printReport(runs);
  const totals = {
    failures: runs.reduce((sum, run) => sum + Object.values(run.checks).filter((check) => check.status === 'fail').length, 0),
    checks: runs.length * CHECKS.length,
  };
  printSummary(runs, totals);

  if (JSON_PATH) {
    const file = resolve(REPO, JSON_PATH);
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, `${JSON.stringify({
      generatedAt: startedAt,
      base: BASE,
      url: PAGE_URL,
      height: HEIGHT,
      widths: WIDTHS,
      phoneWidths: PHONE_WIDTHS,
      locales: LOCALES,
      rules: { minTargetPx: MIN_TARGET, minFontPx: MIN_FONT, maxHeroGapPx: MAX_HERO_GAP, minDrawerAlpha: MIN_DRAWER_ALPHA, desktopNavWidth: DESKTOP_NAV_WIDTH, phoneLimitPx: PHONE_LIMIT },
      checks: CHECKS,
      runs,
      consoleErrors,
      summary: {
        runs: runs.length,
        failedChecks: totals.failures,
        failedRuns: runs.filter((run) => Object.values(run.checks).some((check) => check.status === 'fail')).length,
        byCheck: Object.fromEntries(CHECKS.map((check) => [check.id, runs.filter((run) => run.checks[check.id]?.status === 'fail').length])),
      },
    }, null, 2)}\n`);
    if (!QUIET) console.error(`json     ${file}`);
  }

  process.exitCode = totals.failures ? 1 : 0;
}

await main();
