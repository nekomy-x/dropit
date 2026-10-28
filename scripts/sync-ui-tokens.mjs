#!/usr/bin/env node
/**
 * Keep the two panel apps on one UI kit.
 *
 *   node scripts/sync-ui-tokens.mjs           # copy canonical -> apps
 *   node scripts/sync-ui-tokens.mjs --check   # fail if a copy drifted
 *
 * Canonical sources live in packages/ui/. Each app gets a byte-identical copy
 * because both are separate Vite builds with their own tsconfig and CSS
 * pipeline; the copies are never edited by hand.
 *
 * Run after touching packages/ui/*, then build both apps.
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');

/** canonical -> app copy */
const FILES = [
  ['packages/ui/tokens.css', 'apps/web-club/src/styles/club-tokens.css'],
  ['packages/ui/tokens.css', 'apps/web-admin/src/styles/panel-tokens.css'],
  ['packages/ui/panel.css', 'apps/web-club/src/styles/panel.css'],
  ['packages/ui/panel.css', 'apps/web-admin/src/styles/panel.css'],
  ['packages/ui/Icon.tsx', 'apps/web-club/src/components/Icon.tsx'],
  ['packages/ui/Icon.tsx', 'apps/web-admin/src/components/Icon.tsx'],
  ['packages/ui/theme.tsx', 'apps/web-club/src/theme.tsx'],
  ['packages/ui/theme.tsx', 'apps/web-admin/src/theme.tsx'],
  ['packages/ui/charts.tsx', 'apps/web-club/src/components/ui/charts.tsx'],
  ['packages/ui/charts.tsx', 'apps/web-admin/src/components/ui/charts.tsx'],
  ['packages/ui/StatCard.tsx', 'apps/web-club/src/components/ui/StatCard.tsx'],
  ['packages/ui/StatCard.tsx', 'apps/web-admin/src/components/ui/StatCard.tsx'],
  ['packages/ui/Select.tsx', 'apps/web-club/src/components/ui/Select.tsx'],
  ['packages/ui/Select.tsx', 'apps/web-admin/src/components/ui/Select.tsx'],
  ['packages/ui/TrashAction.tsx', 'apps/web-club/src/components/ui/TrashAction.tsx'],
  ['packages/ui/TrashAction.tsx', 'apps/web-admin/src/components/ui/TrashAction.tsx'],
  ['packages/ui/i18n.tsx', 'apps/web-club/src/i18n.tsx'],
  ['packages/ui/i18n.tsx', 'apps/web-admin/src/i18n.tsx'],
  ['packages/ui/menu.tsx', 'apps/web-club/src/menu.tsx'],
  ['packages/ui/menu.tsx', 'apps/web-admin/src/menu.tsx'],
  ['packages/ui/nav-drawer.tsx', 'apps/web-club/src/nav-drawer.tsx'],
  ['packages/ui/nav-drawer.tsx', 'apps/web-admin/src/nav-drawer.tsx'],
  ['packages/ui/modal.tsx', 'apps/web-club/src/modal.tsx'],
  ['packages/ui/modal.tsx', 'apps/web-admin/src/modal.tsx'],
  ['packages/ui/account.tsx', 'apps/web-club/src/account.tsx'],
  ['packages/ui/account.tsx', 'apps/web-admin/src/account.tsx'],
  ['packages/ui/messages.ts', 'apps/web-club/src/shell-messages.ts'],
  ['packages/ui/messages.ts', 'apps/web-admin/src/shell-messages.ts'],
  // Auth surfaces (two skins). Kept in its own file so a change to the
  // login/registration layout never touches panel.css.
  ['packages/ui/auth.css', 'apps/web-club/src/styles/auth.css'],
  ['packages/ui/auth.css', 'apps/web-admin/src/styles/auth.css'],
  // The "not production" pill: its own file so a rewrite of either the auth
  // layout or the status band cannot drop it.
  ['packages/ui/test-badge.css', 'apps/web-club/src/styles/test-badge.css'],
  ['packages/ui/test-badge.css', 'apps/web-admin/src/styles/test-badge.css'],
  // The deployment strip: one fixed band at the bottom edge (test / maintenance
  // / nothing) plus the offsets that keep the shell clear of it. It is the
  // product's only environment indicator — there is no pill, badge or inline
  // footnote carrying the same fact anywhere else.
  ['packages/ui/DeploymentStatus.tsx', 'apps/web-club/src/components/DeploymentStatus.tsx'],
  ['packages/ui/DeploymentStatus.tsx', 'apps/web-admin/src/components/DeploymentStatus.tsx'],
  ['packages/ui/status-bar.css', 'apps/web-club/src/styles/status-bar.css'],
  ['packages/ui/status-bar.css', 'apps/web-admin/src/styles/status-bar.css'],
  // Pointer selection guard: one listener that clears a stale text selection
  // when a control is clicked (see the file's header for the reported cases).
  ['packages/ui/pointer-guard.ts', 'apps/web-club/src/pointer-guard.ts'],
  ['packages/ui/pointer-guard.ts', 'apps/web-admin/src/pointer-guard.ts'],
];

const check = process.argv.includes('--check');
let failures = 0;

for (const [from, to] of FILES) {
  const source = join(REPO, from);
  const target = join(REPO, to);
  if (!existsSync(source)) {
    console.error(`missing canonical source: ${from}`);
    failures += 1;
    continue;
  }
  if (check) {
    if (!existsSync(target)) {
      console.error(`missing copy: ${to}`);
      failures += 1;
      continue;
    }
    if (!readFileSync(source).equals(readFileSync(target))) {
      console.error(`drifted: ${to} differs from ${from} — run node scripts/sync-ui-tokens.mjs`);
      failures += 1;
    } else {
      console.log(`ok  ${to}`);
    }
  } else {
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(source, target);
    console.log(`wrote ${to}`);
  }
}

if (failures > 0) process.exit(1);
