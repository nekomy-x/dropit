/**
 * Incidents catalog — `pages/IncidentsPage.tsx`, the live safety surface.
 *
 * The incident `kind` and `message` are API values and stay as returned; only
 * the frame around them is translated.
 */
import type { MessageCatalog } from '../i18n';

export const incidentsCatalog = {
  en: {
    "incidents.title": "Incidents",
    "incidents.resolveFailed": "Resolve failed",
    "incidents.loading": "Loading incidents…",
    "incidents.emptyTitle": "No open incidents",
    "incidents.emptyBody": "No unresolved payout or ledger issues were returned.",
    "incidents.openIncidents": "Open incidents",
    "incidents.openSubtitle": "One row per unresolved issue, oldest handling first.",
    "incidents.openCount": "{count} open",
    "incidents.resolve": "Resolve",
    "incidents.resolving": "Resolving…",
  },
  es: {
    "incidents.title": "Incidencias",
    "incidents.resolveFailed": "No se pudo resolver",
    "incidents.loading": "Cargando las incidencias…",
    "incidents.emptyTitle": "No hay incidencias abiertas",
    "incidents.emptyBody": "No se devolvieron incidencias de liquidación ni del libro mayor sin resolver.",
    "incidents.openIncidents": "Incidencias abiertas",
    "incidents.openSubtitle": "Una fila por incidencia sin resolver, las más antiguas primero.",
    "incidents.openCount": "{count} abiertas",
    "incidents.resolve": "Resolver",
    "incidents.resolving": "Resolviendo…",
  },
  fr: {
    "incidents.title": "Incidents",
    "incidents.resolveFailed": "La résolution a échoué",
    "incidents.loading": "Chargement des incidents…",
    "incidents.emptyTitle": "Aucun incident ouvert",
    "incidents.emptyBody": "Aucun problème de versement ou de grand livre non résolu n’a été renvoyé.",
    "incidents.openIncidents": "Incidents ouverts",
    "incidents.openSubtitle": "Une ligne par problème non résolu, les plus anciens d’abord.",
    "incidents.openCount": "{count} ouverts",
    "incidents.resolve": "Résoudre",
    "incidents.resolving": "Résolution…",
  },
  de: {
    "incidents.title": "Störungen",
    "incidents.resolveFailed": "Schließen fehlgeschlagen",
    "incidents.loading": "Störungen werden geladen…",
    "incidents.emptyTitle": "Keine offenen Störungen",
    "incidents.emptyBody": "Es wurden keine ungelösten Auszahlungs- oder Buchungsprobleme gemeldet.",
    "incidents.openIncidents": "Offene Störungen",
    "incidents.openSubtitle": "Eine Zeile pro ungelöstem Problem, älteste zuerst.",
    "incidents.openCount": "{count} offen",
    "incidents.resolve": "Schließen",
    "incidents.resolving": "Wird geschlossen…",
  },
  pt: {
    "incidents.title": "Incidentes",
    "incidents.resolveFailed": "Não foi possível resolver",
    "incidents.loading": "A carregar os incidentes…",
    "incidents.emptyTitle": "Nenhum incidente aberto",
    "incidents.emptyBody": "Não foram devolvidos problemas de pagamento ou do razão por resolver.",
    "incidents.openIncidents": "Incidentes abertos",
    "incidents.openSubtitle": "Uma linha por problema por resolver, os mais antigos primeiro.",
    "incidents.openCount": "{count} abertos",
    "incidents.resolve": "Resolver",
    "incidents.resolving": "A resolver…",
  },
  ru: {
    "incidents.title": "Инциденты",
    "incidents.resolveFailed": "Не удалось закрыть",
    "incidents.loading": "Загружаем инциденты…",
    "incidents.emptyTitle": "Открытых инцидентов нет",
    "incidents.emptyBody": "Незакрытых проблем с выплатами или реестром не найдено.",
    "incidents.openIncidents": "Открытые инциденты",
    "incidents.openSubtitle": "Одна строка на нерешённую проблему, сначала самые старые.",
    "incidents.openCount": "Открыто: {count}",
    "incidents.resolve": "Закрыть",
    "incidents.resolving": "Закрываем…",
  },
} satisfies MessageCatalog;
