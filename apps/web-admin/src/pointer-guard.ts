/**
 * Pointer selection guard — a click is a click, not a selection.
 *
 * Why this exists
 * ---------------
 * Two leftovers were reported after using the panels with a mouse:
 *
 *   1. Dragging across a control (a button, a table action, a link, the account
 *      switcher) painted a text selection over its label. The browser treats a
 *      `<button>`'s text as selectable, so a slightly sloppy press-and-move
 *      left "Mark played" or the account name highlighted even though the
 *      control had done its job.
 *   2. A selection made earlier on the page — say a paragraph in a card —
 *      survived a click on a control. On the account switcher that is the worst
 *      case: the menu opens and the paragraph behind it stays highlighted, so
 *      it reads as if the click selected the page.
 *
 * What it does
 * ------------
 *   - CSS (see the "Selection" block in `panel.css`) makes chrome — buttons,
 *     links, menu items, segmented options, chips, nav rows, the account row —
 *     unselectable, so a drag across a control cannot select its label. Text
 *     entry, table cells and body copy stay selectable, because that text is
 *     what people copy.
 *   - This module clears an existing selection on a pointer press over a
 *     control, which is what a click outside a selection means everywhere else
 *     in the platform's own UI. A press inside the selection (dragging it) and
 *     a press inside a text field (repositioning the caret) are left alone.
 *
 * It never calls `preventDefault()`, so drag-to-select inside a paragraph, text
 * selection inside an input, and every element's own pointer handler keep
 * working exactly as before. Installing twice is a no-op.
 */

/**
 * Controls whose label is chrome rather than copy. Keep in step with the
 * "Selection" block in `panel.css`.
 *
 * Two absences are deliberate:
 *   - no bare `a[href]`: an inline link inside a sentence is prose with an
 *     action attached, and clicking it must not clear a selection the user is
 *     reading. Links used as controls carry a button class or an interactive
 *     role, which are listed here;
 *   - no bare `label`: `.field` wraps the whole control (caption + input), so
 *     matching it would make every form caption unselectable. Only tile-style
 *     choices are listed by class.
 */
const CONTROL_SELECTOR = [
  'button',
  'summary',
  'select',
  '[role="button"]',
  '[role="menuitem"]',
  '[role="menuitemradio"]',
  '[role="option"]',
  '[role="tab"]',
  '[role="radio"]',
  '[role="switch"]',
  '.btn',
  '.btn-primary',
  '.btn-secondary',
  '.btn-quiet',
  '.btn-outline',
  '.btn-danger',
  '.club-button',
  '.connect-btn-primary',
  '.connect-btn-ghost',
  '.panel-nav-link',
  '.panel-account',
  '.panel-menu-item',
  '.panel-card-link',
  '.seg-item',
  '.chip',
  '.list-row',
  '.action-row',
  '.auth-link',
  '.role-option',
  '.auth-choice',
  '.panel-language-option',
  '.qr-switch-copy',
].join(',');

/** Typing surfaces keep their own selection and caret behaviour. */
const TEXT_ENTRY_SELECTOR = [
  'input:not([type="checkbox"]):not([type="radio"]):not([type="range"]):not([type="file"]):not([type="button"]):not([type="submit"])',
  'textarea',
  '[contenteditable="true"]',
].join(',');

function selectionInside(selection: Selection, node: Element): boolean {
  for (let index = 0; index < selection.rangeCount; index += 1) {
    const range = selection.getRangeAt(index);
    if (range.intersectsNode(node)) return true;
  }
  return false;
}

function onPointerDown(event: PointerEvent): void {
  const target = event.target instanceof Element ? event.target : null;
  if (!target) return;
  if (target.closest(TEXT_ENTRY_SELECTOR)) return;

  const selection = window.getSelection();
  if (!selection || selection.isCollapsed) return;

  const control = target.closest(CONTROL_SELECTOR);
  if (!control) return;
  // A press on the selected text itself starts a drag of that selection.
  if (selectionInside(selection, control)) return;

  selection.removeAllRanges();
}

/**
 * Installs the guard once per document. Called from each app entry point.
 */
export function installPointerSelectionGuard(): void {
  if (typeof document === 'undefined') return;
  const flag = '__dropitPointerSelectionGuard';
  const documentWithFlag = document as Document & Record<string, unknown>;
  if (documentWithFlag[flag]) return;
  documentWithFlag[flag] = true;

  document.addEventListener('pointerdown', onPointerDown, { capture: true, passive: true });
}
