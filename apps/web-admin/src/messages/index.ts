/**
 * Admin catalog registry — one file per area, merged with the shared shell
 * catalog by `../locale.tsx`.
 *
 * A module exports `Record<Language, Record<string, string>>` with all six
 * languages. Duplicate keys across modules are reported instead of letting the
 * last area silently win.
 *
 * The shared component catalogs (`table`, `console`, `analytics`, `shared`,
 * `status`, `nav`) sit next to the page catalogs because a component used by
 * four screens owns its copy once, not four times.
 */
import type { Language, MessageCatalog } from '../i18n';
import { navCatalog } from './nav';
import { statusCatalog } from './status';
import { sharedCatalog } from './shared';
import { overviewCatalog } from './overview';
import { tableCatalog } from './table';
import { consoleCatalog } from './console';
import { analyticsCatalog } from './analytics';
import { managementCatalog } from './management';
import { accessCatalog } from './access';
import { activityCatalog } from './activity';
import { settingsCatalog } from './settings';
import { payoutsCatalog } from './payouts';
import { incidentsCatalog } from './incidents';
import { guestsCatalog } from './guests';
import { paymentsCatalog } from './payments';
import { clubsCatalog } from './clubs';
import { djsCatalog } from './djs';
import { auditLogCatalog } from './audit-log';
import { platformSettingsCatalog } from './platform-settings';
import { dashboardCatalog } from './dashboard';
import { loginCatalog } from './login';
import { mailCatalog } from './mail';
import { onboardingCatalog } from './onboarding';
import { apiKeysCatalog } from './api-keys';
import { deploymentCatalog } from './deployment';

export interface CatalogProblem {
  kind: 'duplicate-key';
  area: string;
  key: string;
}

const AREAS: ReadonlyArray<{ name: string; catalog: MessageCatalog }> = [
  // Shared vocabulary first: navigation, status words, component copy.
  { name: 'nav', catalog: navCatalog },
  { name: 'status', catalog: statusCatalog },
  { name: 'shared', catalog: sharedCatalog },
  { name: 'deployment', catalog: deploymentCatalog },
  { name: 'table', catalog: tableCatalog },
  { name: 'console', catalog: consoleCatalog },
  { name: 'analytics', catalog: analyticsCatalog },
  // One module per screen area.
  { name: 'overview', catalog: overviewCatalog },
  { name: 'management', catalog: managementCatalog },
  { name: 'activity', catalog: activityCatalog },
  { name: 'access', catalog: accessCatalog },
  { name: 'settings', catalog: settingsCatalog },
  { name: 'payouts', catalog: payoutsCatalog },
  { name: 'incidents', catalog: incidentsCatalog },
  { name: 'guests', catalog: guestsCatalog },
  { name: 'payments', catalog: paymentsCatalog },
  { name: 'clubs', catalog: clubsCatalog },
  { name: 'djs', catalog: djsCatalog },
  { name: 'auditLog', catalog: auditLogCatalog },
  { name: 'platformSettings', catalog: platformSettingsCatalog },
  { name: 'dashboard', catalog: dashboardCatalog },
  { name: 'login', catalog: loginCatalog },
  { name: 'mail', catalog: mailCatalog },
  // Club onboarding: the application queue and its invite link.
  { name: 'onboarding', catalog: onboardingCatalog },
  // API-key oversight: every club's scoped keys, read-only plus revocation.
  { name: 'apiKeys', catalog: apiKeysCatalog },
];

export function buildCatalog(): { catalog: MessageCatalog; problems: CatalogProblem[] } {
  const problems: CatalogProblem[] = [];
  const catalog = {} as MessageCatalog;
  for (const area of AREAS) {
    for (const [language, messages] of Object.entries(area.catalog) as Array<[Language, Record<string, string>]>) {
      const target = (catalog[language] ??= {});
      for (const [key, value] of Object.entries(messages)) {
        if (key in target) {
          problems.push({ kind: 'duplicate-key', area: area.name, key });
          continue;
        }
        target[key] = value;
      }
    }
  }
  return { catalog, problems };
}

export { AREAS };