/**
 * Panel catalog registry — one file per screen area.
 *
 * Every area module exports `Record<Language, Record<string, string>>` with the
 * complete set of six languages. Splitting by area keeps a translation change
 * inside one file instead of a 2000-line catalog, and keeps two people editing
 * different screens from touching the same module.
 *
 * Rules a module must follow (enforced by `node scripts/i18n-check.mjs`):
 *   1. every key in `en` exists in `es`, `fr`, `de`, `pt` and `ru`;
 *   2. keys are unique across all modules — a duplicate is a merge conflict;
 *   3. placeholders `{like_this}` survive translation unchanged;
 *   4. money is never baked into a string: pass a formatted value in.
 */
import type { Language, MessageCatalog } from '../i18n';
import { shellMessages } from '../shell-messages';
import { dashboardCatalog } from './dashboard';
import { sessionsCatalog } from './sessions';
import { djsCatalog } from './djs';
import { pricingCatalog } from './pricing';
import { earningsCatalog } from './earnings';
import { djConsoleCatalog } from './dj-console';
import { authCatalog } from './auth';
import { deploymentCatalog } from './deployment';
import { qrStudioCatalog } from './qr-studio';

const AREAS: ReadonlyArray<{ name: string; catalog: MessageCatalog }> = [
  // The shared shell first: navigation, account menu, common vocabulary.
  { name: 'shell', catalog: shellMessages },
  { name: 'dashboard', catalog: dashboardCatalog },
  { name: 'sessions', catalog: sessionsCatalog },
  { name: 'djs', catalog: djsCatalog },
  { name: 'pricing', catalog: pricingCatalog },
  { name: 'earnings', catalog: earningsCatalog },
  { name: 'dj-console', catalog: djConsoleCatalog },
  { name: 'auth', catalog: authCatalog },
  { name: 'qr-studio', catalog: qrStudioCatalog },
  // The fixed bottom strip: what this deployment is (test / maintenance).
  { name: 'deployment', catalog: deploymentCatalog },
];

export interface CatalogProblem {
  kind: 'duplicate-key' | 'missing-language' | 'unknown-language';
  area: string;
  key: string;
  detail: string;
}

/**
 * Merges the area catalogs and reports collisions instead of silently letting
 * the last area win — a duplicate key is almost always a copy-paste mistake
 * that would show the wrong sentence on screen.
 */
export function buildCatalog(): { catalog: MessageCatalog; problems: CatalogProblem[] } {
  const problems: CatalogProblem[] = [];
  const catalog = {} as MessageCatalog;
  for (const area of AREAS) {
    for (const [language, messages] of Object.entries(area.catalog) as Array<[Language, Record<string, string>]>) {
      const target = (catalog[language] ??= {});
      for (const [key, value] of Object.entries(messages)) {
        if (key in target) {
          problems.push({
            kind: 'duplicate-key',
            area: area.name,
            key,
            detail: `"${key}" is declared in ${area.name} and an earlier area`,
          });
          continue;
        }
        target[key] = value;
      }
    }
  }
  return { catalog, problems };
}

export { AREAS };
