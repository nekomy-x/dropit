/**
 * DJ console — the requests workspace.
 *
 * The DJ's processing surface has its own copy: the live strip, the catalog
 * preview controls and the "find this track" links. It lives in this module
 * rather than in `dj-console.ts` because that file already covers the page
 * head, room control and session details, and a translation change to one
 * screen should not have to open a file about another.
 *
 * Honesty rules (docs/ui/PANEL-DESIGN.md §Data honesty):
 *   - the strip says what is actually keeping the list current: the socket
 *     (`live`), the documented timer fallback (`timer`), or nothing
 *     (`offline`). It never claims live data while it is polling.
 *   - `dj.findInService` is a SEARCH, not a catalog match: the backend exposes
 *     `title`, `artist`, `isrc` and a 30-second `preview_url`, and no streaming
 *     identifiers, so every external link opens the service's search results.
 *   - `dj.previewUnavailable` is what an absent `preview_url` renders as. A
 *     disabled control with no explanation reads as a bug.
 */
import type { MessageCatalog } from '../i18n';

export const djRequestsCatalog: MessageCatalog = {
  en: {
    // --- Live strip -------------------------------------------------------
    'dj.liveStale': 'Connection lost — showing the last queue',
    'dj.liveTimer': 'Timer refresh',
    'dj.updatedSecondsAgo': 'updated {seconds} s ago',
    'dj.updatedJustNow': 'updated just now',
    'dj.updatedNever': 'not synced yet',
    'dj.waitingNow': 'Waiting',
    'dj.queuedNow': 'Queued',
    'dj.newBadge': 'New',
    // --- Catalog preview and music links ---------------------------------
    'dj.playPreview': 'Play preview',
    'dj.stopPreview': 'Stop preview',
    'dj.previewUnavailable': 'No preview in the catalog',
    'dj.previewStreamLabel': 'Catalog preview',
    'dj.findInService': 'Find {track} in {service}',
    'dj.musicSources': 'Open in',
    'dj.copyTrack': 'Copy track',
    'dj.trackCopied': 'Track copied',
    'dj.isrcCopy': 'Copy ISRC',
    'dj.isrcCopied': 'ISRC copied',
    // --- Keyboard flow ----------------------------------------------------
    'dj.keyboardHint': '↑↓ select · Enter accept · Backspace decline · P played · / search',
    'dj.rejectPrompt': 'Decline with a reason',
    'dj.rejectReasonStyle': 'Wrong style',
    'dj.rejectReasonAlreadyPlayed': 'Already played',
    'dj.rejectReasonLater': 'Later',
  },
  es: {
    // --- Live strip -------------------------------------------------------
    'dj.liveStale': 'Conexión perdida: se muestra la última cola',
    'dj.liveTimer': 'Actualización por temporizador',
    'dj.updatedSecondsAgo': 'actualizado hace {seconds} s',
    'dj.updatedJustNow': 'actualizado ahora mismo',
    'dj.updatedNever': 'aún sin sincronizar',
    'dj.waitingNow': 'En espera',
    'dj.queuedNow': 'En cola',
    'dj.newBadge': 'Nuevo',
    // --- Catalog preview and music links ---------------------------------
    'dj.playPreview': 'Reproducir vista previa',
    'dj.stopPreview': 'Detener vista previa',
    'dj.previewUnavailable': 'Sin vista previa en el catálogo',
    'dj.previewStreamLabel': 'Vista previa del catálogo',
    'dj.findInService': 'Buscar {track} en {service}',
    'dj.musicSources': 'Abrir en',
    'dj.copyTrack': 'Copiar tema',
    'dj.trackCopied': 'Tema copiado',
    'dj.isrcCopy': 'Copiar ISRC',
    'dj.isrcCopied': 'ISRC copiado',
    // --- Keyboard flow ----------------------------------------------------
    'dj.keyboardHint': '↑↓ seleccionar · Enter aceptar · Backspace rechazar · P emitido · / buscar',
    'dj.rejectPrompt': 'Rechazar con motivo',
    'dj.rejectReasonStyle': 'Estilo equivocado',
    'dj.rejectReasonAlreadyPlayed': 'Ya emitido',
    'dj.rejectReasonLater': 'Después',
  },
  fr: {
    // --- Live strip -------------------------------------------------------
    'dj.liveStale': 'Connexion perdue — dernière file affichée',
    'dj.liveTimer': 'Rafraîchissement par minuteur',
    'dj.updatedSecondsAgo': 'mis à jour il y a {seconds} s',
    'dj.updatedJustNow': 'mis à jour à l’instant',
    'dj.updatedNever': 'pas encore synchronisé',
    'dj.waitingNow': 'En attente',
    'dj.queuedNow': 'En file',
    'dj.newBadge': 'Nouveau',
    // --- Catalog preview and music links ---------------------------------
    'dj.playPreview': 'Lire l’aperçu',
    'dj.stopPreview': 'Arrêter l’aperçu',
    'dj.previewUnavailable': 'Aucun aperçu dans le catalogue',
    'dj.previewStreamLabel': 'Aperçu du catalogue',
    'dj.findInService': 'Rechercher {track} dans {service}',
    'dj.musicSources': 'Ouvrir dans',
    'dj.copyTrack': 'Copier le morceau',
    'dj.trackCopied': 'Morceau copié',
    'dj.isrcCopy': 'Copier l’ISRC',
    'dj.isrcCopied': 'ISRC copié',
    // --- Keyboard flow ----------------------------------------------------
    'dj.keyboardHint': '↑↓ sélectionner · Entrée accepter · Retour arrière refuser · P diffusé · / rechercher',
    'dj.rejectPrompt': 'Refuser avec un motif',
    'dj.rejectReasonStyle': 'Mauvais style',
    'dj.rejectReasonAlreadyPlayed': 'Déjà diffusé',
    'dj.rejectReasonLater': 'Plus tard',
  },
  de: {
    // --- Live strip -------------------------------------------------------
    'dj.liveStale': 'Verbindung verloren – letzte Queue wird angezeigt',
    'dj.liveTimer': 'Aktualisierung per Timer',
    'dj.updatedSecondsAgo': 'vor {seconds} s aktualisiert',
    'dj.updatedJustNow': 'gerade aktualisiert',
    'dj.updatedNever': 'noch nicht synchronisiert',
    'dj.waitingNow': 'Wartend',
    'dj.queuedNow': 'In der Queue',
    'dj.newBadge': 'Neu',
    // --- Catalog preview and music links ---------------------------------
    'dj.playPreview': 'Vorschau abspielen',
    'dj.stopPreview': 'Vorschau stoppen',
    'dj.previewUnavailable': 'Keine Vorschau im Katalog',
    'dj.previewStreamLabel': 'Katalog-Vorschau',
    'dj.findInService': '{track} in {service} suchen',
    'dj.musicSources': 'Öffnen in',
    'dj.copyTrack': 'Titel kopieren',
    'dj.trackCopied': 'Titel kopiert',
    'dj.isrcCopy': 'ISRC kopieren',
    'dj.isrcCopied': 'ISRC kopiert',
    // --- Keyboard flow ----------------------------------------------------
    'dj.keyboardHint': '↑↓ auswählen · Enter annehmen · Backspace ablehnen · P gespielt · / suchen',
    'dj.rejectPrompt': 'Mit Grund ablehnen',
    'dj.rejectReasonStyle': 'Falscher Stil',
    'dj.rejectReasonAlreadyPlayed': 'Schon gespielt',
    'dj.rejectReasonLater': 'Später',
  },
  pt: {
    // --- Live strip -------------------------------------------------------
    'dj.liveStale': 'Ligação perdida — a mostrar a última fila',
    'dj.liveTimer': 'Atualização por temporizador',
    'dj.updatedSecondsAgo': 'atualizado há {seconds} s',
    'dj.updatedJustNow': 'atualizado agora mesmo',
    'dj.updatedNever': 'ainda por sincronizar',
    'dj.waitingNow': 'Em espera',
    'dj.queuedNow': 'Em fila',
    'dj.newBadge': 'Novo',
    // --- Catalog preview and music links ---------------------------------
    'dj.playPreview': 'Reproduzir pré-visualização',
    'dj.stopPreview': 'Parar pré-visualização',
    'dj.previewUnavailable': 'Sem pré-visualização no catálogo',
    'dj.previewStreamLabel': 'Pré-visualização do catálogo',
    'dj.findInService': 'Procurar {track} em {service}',
    'dj.musicSources': 'Abrir em',
    'dj.copyTrack': 'Copiar tema',
    'dj.trackCopied': 'Tema copiado',
    'dj.isrcCopy': 'Copiar ISRC',
    'dj.isrcCopied': 'ISRC copiado',
    // --- Keyboard flow ----------------------------------------------------
    'dj.keyboardHint': '↑↓ selecionar · Enter aceitar · Backspace recusar · P tocado · / procurar',
    'dj.rejectPrompt': 'Recusar com motivo',
    'dj.rejectReasonStyle': 'Estilo errado',
    'dj.rejectReasonAlreadyPlayed': 'Já tocado',
    'dj.rejectReasonLater': 'Mais tarde',
  },
  ru: {
    // --- Live strip -------------------------------------------------------
    'dj.liveStale': 'Связь потеряна — показана последняя очередь',
    'dj.liveTimer': 'Обновление по таймеру',
    'dj.updatedSecondsAgo': 'обновлено {seconds} с назад',
    'dj.updatedJustNow': 'обновлено только что',
    'dj.updatedNever': 'ещё не синхронизировано',
    'dj.waitingNow': 'Ждут',
    'dj.queuedNow': 'В очереди',
    'dj.newBadge': 'Новое',
    // --- Catalog preview and music links ---------------------------------
    'dj.playPreview': 'Послушать превью',
    'dj.stopPreview': 'Остановить превью',
    'dj.previewUnavailable': 'Превью нет в каталоге',
    'dj.previewStreamLabel': 'Превью каталога',
    'dj.findInService': 'Найти {track} в {service}',
    'dj.musicSources': 'Открыть в',
    'dj.copyTrack': 'Скопировать трек',
    'dj.trackCopied': 'Трек скопирован',
    'dj.isrcCopy': 'Скопировать ISRC',
    'dj.isrcCopied': 'ISRC скопирован',
    // --- Keyboard flow ----------------------------------------------------
    'dj.keyboardHint': '↑↓ выбор · Enter принять · Backspace отклонить · P сыграно · / поиск',
    'dj.rejectPrompt': 'Отклонить с причиной',
    'dj.rejectReasonStyle': 'Не тот стиль',
    'dj.rejectReasonAlreadyPlayed': 'Уже играл',
    'dj.rejectReasonLater': 'Позже',
  },
};
