/**
 * DJ roster catalog — the club's DJ management screen (`pages/DjsPage.tsx`).
 *
 * The screen manages access, presence, shift and the club's own note for every
 * DJ. Copy contract:
 *   - a note is visible only to this club, never to the DJ or a guest;
 *   - an invitation link is single-use and its exact expiry is stated;
 *   - presence is a server fact with an age, so a stale value says how old it is
 *     instead of presenting itself as current.
 *
 * `djs.age*` strings are BARE quantities ("3 min"): the surrounding sentence
 * supplies the preposition, because "seen 3 min ago" / "visto hace 3 min" /
 * "vor 3 Minuten" do not share a word order. Each unit has a singular and a
 * plural key — English/Spanish/French/German/Portuguese inflect after the
 * number, so `formatAge` picks the form instead of appending an "s".
 * `djs.fail*` wrap a translated action phrase (`djs.action*`), which is an
 * infinitive in every language except German, where it is the "… zu …" clause
 * that fits "{action} war nicht möglich."
 */
import type { MessageCatalog } from '../i18n';

export const djsCatalog = {
  en: {
    // page head
    'djs.heading': 'DJs',
    'djs.showRemoved': 'Show removed',
    'djs.hideRemoved': 'Hide removed',

    // load and error states
    'djs.panelYourDjs': 'Your DJs',
    'djs.panelYourDjsDetail': 'Who can sign in to this club and be assigned to a shift.',
    'djs.errorTitle': 'The DJ list could not be loaded',
    'djs.errorTail': 'The roster is read from the club API — retry, and if it keeps failing the DJs cannot be managed from here.',
    'djs.stubTitle': 'DJ directory is waiting for the service',
    'djs.stubDetail': 'Invitation links are created when the club DJ endpoint is available.',
    'djs.loadingDjs': 'Loading DJs',

    // roster panel
    'djs.rosterTitle': 'Roster',
    'djs.rosterDetail': 'Presence comes from the DJ console; this list refreshes every 20 seconds.',
    'djs.onShiftCount': '{count} on shift',
    'djs.presenceSummary': '{online} online · {total} in roster',
    'djs.pollFailedStale': 'The last refresh failed, so presence below is from {age} ago.',
    'djs.pollFailedEarlier': 'The last refresh failed, so presence below is from an earlier request.',
    'djs.emptyTitle': 'No DJs in the roster',
    'djs.emptyDetail': 'Create an invitation below to give a DJ access to this club workspace.',
    'djs.tableCaption': 'DJ roster: access, presence, shift and the club’s note for each DJ.',
    'djs.colDj': 'DJ',
    'djs.colAccess': 'Access',
    'djs.colPresence': 'Presence',
    'djs.colShift': 'Shift',
    'djs.colNote': 'Note',
    'djs.colActions': 'Actions',

    // presence and shift cells
    'djs.presenceOnline': 'Online',
    'djs.presenceOffline': 'Offline',
    'djs.presenceUnknown': 'Unknown',
    'djs.presenceLive': 'live now',
    'djs.presenceSeen': 'seen {age} ago',
    'djs.presenceLastSeen': 'last seen {age} ago',
    'djs.presenceNever': 'never opened the console',
    'djs.inviteExpires': 'invite expires {date}',
    'djs.inviteNotAccepted': 'invitation not accepted yet',
    'djs.onShift': 'On shift',
    'djs.offShift': 'Off shift',
    'djs.openSession': 'open session',
    'djs.noNote': 'No note',

    // row actions
    'djs.editNote': 'Edit note',
    'djs.addNote': 'Add note',
    'djs.suspendAction': 'Suspend',
    'djs.restoreAccess': 'Restore access',
    'djs.remove': 'Remove',
    'djs.removeBlockedTitle': 'Close the open shift before removing this DJ',

    // inline note panel
    'djs.noteLabel': 'Working note for {name}',
    'djs.notePlaceholder': 'e.g. vinyl only, unavailable in August, brings their own controller',
    'djs.noteCounter': '{count}/{max} · only this club sees it',
    'djs.saveNote': 'Save note',
    'djs.noteTooLong': 'The note is {count} characters. Keep it under {max}.',

    // inline access panel
    'djs.confirmSuspendTitle': 'Suspend access for {name}?',
    'djs.confirmRestoreTitle': 'Restore access for {name}?',
    'djs.confirmSuspendBody': 'They stay on the roster with their note and history, but cannot sign in to this club or be assigned to a shift. You can restore access here at any time.',
    'djs.confirmRestoreBody': 'They can sign in to this club again and be assigned to a shift.',
    'djs.confirmSuspendAction': 'Yes, suspend access',
    'djs.confirmRestoreAction': 'Yes, restore access',

    // inline remove panel
    'djs.confirmRemoveTitle': 'Remove {name} from the roster?',
    'djs.confirmRemoveBodyKeep': 'Their access is revoked and they disappear from the roster. Shift and payout history is kept, and you can bring them back with “Show removed”.',
    'djs.confirmRemoveBodyHard': 'This invitation was never accepted, so it is deleted outright and the address can be invited again.',
    'djs.confirmRemoveAction': 'Yes, remove',
    'djs.removing': 'Removing…',

    // removed DJs
    'djs.removedTitle': 'Removed from the roster',
    'djs.removedDetail': 'Removed DJs cannot sign in. Their shifts and payouts stay in the history.',
    'djs.removedAt': 'removed {date}',
    'djs.restoring': 'Restoring…',
    'djs.restore': 'Restore',
    'djs.removedEmpty': 'No DJs have been removed from this roster.',

    // invitation form
    'djs.invitePanelTitle': 'Invite a DJ',
    'djs.invitePanelDetail': 'A single-use link, created by the club API.',
    'djs.fieldDisplayName': 'Display name',
    'djs.fieldEmail': 'Email',
    'djs.inviteReady': 'Invitation ready',
    'djs.inviteReadyDetail': 'Copy this single-use link and send it to the DJ.',
    'djs.inviteExpiresOnce': 'Expires {date} · can be used once.',
    'djs.copied': 'Copied',
    'djs.copyLink': 'Copy invitation link',
    'djs.copyBlocked': 'Clipboard access is blocked in this browser. Select the link and copy it manually.',
    'djs.copyFailed': 'The link was not copied — the browser refused clipboard access. Select it and copy manually.',
    'djs.inviting': 'Inviting…',
    'djs.sendInvitation': 'Send invitation',
    'djs.inviteHintSingleUse': 'Invitation links are single-use. The exact expiry is shown after the link is created.',

    // toasts
    'djs.toastSuspendedTitle': 'Access suspended for {name}',
    'djs.toastSuspendedMessage': 'They stay on the roster with their note and history, but cannot sign in to this club or be assigned to a shift. Restore access here at any time.',
    'djs.toastRestoredTitle': 'Access restored for {name}',
    'djs.toastAccessError': 'Access not changed',
    'djs.toastNoteCleared': 'Note cleared for {name}',
    'djs.toastNoteSaved': 'Note saved for {name}',
    'djs.toastNoteClearedMessage': 'The roster shows no note for this DJ.',
    'djs.toastNoteSavedMessage': 'The note is visible only to this club — not to the DJ and not to guests.',
    'djs.toastNoteError': 'Note not saved',
    'djs.toastRemovedTitle': '{name} removed from the roster',
    'djs.toastRemovedHard': 'The invitation was never accepted, so it was deleted outright. This address can be invited again.',
    'djs.toastRemovedSoft': 'Access is revoked and the DJ is off the roster. Shift and payout history stays, and “Show removed” brings them back.',
    'djs.toastRemoveError': 'DJ not removed',
    'djs.toastBackTitle': '{name} is back on the roster',
    'djs.toastBackMessage': 'Access is active again, so they can sign in and be assigned to a shift.',
    'djs.toastRestoreError': 'DJ not restored',
    'djs.toastInvitedTitle': 'Invitation created',
    'djs.toastInvitedMessage': 'Copy the single-use link and send it to {name}. It expires {date}.',
    'djs.toastInviteError': 'Invitation not created',

    // ages (bare quantities — the sentence carries the preposition)
    'djs.ageSecond': '{count} s',
    'djs.ageSeconds': '{count} s',
    'djs.ageMinute': '{count} min',
    'djs.ageMinutes': '{count} min',
    'djs.ageHour': '{count} h',
    'djs.ageHours': '{count} h',
    'djs.ageDay': '{count} d',
    'djs.ageDays': '{count} d',

    // failure sentences and the action phrases they wrap
    'djs.failHttp': 'Could not {action}. The server answered {status}: {message}. Try again; if it keeps failing, send this code to support.',
    'djs.failNotConnected': 'Could not {action}. The club DJ endpoint is not connected yet — this action will work once the service is available.',
    'djs.failOffline': 'Could not {action}. The request did not reach the server — check the connection and try again.',
    'djs.actionSuspend': 'suspend {name}',
    'djs.actionRestoreAccess': 'restore access for {name}',
    'djs.actionSaveNote': 'save the note for {name}',
    'djs.actionRemove': 'remove {name}',
    'djs.actionRestore': 'restore {name}',
    'djs.actionInvite': 'create the invitation',
  },
  es: {
    // page head
    'djs.heading': 'DJs',
    'djs.showRemoved': 'Mostrar eliminados',
    'djs.hideRemoved': 'Ocultar eliminados',

    // load and error states
    'djs.panelYourDjs': 'Tus DJs',
    'djs.panelYourDjsDetail': 'Quién puede entrar en este club y recibir un turno asignado.',
    'djs.errorTitle': 'No se pudo cargar la lista de DJs',
    'djs.errorTail': 'El equipo se lee de la API del club: reintenta y, si sigue fallando, los DJs no se pueden gestionar desde aquí.',
    'djs.stubTitle': 'El directorio de DJs espera al servicio',
    'djs.stubDetail': 'Los enlaces de invitación se crean cuando el endpoint de DJs del club esté disponible.',
    'djs.loadingDjs': 'Cargando DJs',

    // roster panel
    'djs.rosterTitle': 'Equipo',
    'djs.rosterDetail': 'La presencia viene de la consola del DJ; esta lista se actualiza cada 20 segundos.',
    'djs.onShiftCount': '{count} de turno',
    'djs.presenceSummary': '{online} en línea · {total} en el equipo',
    'djs.pollFailedStale': 'La última actualización falló, así que la presencia de abajo es de hace {age}.',
    'djs.pollFailedEarlier': 'La última actualización falló, así que la presencia de abajo viene de una petición anterior.',
    'djs.emptyTitle': 'No hay DJs en el equipo',
    'djs.emptyDetail': 'Crea una invitación abajo para dar a un DJ acceso a este espacio del club.',
    'djs.tableCaption': 'Equipo de DJs: acceso, presencia, turno y la nota del club sobre cada DJ.',
    'djs.colDj': 'DJ',
    'djs.colAccess': 'Acceso',
    'djs.colPresence': 'Presencia',
    'djs.colShift': 'Turno',
    'djs.colNote': 'Nota',
    'djs.colActions': 'Acciones',

    // presence and shift cells
    'djs.presenceOnline': 'En línea',
    'djs.presenceOffline': 'Desconectado',
    'djs.presenceUnknown': 'Desconocido',
    'djs.presenceLive': 'ahora en línea',
    'djs.presenceSeen': 'visto hace {age}',
    'djs.presenceLastSeen': 'visto por última vez hace {age}',
    'djs.presenceNever': 'nunca abrió la consola',
    'djs.inviteExpires': 'la invitación caduca el {date}',
    'djs.inviteNotAccepted': 'invitación aún sin aceptar',
    'djs.onShift': 'De turno',
    'djs.offShift': 'Fuera de turno',
    'djs.openSession': 'sesión abierta',
    'djs.noNote': 'Sin nota',

    // row actions
    'djs.editNote': 'Editar nota',
    'djs.addNote': 'Añadir nota',
    'djs.suspendAction': 'Suspender',
    'djs.restoreAccess': 'Restaurar acceso',
    'djs.remove': 'Eliminar',
    'djs.removeBlockedTitle': 'Cierra el turno abierto antes de eliminar a este DJ',

    // inline note panel
    'djs.noteLabel': 'Nota interna sobre {name}',
    'djs.notePlaceholder': 'p. ej. solo vinilo, no disponible en agosto, trae su propio controlador',
    'djs.noteCounter': '{count}/{max} · solo este club la ve',
    'djs.saveNote': 'Guardar nota',
    'djs.noteTooLong': 'La nota tiene {count} caracteres. Que no pase de {max}.',

    // inline access panel
    'djs.confirmSuspendTitle': '¿Suspender el acceso de {name}?',
    'djs.confirmRestoreTitle': '¿Restaurar el acceso de {name}?',
    'djs.confirmSuspendBody': 'Sigue en el equipo con su nota y su historial, pero no puede entrar en este club ni recibir un turno. Puedes restaurar el acceso aquí cuando quieras.',
    'djs.confirmRestoreBody': 'Podrá entrar de nuevo en este club y recibir un turno.',
    'djs.confirmSuspendAction': 'Sí, suspender el acceso',
    'djs.confirmRestoreAction': 'Sí, restaurar el acceso',

    // inline remove panel
    'djs.confirmRemoveTitle': '¿Eliminar a {name} del equipo?',
    'djs.confirmRemoveBodyKeep': 'Su acceso se revoca y desaparece del equipo. El historial de turnos y pagos se conserva, y puedes devolverlo con «Mostrar eliminados».',
    'djs.confirmRemoveBodyHard': 'Esta invitación nunca se aceptó, así que se borra del todo y la dirección se puede invitar de nuevo.',
    'djs.confirmRemoveAction': 'Sí, eliminar',
    'djs.removing': 'Eliminando…',

    // removed DJs
    'djs.removedTitle': 'Eliminados del equipo',
    'djs.removedDetail': 'Los DJs eliminados no pueden entrar. Sus turnos y pagos siguen en el historial.',
    'djs.removedAt': 'eliminado el {date}',
    'djs.restoring': 'Restaurando…',
    'djs.restore': 'Restaurar',
    'djs.removedEmpty': 'No se ha eliminado a ningún DJ de este equipo.',

    // invitation form
    'djs.invitePanelTitle': 'Invitar a un DJ',
    'djs.invitePanelDetail': 'Un enlace de un solo uso, creado por la API del club.',
    'djs.fieldDisplayName': 'Nombre visible',
    'djs.fieldEmail': 'Correo',
    'djs.inviteReady': 'Invitación lista',
    'djs.inviteReadyDetail': 'Copia este enlace de un solo uso y envíalo al DJ.',
    'djs.inviteExpiresOnce': 'Caduca el {date} · se puede usar una vez.',
    'djs.copied': 'Copiado',
    'djs.copyLink': 'Copiar enlace de invitación',
    'djs.copyBlocked': 'El acceso al portapapeles está bloqueado en este navegador. Selecciona el enlace y cópialo a mano.',
    'djs.copyFailed': 'El enlace no se copió: el navegador rechazó el acceso al portapapeles. Selecciónalo y cópialo a mano.',
    'djs.inviting': 'Invitando…',
    'djs.sendInvitation': 'Enviar invitación',
    'djs.inviteHintSingleUse': 'Los enlaces de invitación son de un solo uso. La caducidad exacta aparece después de crear el enlace.',

    // toasts
    'djs.toastSuspendedTitle': 'Acceso suspendido para {name}',
    'djs.toastSuspendedMessage': 'Sigue en el equipo con su nota y su historial, pero no puede entrar en este club ni recibir un turno. Restaura el acceso aquí cuando quieras.',
    'djs.toastRestoredTitle': 'Acceso restaurado para {name}',
    'djs.toastAccessError': 'Acceso sin cambios',
    'djs.toastNoteCleared': 'Nota de {name} borrada',
    'djs.toastNoteSaved': 'Nota de {name} guardada',
    'djs.toastNoteClearedMessage': 'El equipo no muestra ninguna nota para este DJ.',
    'djs.toastNoteSavedMessage': 'La nota solo la ve este club: ni el DJ ni los invitados.',
    'djs.toastNoteError': 'Nota sin guardar',
    'djs.toastRemovedTitle': '{name} eliminado del equipo',
    'djs.toastRemovedHard': 'La invitación nunca se aceptó, así que se borró del todo. Esta dirección se puede invitar de nuevo.',
    'djs.toastRemovedSoft': 'El acceso está revocado y el DJ sale del equipo. El historial de turnos y pagos se conserva, y «Mostrar eliminados» lo devuelve.',
    'djs.toastRemoveError': 'DJ sin eliminar',
    'djs.toastBackTitle': '{name} vuelve al equipo',
    'djs.toastBackMessage': 'El acceso vuelve a estar activo, así que puede entrar y recibir un turno.',
    'djs.toastRestoreError': 'DJ sin restaurar',
    'djs.toastInvitedTitle': 'Invitación creada',
    'djs.toastInvitedMessage': 'Copia el enlace de un solo uso y envíalo a {name}. Caduca el {date}.',
    'djs.toastInviteError': 'Invitación sin crear',

    // ages
    'djs.ageSecond': '{count} segundo',
    'djs.ageSeconds': '{count} segundos',
    'djs.ageMinute': '{count} minuto',
    'djs.ageMinutes': '{count} minutos',
    'djs.ageHour': '{count} hora',
    'djs.ageHours': '{count} horas',
    'djs.ageDay': '{count} día',
    'djs.ageDays': '{count} días',

    // failure sentences and action phrases
    'djs.failHttp': 'No se pudo {action}. El servidor respondió {status}: {message}. Vuelve a intentarlo; si sigue fallando, envía este código a soporte.',
    'djs.failNotConnected': 'No se pudo {action}. El endpoint de DJs del club aún no está conectado: la acción funcionará cuando el servicio esté disponible.',
    'djs.failOffline': 'No se pudo {action}. La petición no llegó al servidor: revisa la conexión y vuelve a intentarlo.',
    'djs.actionSuspend': 'suspender a {name}',
    'djs.actionRestoreAccess': 'restaurar el acceso de {name}',
    'djs.actionSaveNote': 'guardar la nota de {name}',
    'djs.actionRemove': 'eliminar a {name}',
    'djs.actionRestore': 'restaurar a {name}',
    'djs.actionInvite': 'crear la invitación',
  },
  fr: {
    // page head
    'djs.heading': 'DJs',
    'djs.showRemoved': 'Afficher les retirés',
    'djs.hideRemoved': 'Masquer les retirés',

    // load and error states
    'djs.panelYourDjs': 'Vos DJs',
    'djs.panelYourDjsDetail': 'Qui peut se connecter à ce club et être affecté à un service.',
    'djs.errorTitle': 'Impossible de charger la liste des DJs',
    'djs.errorTail': 'L’équipe est lue depuis l’API du club : réessayez, et si l’échec persiste, les DJs ne peuvent pas être gérés ici.',
    'djs.stubTitle': 'Le répertoire des DJs attend le service',
    'djs.stubDetail': 'Les liens d’invitation sont créés quand l’endpoint DJ du club est disponible.',
    'djs.loadingDjs': 'Chargement des DJs',

    // roster panel
    'djs.rosterTitle': 'Équipe',
    'djs.rosterDetail': 'La présence vient de la console DJ ; cette liste se rafraîchit toutes les 20 secondes.',
    'djs.onShiftCount': '{count} en service',
    'djs.presenceSummary': '{online} en ligne · {total} dans l’équipe',
    'djs.pollFailedStale': 'Le dernier rafraîchissement a échoué : la présence ci-dessous date de {age}.',
    'djs.pollFailedEarlier': 'Le dernier rafraîchissement a échoué : la présence ci-dessous vient d’une requête plus ancienne.',
    'djs.emptyTitle': 'Aucun DJ dans l’équipe',
    'djs.emptyDetail': 'Créez une invitation ci-dessous pour donner à un DJ l’accès à cet espace du club.',
    'djs.tableCaption': 'Équipe de DJs : accès, présence, service et note du club sur chaque DJ.',
    'djs.colDj': 'DJ',
    'djs.colAccess': 'Accès',
    'djs.colPresence': 'Présence',
    'djs.colShift': 'Service',
    'djs.colNote': 'Note',
    'djs.colActions': 'Opérations',

    // presence and shift cells
    'djs.presenceOnline': 'En ligne',
    'djs.presenceOffline': 'Hors ligne',
    'djs.presenceUnknown': 'Inconnu',
    'djs.presenceLive': 'en ligne maintenant',
    'djs.presenceSeen': 'vu il y a {age}',
    'djs.presenceLastSeen': 'dernière activité il y a {age}',
    'djs.presenceNever': 'n’a jamais ouvert la console',
    'djs.inviteExpires': 'invitation valable jusqu’au {date}',
    'djs.inviteNotAccepted': 'invitation pas encore acceptée',
    'djs.onShift': 'En service',
    'djs.offShift': 'Pas en service',
    'djs.openSession': 'session ouverte',
    'djs.noNote': 'Aucune note',

    // row actions
    'djs.editNote': 'Modifier la note',
    'djs.addNote': 'Ajouter une note',
    'djs.suspendAction': 'Suspendre',
    'djs.restoreAccess': 'Rétablir l’accès',
    'djs.remove': 'Retirer',
    'djs.removeBlockedTitle': 'Fermez le service en cours avant de retirer ce DJ',

    // inline note panel
    'djs.noteLabel': 'Note de travail sur {name}',
    'djs.notePlaceholder': 'ex. vinyle uniquement, absent en août, apporte son propre contrôleur',
    'djs.noteCounter': '{count}/{max} · seul ce club la voit',
    'djs.saveNote': 'Enregistrer la note',
    'djs.noteTooLong': 'La note fait {count} caractères. Restez sous {max}.',

    // inline access panel
    'djs.confirmSuspendTitle': 'Suspendre l’accès de {name} ?',
    'djs.confirmRestoreTitle': 'Rétablir l’accès de {name} ?',
    'djs.confirmSuspendBody': 'Le DJ reste dans l’équipe avec sa note et son historique, mais ne peut plus se connecter à ce club ni être affecté à un service. Vous pouvez rétablir l’accès ici à tout moment.',
    'djs.confirmRestoreBody': 'Le DJ pourra de nouveau se connecter à ce club et être affecté à un service.',
    'djs.confirmSuspendAction': 'Oui, suspendre l’accès',
    'djs.confirmRestoreAction': 'Oui, rétablir l’accès',

    // inline remove panel
    'djs.confirmRemoveTitle': 'Retirer {name} de l’équipe ?',
    'djs.confirmRemoveBodyKeep': 'Son accès est révoqué et le DJ disparaît de l’équipe. L’historique des services et des paiements est conservé, et « Afficher les retirés » permet de le réintégrer.',
    'djs.confirmRemoveBodyHard': 'Cette invitation n’a jamais été acceptée : elle est supprimée définitivement et l’adresse peut être invitée à nouveau.',
    'djs.confirmRemoveAction': 'Oui, retirer',
    'djs.removing': 'Retrait…',

    // removed DJs
    'djs.removedTitle': 'Retirés de l’équipe',
    'djs.removedDetail': 'Les DJs retirés ne peuvent plus se connecter. Leurs services et paiements restent dans l’historique.',
    'djs.removedAt': 'retiré le {date}',
    'djs.restoring': 'Rétablissement…',
    'djs.restore': 'Rétablir',
    'djs.removedEmpty': 'Aucun DJ n’a été retiré de cette équipe.',

    // invitation form
    'djs.invitePanelTitle': 'Inviter un DJ',
    'djs.invitePanelDetail': 'Un lien à usage unique, créé par l’API du club.',
    'djs.fieldDisplayName': 'Nom affiché',
    'djs.fieldEmail': 'E-mail',
    'djs.inviteReady': 'Invitation prête',
    'djs.inviteReadyDetail': 'Copiez ce lien à usage unique et envoyez-le au DJ.',
    'djs.inviteExpiresOnce': 'Expire le {date} · utilisable une fois.',
    'djs.copied': 'Copié',
    'djs.copyLink': 'Copier le lien d’invitation',
    'djs.copyBlocked': 'L’accès au presse-papiers est bloqué dans ce navigateur. Sélectionnez le lien et copiez-le à la main.',
    'djs.copyFailed': 'Le lien n’a pas été copié : le navigateur a refusé l’accès au presse-papiers. Sélectionnez-le et copiez à la main.',
    'djs.inviting': 'Invitation…',
    'djs.sendInvitation': 'Envoyer l’invitation',
    'djs.inviteHintSingleUse': 'Les liens d’invitation sont à usage unique. L’expiration exacte s’affiche une fois le lien créé.',

    // toasts
    'djs.toastSuspendedTitle': 'Accès suspendu pour {name}',
    'djs.toastSuspendedMessage': 'Le DJ reste dans l’équipe avec sa note et son historique, mais ne peut plus se connecter à ce club ni être affecté à un service. Rétablissez l’accès ici à tout moment.',
    'djs.toastRestoredTitle': 'Accès rétabli pour {name}',
    'djs.toastAccessError': 'Accès inchangé',
    'djs.toastNoteCleared': 'Note de {name} effacée',
    'djs.toastNoteSaved': 'Note de {name} enregistrée',
    'djs.toastNoteClearedMessage': 'L’équipe n’affiche aucune note pour ce DJ.',
    'djs.toastNoteSavedMessage': 'La note n’est visible que pour ce club : ni le DJ ni les invités.',
    'djs.toastNoteError': 'Note non enregistrée',
    'djs.toastRemovedTitle': '{name} retiré de l’équipe',
    'djs.toastRemovedHard': 'L’invitation n’a jamais été acceptée : elle a été supprimée définitivement. Cette adresse peut être invitée à nouveau.',
    'djs.toastRemovedSoft': 'L’accès est révoqué et le DJ quitte l’équipe. L’historique des services et des paiements est conservé, et « Afficher les retirés » le réintègre.',
    'djs.toastRemoveError': 'DJ non retiré',
    'djs.toastBackTitle': '{name} réintègre l’équipe',
    'djs.toastBackMessage': 'L’accès est de nouveau actif : le DJ peut se connecter et être affecté à un service.',
    'djs.toastRestoreError': 'DJ non rétabli',
    'djs.toastInvitedTitle': 'Invitation créée',
    'djs.toastInvitedMessage': 'Copiez le lien à usage unique et envoyez-le à {name}. Il expire le {date}.',
    'djs.toastInviteError': 'Invitation non créée',

    // ages
    'djs.ageSecond': '{count} seconde',
    'djs.ageSeconds': '{count} secondes',
    'djs.ageMinute': '{count} minute',
    'djs.ageMinutes': '{count} minutes',
    'djs.ageHour': '{count} heure',
    'djs.ageHours': '{count} heures',
    'djs.ageDay': '{count} jour',
    'djs.ageDays': '{count} jours',

    // failure sentences and action phrases
    'djs.failHttp': 'Impossible de {action}. Le serveur a répondu {status} : {message}. Réessayez ; si l’échec persiste, envoyez ce code au support.',
    'djs.failNotConnected': 'Impossible de {action}. L’endpoint DJ du club n’est pas encore branché : l’action fonctionnera dès que le service sera disponible.',
    'djs.failOffline': 'Impossible de {action}. La requête n’a pas atteint le serveur : vérifiez la connexion et réessayez.',
    'djs.actionSuspend': 'suspendre {name}',
    'djs.actionRestoreAccess': 'rétablir l’accès de {name}',
    'djs.actionSaveNote': 'enregistrer la note de {name}',
    'djs.actionRemove': 'retirer {name}',
    'djs.actionRestore': 'rétablir {name}',
    'djs.actionInvite': 'créer l’invitation',
  },
  de: {
    // page head
    'djs.heading': 'DJs',
    'djs.showRemoved': 'Entfernte anzeigen',
    'djs.hideRemoved': 'Entfernte ausblenden',

    // load and error states
    'djs.panelYourDjs': 'Deine DJs',
    'djs.panelYourDjsDetail': 'Wer sich in diesem Club anmelden und einer Schicht zugeteilt werden kann.',
    'djs.errorTitle': 'Die DJ-Liste konnte nicht geladen werden',
    'djs.errorTail': 'Das Team wird aus der Club-API gelesen — versuche es erneut; wenn es weiter scheitert, lassen sich die DJs von hier aus nicht verwalten.',
    'djs.stubTitle': 'Das DJ-Verzeichnis wartet auf den Dienst',
    'djs.stubDetail': 'Einladungslinks entstehen, sobald der DJ-Endpunkt des Clubs verfügbar ist.',
    'djs.loadingDjs': 'DJs werden geladen',

    // roster panel
    'djs.rosterTitle': 'Team',
    'djs.rosterDetail': 'Die Präsenz kommt aus der DJ-Konsole; diese Liste aktualisiert sich alle 20 Sekunden.',
    'djs.onShiftCount': '{count} im Dienst',
    'djs.presenceSummary': '{online} online · {total} im Team',
    'djs.pollFailedStale': 'Die letzte Aktualisierung ist fehlgeschlagen, die Präsenz unten wurde vor {age} gelesen.',
    'djs.pollFailedEarlier': 'Die letzte Aktualisierung ist fehlgeschlagen, die Präsenz unten stammt von einer früheren Anfrage.',
    'djs.emptyTitle': 'Keine DJs im Team',
    'djs.emptyDetail': 'Erstelle unten eine Einladung, damit ein DJ Zugriff auf diesen Clubbereich bekommt.',
    'djs.tableCaption': 'DJ-Team: Zugriff, Präsenz, Schicht und die Notiz des Clubs zu jedem DJ.',
    'djs.colDj': 'DJ',
    'djs.colAccess': 'Zugriff',
    'djs.colPresence': 'Präsenz',
    'djs.colShift': 'Schicht',
    'djs.colNote': 'Notiz',
    'djs.colActions': 'Aktionen',

    // presence and shift cells
    'djs.presenceOnline': 'Verbunden',
    'djs.presenceOffline': 'Nicht verbunden',
    'djs.presenceUnknown': 'Unbekannt',
    'djs.presenceLive': 'jetzt online',
    'djs.presenceSeen': 'gesehen vor {age}',
    'djs.presenceLastSeen': 'zuletzt gesehen vor {age}',
    'djs.presenceNever': 'hat die Konsole nie geöffnet',
    'djs.inviteExpires': 'Einladung läuft ab am {date}',
    'djs.inviteNotAccepted': 'Einladung noch nicht angenommen',
    'djs.onShift': 'Im Dienst',
    'djs.offShift': 'Nicht im Dienst',
    'djs.openSession': 'Schicht offen',
    'djs.noNote': 'Keine Notiz',

    // row actions
    'djs.editNote': 'Notiz bearbeiten',
    'djs.addNote': 'Notiz hinzufügen',
    'djs.suspendAction': 'Sperren',
    'djs.restoreAccess': 'Zugriff wiederherstellen',
    'djs.remove': 'Entfernen',
    'djs.removeBlockedTitle': 'Schließe die offene Schicht, bevor du diesen DJ entfernst',

    // inline note panel
    'djs.noteLabel': 'Arbeitsnotiz zu {name}',
    'djs.notePlaceholder': 'z. B. nur Vinyl, im August nicht verfügbar, bringt eigenen Controller',
    'djs.noteCounter': '{count}/{max} · nur dieser Club sieht sie',
    'djs.saveNote': 'Notiz speichern',
    'djs.noteTooLong': 'Die Notiz hat {count} Zeichen. Bleibe unter {max}.',

    // inline access panel
    'djs.confirmSuspendTitle': 'Zugriff für {name} sperren?',
    'djs.confirmRestoreTitle': 'Zugriff für {name} wiederherstellen?',
    'djs.confirmSuspendBody': 'Die Person bleibt mit Notiz und Verlauf im Team, kann sich aber nicht in diesem Club anmelden und keiner Schicht zugeteilt werden. Du kannst den Zugriff hier jederzeit wiederherstellen.',
    'djs.confirmRestoreBody': 'Die Person kann sich wieder in diesem Club anmelden und einer Schicht zugeteilt werden.',
    'djs.confirmSuspendAction': 'Ja, Zugriff sperren',
    'djs.confirmRestoreAction': 'Ja, Zugriff wiederherstellen',

    // inline remove panel
    'djs.confirmRemoveTitle': '{name} aus dem Team entfernen?',
    'djs.confirmRemoveBodyKeep': 'Der Zugriff wird entzogen und die Person verschwindet aus dem Team. Schicht- und Auszahlungsverlauf bleiben erhalten, „Entfernte anzeigen“ holt sie zurück.',
    'djs.confirmRemoveBodyHard': 'Diese Einladung wurde nie angenommen, sie wird also endgültig gelöscht und die Adresse kann erneut eingeladen werden.',
    'djs.confirmRemoveAction': 'Ja, entfernen',
    'djs.removing': 'Wird entfernt…',

    // removed DJs
    'djs.removedTitle': 'Aus dem Team entfernt',
    'djs.removedDetail': 'Entfernte DJs können sich nicht anmelden. Ihre Schichten und Auszahlungen bleiben im Verlauf.',
    'djs.removedAt': 'entfernt am {date}',
    'djs.restoring': 'Wird wiederhergestellt…',
    'djs.restore': 'Wiederherstellen',
    'djs.removedEmpty': 'Aus diesem Team wurde kein DJ entfernt.',

    // invitation form
    'djs.invitePanelTitle': 'DJ einladen',
    'djs.invitePanelDetail': 'Ein Einmal-Link, erzeugt von der Club-API.',
    'djs.fieldDisplayName': 'Anzeigename',
    'djs.fieldEmail': 'E-Mail',
    'djs.inviteReady': 'Einladung bereit',
    'djs.inviteReadyDetail': 'Kopiere diesen Einmal-Link und schicke ihn an den DJ.',
    'djs.inviteExpiresOnce': 'Läuft ab am {date} · einmal verwendbar.',
    'djs.copied': 'Kopiert',
    'djs.copyLink': 'Einladungslink kopieren',
    'djs.copyBlocked': 'Der Zugriff auf die Zwischenablage ist in diesem Browser gesperrt. Markiere den Link und kopiere ihn von Hand.',
    'djs.copyFailed': 'Der Link wurde nicht kopiert — der Browser hat den Zugriff auf die Zwischenablage verweigert. Markiere ihn und kopiere von Hand.',
    'djs.inviting': 'Wird eingeladen…',
    'djs.sendInvitation': 'Einladung senden',
    'djs.inviteHintSingleUse': 'Einladungslinks gelten einmal. Die genaue Ablaufzeit erscheint, sobald der Link erstellt ist.',

    // toasts
    'djs.toastSuspendedTitle': 'Zugriff für {name} gesperrt',
    'djs.toastSuspendedMessage': 'Die Person bleibt mit Notiz und Verlauf im Team, kann sich aber nicht in diesem Club anmelden und keiner Schicht zugeteilt werden. Stelle den Zugriff hier jederzeit wieder her.',
    'djs.toastRestoredTitle': 'Zugriff für {name} wiederhergestellt',
    'djs.toastAccessError': 'Zugriff nicht geändert',
    'djs.toastNoteCleared': 'Notiz zu {name} gelöscht',
    'djs.toastNoteSaved': 'Notiz zu {name} gespeichert',
    'djs.toastNoteClearedMessage': 'Im Team steht keine Notiz zu diesem DJ.',
    'djs.toastNoteSavedMessage': 'Die Notiz sieht nur dieser Club — nicht der DJ und nicht die Gäste.',
    'djs.toastNoteError': 'Notiz nicht gespeichert',
    'djs.toastRemovedTitle': '{name} aus dem Team entfernt',
    'djs.toastRemovedHard': 'Die Einladung wurde nie angenommen und endgültig gelöscht. Diese Adresse kann erneut eingeladen werden.',
    'djs.toastRemovedSoft': 'Der Zugriff ist entzogen und der DJ ist aus dem Team. Schicht- und Auszahlungsverlauf bleiben, „Entfernte anzeigen“ holt ihn zurück.',
    'djs.toastRemoveError': 'DJ nicht entfernt',
    'djs.toastBackTitle': '{name} ist wieder im Team',
    'djs.toastBackMessage': 'Der Zugriff ist wieder aktiv, die Person kann sich anmelden und einer Schicht zugeteilt werden.',
    'djs.toastRestoreError': 'DJ nicht wiederhergestellt',
    'djs.toastInvitedTitle': 'Einladung erstellt',
    'djs.toastInvitedMessage': 'Kopiere den Einmal-Link und schicke ihn an {name}. Er läuft am {date} ab.',
    'djs.toastInviteError': 'Einladung nicht erstellt',

    // ages
    'djs.ageSecond': '{count} Sekunde',
    'djs.ageSeconds': '{count} Sekunden',
    'djs.ageMinute': '{count} Minute',
    'djs.ageMinutes': '{count} Minuten',
    'djs.ageHour': '{count} Stunde',
    'djs.ageHours': '{count} Stunden',
    'djs.ageDay': '{count} Tag',
    'djs.ageDays': '{count} Tagen',

    // failure sentences and action phrases
    'djs.failHttp': '{action} war nicht möglich. Der Server antwortete {status}: {message}. Versuche es erneut; wenn es weiter scheitert, schicke diesen Code an den Support.',
    'djs.failNotConnected': '{action} war nicht möglich. Der DJ-Endpunkt des Clubs ist noch nicht angebunden — die Aktion klappt, sobald der Dienst verfügbar ist.',
    'djs.failOffline': '{action} war nicht möglich. Die Anfrage hat den Server nicht erreicht — prüfe die Verbindung und versuche es erneut.',
    'djs.actionSuspend': '{name} zu sperren',
    'djs.actionRestoreAccess': 'Den Zugriff für {name} wiederherzustellen',
    'djs.actionSaveNote': 'Die Notiz zu {name} zu speichern',
    'djs.actionRemove': '{name} zu entfernen',
    'djs.actionRestore': '{name} wiederherzustellen',
    'djs.actionInvite': 'Die Einladung zu erstellen',
  },
  pt: {
    // page head
    'djs.heading': 'DJs',
    'djs.showRemoved': 'Mostrar removidos',
    'djs.hideRemoved': 'Ocultar removidos',

    // load and error states
    'djs.panelYourDjs': 'Os seus DJs',
    'djs.panelYourDjsDetail': 'Quem pode entrar neste clube e receber um turno atribuído.',
    'djs.errorTitle': 'Não foi possível carregar a lista de DJs',
    'djs.errorTail': 'A equipa é lida da API do clube: tente de novo e, se continuar a falhar, os DJs não podem ser geridos aqui.',
    'djs.stubTitle': 'O diretório de DJs aguarda o serviço',
    'djs.stubDetail': 'Os links de convite são criados quando o endpoint de DJs do clube estiver disponível.',
    'djs.loadingDjs': 'A carregar DJs',

    // roster panel
    'djs.rosterTitle': 'Equipa',
    'djs.rosterDetail': 'A presença vem da consola do DJ; esta lista atualiza-se a cada 20 segundos.',
    'djs.onShiftCount': '{count} em turno',
    'djs.presenceSummary': '{online} online · {total} na equipa',
    'djs.pollFailedStale': 'A última atualização falhou, por isso a presença abaixo é de há {age}.',
    'djs.pollFailedEarlier': 'A última atualização falhou, por isso a presença abaixo vem de um pedido anterior.',
    'djs.emptyTitle': 'Não há DJs na equipa',
    'djs.emptyDetail': 'Crie um convite abaixo para dar a um DJ acesso a este espaço do clube.',
    'djs.tableCaption': 'Equipa de DJs: acesso, presença, turno e a nota do clube sobre cada DJ.',
    'djs.colDj': 'DJ',
    'djs.colAccess': 'Acesso',
    'djs.colPresence': 'Presença',
    'djs.colShift': 'Turno',
    'djs.colNote': 'Nota',
    'djs.colActions': 'Ações',

    // presence and shift cells
    'djs.presenceOnline': 'Em linha',
    'djs.presenceOffline': 'Desligado',
    'djs.presenceUnknown': 'Desconhecido',
    'djs.presenceLive': 'agora online',
    'djs.presenceSeen': 'visto há {age}',
    'djs.presenceLastSeen': 'visto pela última vez há {age}',
    'djs.presenceNever': 'nunca abriu a consola',
    'djs.inviteExpires': 'o convite expira em {date}',
    'djs.inviteNotAccepted': 'convite ainda não aceite',
    'djs.onShift': 'Em turno',
    'djs.offShift': 'Fora de turno',
    'djs.openSession': 'sessão aberta',
    'djs.noNote': 'Sem nota',

    // row actions
    'djs.editNote': 'Editar nota',
    'djs.addNote': 'Adicionar nota',
    'djs.suspendAction': 'Suspender',
    'djs.restoreAccess': 'Restaurar acesso',
    'djs.remove': 'Remover',
    'djs.removeBlockedTitle': 'Feche o turno aberto antes de remover este DJ',

    // inline note panel
    'djs.noteLabel': 'Nota interna sobre {name}',
    'djs.notePlaceholder': 'p. ex. só vinil, indisponível em agosto, traz o seu próprio controlador',
    'djs.noteCounter': '{count}/{max} · só este clube a vê',
    'djs.saveNote': 'Guardar nota',
    'djs.noteTooLong': 'A nota tem {count} caracteres. Mantenha-a abaixo de {max}.',

    // inline access panel
    'djs.confirmSuspendTitle': 'Suspender o acesso de {name}?',
    'djs.confirmRestoreTitle': 'Restaurar o acesso de {name}?',
    'djs.confirmSuspendBody': 'Continua na equipa com a sua nota e o seu histórico, mas não pode entrar neste clube nem receber um turno. Pode restaurar o acesso aqui a qualquer momento.',
    'djs.confirmRestoreBody': 'Pode voltar a entrar neste clube e receber um turno.',
    'djs.confirmSuspendAction': 'Sim, suspender o acesso',
    'djs.confirmRestoreAction': 'Sim, restaurar o acesso',

    // inline remove panel
    'djs.confirmRemoveTitle': 'Remover {name} da equipa?',
    'djs.confirmRemoveBodyKeep': 'O acesso é revogado e o DJ desaparece da equipa. O histórico de turnos e pagamentos mantém-se e «Mostrar removidos» devolve-o.',
    'djs.confirmRemoveBodyHard': 'Este convite nunca foi aceite, por isso é apagado de vez e o endereço pode voltar a ser convidado.',
    'djs.confirmRemoveAction': 'Sim, remover',
    'djs.removing': 'A remover…',

    // removed DJs
    'djs.removedTitle': 'Removidos da equipa',
    'djs.removedDetail': 'Os DJs removidos não podem entrar. Os seus turnos e pagamentos ficam no histórico.',
    'djs.removedAt': 'removido em {date}',
    'djs.restoring': 'A restaurar…',
    'djs.restore': 'Restaurar',
    'djs.removedEmpty': 'Nenhum DJ foi removido desta equipa.',

    // invitation form
    'djs.invitePanelTitle': 'Convidar um DJ',
    'djs.invitePanelDetail': 'Um link de uso único, criado pela API do clube.',
    'djs.fieldDisplayName': 'Nome visível',
    'djs.fieldEmail': 'E-mail',
    'djs.inviteReady': 'Convite pronto',
    'djs.inviteReadyDetail': 'Copie este link de uso único e envie-o ao DJ.',
    'djs.inviteExpiresOnce': 'Expira em {date} · pode ser usado uma vez.',
    'djs.copied': 'Copiado',
    'djs.copyLink': 'Copiar link de convite',
    'djs.copyBlocked': 'O acesso à área de transferência está bloqueado neste navegador. Selecione o link e copie-o à mão.',
    'djs.copyFailed': 'O link não foi copiado — o navegador recusou o acesso à área de transferência. Selecione-o e copie à mão.',
    'djs.inviting': 'A convidar…',
    'djs.sendInvitation': 'Enviar convite',
    'djs.inviteHintSingleUse': 'Os links de convite são de uso único. A validade exata aparece depois de criar o link.',

    // toasts
    'djs.toastSuspendedTitle': 'Acesso suspenso para {name}',
    'djs.toastSuspendedMessage': 'Continua na equipa com a sua nota e o seu histórico, mas não pode entrar neste clube nem receber um turno. Restaure o acesso aqui a qualquer momento.',
    'djs.toastRestoredTitle': 'Acesso restaurado para {name}',
    'djs.toastAccessError': 'Acesso inalterado',
    'djs.toastNoteCleared': 'Nota de {name} apagada',
    'djs.toastNoteSaved': 'Nota de {name} guardada',
    'djs.toastNoteClearedMessage': 'A equipa não mostra qualquer nota para este DJ.',
    'djs.toastNoteSavedMessage': 'A nota só é visível para este clube: nem o DJ nem os convidados.',
    'djs.toastNoteError': 'Nota não guardada',
    'djs.toastRemovedTitle': '{name} removido da equipa',
    'djs.toastRemovedHard': 'O convite nunca foi aceite, por isso foi apagado de vez. Este endereço pode voltar a ser convidado.',
    'djs.toastRemovedSoft': 'O acesso está revogado e o DJ sai da equipa. O histórico de turnos e pagamentos mantém-se e «Mostrar removidos» devolve-o.',
    'djs.toastRemoveError': 'DJ não removido',
    'djs.toastBackTitle': '{name} está de volta à equipa',
    'djs.toastBackMessage': 'O acesso está ativo outra vez, por isso pode entrar e receber um turno.',
    'djs.toastRestoreError': 'DJ não restaurado',
    'djs.toastInvitedTitle': 'Convite criado',
    'djs.toastInvitedMessage': 'Copie o link de uso único e envie-o a {name}. Expira em {date}.',
    'djs.toastInviteError': 'Convite não criado',

    // ages
    'djs.ageSecond': '{count} segundo',
    'djs.ageSeconds': '{count} segundos',
    'djs.ageMinute': '{count} minuto',
    'djs.ageMinutes': '{count} minutos',
    'djs.ageHour': '{count} hora',
    'djs.ageHours': '{count} horas',
    'djs.ageDay': '{count} dia',
    'djs.ageDays': '{count} dias',

    // failure sentences and action phrases
    'djs.failHttp': 'Não foi possível {action}. O servidor respondeu {status}: {message}. Tente de novo; se continuar a falhar, envie este código ao suporte.',
    'djs.failNotConnected': 'Não foi possível {action}. O endpoint de DJs do clube ainda não está ligado: a ação funciona quando o serviço estiver disponível.',
    'djs.failOffline': 'Não foi possível {action}. O pedido não chegou ao servidor: verifique a ligação e tente de novo.',
    'djs.actionSuspend': 'suspender {name}',
    'djs.actionRestoreAccess': 'restaurar o acesso de {name}',
    'djs.actionSaveNote': 'guardar a nota de {name}',
    'djs.actionRemove': 'remover {name}',
    'djs.actionRestore': 'restaurar {name}',
    'djs.actionInvite': 'criar o convite',
  },
  ru: {
    // page head
    'djs.heading': 'Диджеи',
    'djs.showRemoved': 'Показать удалённых',
    'djs.hideRemoved': 'Скрыть удалённых',

    // load and error states
    'djs.panelYourDjs': 'Ваши диджеи',
    'djs.panelYourDjsDetail': 'Кто может входить в этот клуб и получать смену.',
    'djs.errorTitle': 'Не удалось загрузить список диджеев',
    'djs.errorTail': 'Состав читается из API клуба — повторите, а если ошибка повторяется, управлять диджеями отсюда не получится.',
    'djs.stubTitle': 'Справочник диджеев ждёт сервис',
    'djs.stubDetail': 'Ссылки-приглашения создаются, когда эндпоинт диджеев клуба станет доступен.',
    'djs.loadingDjs': 'Загружаем диджеев',

    // roster panel
    'djs.rosterTitle': 'Состав',
    'djs.rosterDetail': 'Присутствие берётся из пульта диджея; список обновляется каждые 20 секунд.',
    'djs.onShiftCount': '{count} на смене',
    'djs.presenceSummary': '{online} на связи · {total} в составе',
    'djs.pollFailedStale': 'Последнее обновление не удалось, поэтому присутствие ниже — {age} назад.',
    'djs.pollFailedEarlier': 'Последнее обновление не удалось, поэтому присутствие ниже — из более раннего запроса.',
    'djs.emptyTitle': 'В составе нет диджеев',
    'djs.emptyDetail': 'Создайте приглашение ниже, чтобы дать диджею доступ к этому клубному пространству.',
    'djs.tableCaption': 'Состав диджеев: доступ, присутствие, смена и заметка клуба по каждому диджею.',
    'djs.colDj': 'DJ',
    'djs.colAccess': 'Доступ',
    'djs.colPresence': 'Присутствие',
    'djs.colShift': 'Смена',
    'djs.colNote': 'Заметка',
    'djs.colActions': 'Действия',

    // presence and shift cells
    'djs.presenceOnline': 'На связи',
    'djs.presenceOffline': 'Не в сети',
    'djs.presenceUnknown': 'Неизвестно',
    'djs.presenceLive': 'сейчас на связи',
    'djs.presenceSeen': 'был в сети {age} назад',
    'djs.presenceLastSeen': 'последний раз в сети {age} назад',
    'djs.presenceNever': 'ни разу не открывал пульт',
    'djs.inviteExpires': 'приглашение действует до {date}',
    'djs.inviteNotAccepted': 'приглашение ещё не принято',
    'djs.onShift': 'На смене',
    'djs.offShift': 'Не на смене',
    'djs.openSession': 'смена открыта',
    'djs.noNote': 'Без заметки',

    // row actions
    'djs.editNote': 'Изменить заметку',
    'djs.addNote': 'Добавить заметку',
    'djs.suspendAction': 'Заблокировать',
    'djs.restoreAccess': 'Вернуть доступ',
    'djs.remove': 'Удалить',
    'djs.removeBlockedTitle': 'Закройте открытую смену, прежде чем удалять диджея',

    // inline note panel
    'djs.noteLabel': 'Рабочая заметка о {name}',
    'djs.notePlaceholder': 'например: только винил, в августе недоступен, приходит со своим контроллером',
    'djs.noteCounter': '{count}/{max} · виден только этому клубу',
    'djs.saveNote': 'Сохранить заметку',
    'djs.noteTooLong': 'В заметке {count} символов. Нужно меньше {max}.',

    // inline access panel
    'djs.confirmSuspendTitle': 'Заблокировать доступ для {name}?',
    'djs.confirmRestoreTitle': 'Вернуть доступ для {name}?',
    'djs.confirmSuspendBody': 'Диджей остаётся в составе со своей заметкой и историей, но не может входить в этот клуб и получать смену. Доступ можно вернуть здесь в любой момент.',
    'djs.confirmRestoreBody': 'Диджей снова сможет входить в этот клуб и получать смену.',
    'djs.confirmSuspendAction': 'Да, заблокировать доступ',
    'djs.confirmRestoreAction': 'Да, вернуть доступ',

    // inline remove panel
    'djs.confirmRemoveTitle': 'Удалить {name} из состава?',
    'djs.confirmRemoveBodyKeep': 'Доступ отзывается, и диджей исчезает из состава. История смен и выплат сохраняется, а «Показать удалённых» вернёт его обратно.',
    'djs.confirmRemoveBodyHard': 'Приглашение так и не приняли, поэтому оно удаляется полностью, а на этот адрес можно пригласить снова.',
    'djs.confirmRemoveAction': 'Да, удалить',
    'djs.removing': 'Удаляем…',

    // removed DJs
    'djs.removedTitle': 'Удалённые из состава',
    'djs.removedDetail': 'Удалённые диджеи не могут входить. Их смены и выплаты остаются в истории.',
    'djs.removedAt': 'удалён {date}',
    'djs.restoring': 'Возвращаем…',
    'djs.restore': 'Вернуть',
    'djs.removedEmpty': 'Из этого состава никого не удаляли.',

    // invitation form
    'djs.invitePanelTitle': 'Пригласить диджея',
    'djs.invitePanelDetail': 'Ссылка на один раз, создаётся через API клуба.',
    'djs.fieldDisplayName': 'Отображаемое имя',
    'djs.fieldEmail': 'E-mail',
    'djs.inviteReady': 'Приглашение готово',
    'djs.inviteReadyDetail': 'Скопируйте эту одноразовую ссылку и отправьте диджею.',
    'djs.inviteExpiresOnce': 'Действует до {date} · можно использовать один раз.',
    'djs.copied': 'Скопировано',
    'djs.copyLink': 'Скопировать ссылку-приглашение',
    'djs.copyBlocked': 'Доступ к буферу обмена в этом браузере закрыт. Выделите ссылку и скопируйте её вручную.',
    'djs.copyFailed': 'Ссылка не скопировалась — браузер отклонил доступ к буферу. Выделите её и скопируйте вручную.',
    'djs.inviting': 'Приглашаем…',
    'djs.sendInvitation': 'Отправить приглашение',
    'djs.inviteHintSingleUse': 'Ссылки-приглашения одноразовые. Точный срок виден после создания ссылки.',

    // toasts
    'djs.toastSuspendedTitle': 'Доступ для {name} заблокирован',
    'djs.toastSuspendedMessage': 'Диджей остаётся в составе со своей заметкой и историей, но не может входить в этот клуб и получать смену. Вернуть доступ можно здесь в любой момент.',
    'djs.toastRestoredTitle': 'Доступ для {name} восстановлен',
    'djs.toastAccessError': 'Доступ не изменён',
    'djs.toastNoteCleared': 'Заметка о {name} очищена',
    'djs.toastNoteSaved': 'Заметка о {name} сохранена',
    'djs.toastNoteClearedMessage': 'В составе по этому диджею заметки нет.',
    'djs.toastNoteSavedMessage': 'Заметку видит только этот клуб — не диджей и не гости.',
    'djs.toastNoteError': 'Заметка не сохранена',
    'djs.toastRemovedTitle': '{name} удалён из состава',
    'djs.toastRemovedHard': 'Приглашение так и не приняли, поэтому оно удалено полностью. На этот адрес можно пригласить снова.',
    'djs.toastRemovedSoft': 'Доступ отозван, диджей вне состава. История смен и выплат сохраняется, а «Показать удалённых» вернёт его.',
    'djs.toastRemoveError': 'Диджей не удалён',
    'djs.toastBackTitle': '{name} снова в составе',
    'djs.toastBackMessage': 'Доступ снова активен: диджей может входить и получать смену.',
    'djs.toastRestoreError': 'Диджей не восстановлен',
    'djs.toastInvitedTitle': 'Приглашение создано',
    'djs.toastInvitedMessage': 'Скопируйте одноразовую ссылку и отправьте {name}. Она действует до {date}.',
    'djs.toastInviteError': 'Приглашение не создано',

    // ages
    'djs.ageSecond': '{count} сек',
    'djs.ageSeconds': '{count} сек',
    'djs.ageMinute': '{count} мин',
    'djs.ageMinutes': '{count} мин',
    'djs.ageHour': '{count} ч',
    'djs.ageHours': '{count} ч',
    'djs.ageDay': '{count} дн',
    'djs.ageDays': '{count} дн',

    // failure sentences and action phrases
    'djs.failHttp': 'Не удалось {action}. Сервер ответил {status}: {message}. Повторите попытку; если ошибка повторяется, отправьте этот код в поддержку.',
    'djs.failNotConnected': 'Не удалось {action}. Эндпоинт диджеев клуба ещё не подключён — действие заработает, когда сервис будет доступен.',
    'djs.failOffline': 'Не удалось {action}. Запрос не дошёл до сервера — проверьте соединение и повторите.',
    'djs.actionSuspend': 'заблокировать {name}',
    'djs.actionRestoreAccess': 'вернуть доступ для {name}',
    'djs.actionSaveNote': 'сохранить заметку о {name}',
    'djs.actionRemove': 'удалить {name}',
    'djs.actionRestore': 'восстановить {name}',
    'djs.actionInvite': 'создать приглашение',
  },
} satisfies MessageCatalog;