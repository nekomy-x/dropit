/**
 * Shared admin component catalog — copy that belongs to a reusable shell piece
 * rather than to one screen: the loading and skeleton surfaces and the dialog
 * close control.
 *
 * Where the shell catalog already carries the sentence (`common.error`,
 * `common.tryAgain`, `common.loading`, `common.close`) it is reused instead of
 * being redeclared here.
 */
import type { MessageCatalog } from '../i18n';

export const sharedCatalog = {
  en: {
    'shared.loadingTable': 'Loading table',
    'shared.loadingStats': 'Loading stats',
    'shared.closeDialog': 'Close dialog',
  },
  es: {
    'shared.loadingTable': 'Cargando la tabla',
    'shared.loadingStats': 'Cargando las métricas',
    'shared.closeDialog': 'Cerrar el diálogo',
  },
  fr: {
    'shared.loadingTable': 'Chargement du tableau',
    'shared.loadingStats': 'Chargement des statistiques',
    'shared.closeDialog': 'Fermer la boîte de dialogue',
  },
  de: {
    'shared.loadingTable': 'Tabelle wird geladen',
    'shared.loadingStats': 'Kennzahlen werden geladen',
    'shared.closeDialog': 'Dialog schließen',
  },
  pt: {
    'shared.loadingTable': 'A carregar a tabela',
    'shared.loadingStats': 'A carregar as métricas',
    'shared.closeDialog': 'Fechar a caixa de diálogo',
  },
  ru: {
    'shared.loadingTable': 'Загружаем таблицу',
    'shared.loadingStats': 'Загружаем показатели',
    'shared.closeDialog': 'Закрыть диалог',
  },
} satisfies MessageCatalog;