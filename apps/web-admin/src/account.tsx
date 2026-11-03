/**
 * Account — the single place account management lives in every panel.
 *
 * Before this component the same three facts were spread over three controls:
 * a workspace "switcher" in the rail (which did nothing), an account row in the
 * rail foot that signed out on click, and a name chip in the top bar that only
 * echoed the current page. Identity, settings and sign-out now belong to one
 * trigger with one menu, so a user learns it once for the club panel, the DJ
 * console and the admin panel alike.
 *
 * Contract
 * --------
 *   - The trigger is a labelled button; the menu lists Settings, then Sign out.
 *   - Settings opens the app's account dialog; sign-out is the last row, marked
 *     as a destructive action and pushed to the right edge.
 *   - Sign-out appears twice by design: in the menu for one click, and at the
 *     bottom of the settings dialog, where a user who is already in settings
 *     expects it.
 *   - On touch widths the rail is hidden, so the same component renders as a
 *     compact avatar button in the top bar — still one menu, one behaviour.
 */
import Icon from './components/Icon';
import { Menu, MenuItem, MenuSeparator } from './menu';
import { Modal } from './modal';
import { useTheme } from './theme';
import { useLocale } from './i18n';

export interface AccountIdentity {
  /** One to three characters; the avatar is a circle, never a photo. */
  initials: string;
  /** Who is signed in: "Club owner", "DJ account", "Platform operator". */
  name: string;
  /** Where: club name, workspace, or the signed-in address. */
  meta: string;
}

interface AccountMenuProps {
  identity: AccountIdentity;
  onOpenSettings: () => void;
  onSignOut: () => void;
  /** `rail` is the full row in the rail foot; `compact` is the mobile top-bar button. */
  variant?: 'rail' | 'compact';
}

export function AccountMenu({ identity, onOpenSettings, onSignOut, variant = 'rail' }: AccountMenuProps) {
  const { t } = useLocale();
  const settingsLabel = t('account.settings');
  const signOutLabel = t('account.signOut');

  return (
    <Menu
      label={t('account.menu')}
      placement={variant === 'rail' ? 'top-start' : 'bottom-end'}
      className={variant === 'rail' ? 'panel-account-menu' : 'panel-account-menu-compact'}
      trigger={(props) =>
        variant === 'rail' ? (
          <button {...props} className="panel-account">
            <span className="panel-account-avatar" aria-hidden="true">{identity.initials}</span>
            <span className="panel-account-copy">
              <span className="panel-account-name">{identity.name}</span>
              <span className="panel-account-meta">{identity.meta}</span>
            </span>
            <span className="panel-account-chevron" aria-hidden="true">
              <Icon name="lucide:chevrons-up-down" size={14} />
            </span>
          </button>
        ) : (
          <button {...props} className="panel-icon-button" aria-label={t('account.menu')}>
            <span className="panel-account-avatar panel-account-avatar-sm" aria-hidden="true">{identity.initials}</span>
          </button>
        )
      }
    >
      <div className="panel-menu-identity">
        <span className="panel-account-avatar" aria-hidden="true">{identity.initials}</span>
        <span className="panel-account-copy">
          <span className="panel-account-name">{identity.name}</span>
          <span className="panel-account-meta">{identity.meta}</span>
        </span>
      </div>
      <MenuSeparator />
      <MenuItem label={settingsLabel} icon="lucide:settings-2" onSelect={onOpenSettings} />
      <MenuSeparator />
      <MenuItem label={signOutLabel} icon="lucide:log-out" tone="danger" onSelect={onSignOut} />
    </Menu>
  );
}

import { LANGUAGE_CODES, nativeNameFor } from './i18n';

/**
 * Language choices as a radio list. Used inside the settings dialog, which is
 * the one place the panel's language is chosen — the same dialog in every app.
 * The endonyms come from `LANGUAGES` in `i18n.tsx`, the single source of the
 * six codes the whole product supports.
 */
export function LanguageList({ idPrefix }: { idPrefix: string }) {
  const { language, setLanguage, t } = useLocale();
  const options = LANGUAGE_CODES.map((code) => ({
    code,
    native: nativeNameFor(code),
  }));
  return (
    <fieldset className="panel-language" aria-labelledby={`${idPrefix}-legend`}>
      <legend id={`${idPrefix}-legend`} className="panel-language-legend">{t('account.language')}</legend>
      <p className="panel-language-hint">{t('account.languageHint')}</p>
      <div className="panel-language-options">
        {options.map((option) => {
          const checked = option.code === language;
          return (
            <label key={option.code} className={`panel-language-option ${checked ? 'is-checked' : ''}`.trim()}>
              <input
                type="radio"
                name={`${idPrefix}-language`}
                value={option.code}
                checked={checked}
                onChange={() => setLanguage(option.code)}
                className="panel-language-input"
              />
              <span className="panel-language-native">{option.native}</span>
              {checked ? <Icon name="lucide:check" size={15} /> : null}
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}


export interface AccountFact {
  label: string;
  value: string;
}

/**
 * The optional developer entry of the settings dialog.
 *
 * It is a row, not a second dialog: the panel that ships a developer surface
 * passes the row copy and a handler that opens (and normally closes the dialog
 * first). A panel without such a surface passes nothing and the row does not
 * exist — an account dialog must never advertise a page that is not there.
 */
export interface DeveloperAccessEntry {
  /** One line under the row title. */
  hint: string;
  /** Label of the row's button. */
  actionLabel: string;
  onOpen: () => void;
}

/**
 * Account settings — the one dialog every panel opens from the account menu.
 *
 * Content order follows what a user came for: who they are (read-only facts the
 * API already returned), the language, the theme, then sign-out last so it is
 * never the thing the cursor lands on by accident. A fact the panel could not
 * read shows `Not read yet` instead of a blank or an invented value.
 */
export function AccountSettings({
  open,
  identity,
  facts = [],
  onClose,
  onSignOut,
  developerAccess,
}: {
  open: boolean;
  identity: AccountIdentity;
  facts?: AccountFact[];
  onClose: () => void;
  onSignOut: () => void;
  developerAccess?: DeveloperAccessEntry;
}) {
  const { t } = useLocale();
  const { theme, setTheme } = useTheme();
  const titleId = 'account-settings';

  if (!open) return null;

  return (
    <Modal
      title={t('account.title')}
      description={t('account.description')}
      closeLabel={t('account.close')}
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn btn-danger" onClick={onSignOut}>
            <Icon name="lucide:log-out" size={15} />
            {t('account.signOut')}
          </button>
          <span className="panel-modal-foot-spacer" />
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            {t('account.done')}
          </button>
        </>
      }
    >
      <div className="panel-menu-identity panel-account-summary">
        <span className="panel-account-avatar" aria-hidden="true">{identity.initials}</span>
        <span className="panel-account-copy">
          <span className="panel-account-name">{identity.name}</span>
          <span className="panel-account-meta">{identity.meta}</span>
        </span>
      </div>

      {/* No facts, no list: an empty <dl> still costs a flex gap and reads as a
          missing block. */}
      {facts.length > 0 ? (
        <dl className="panel-account-facts" aria-labelledby={titleId}>
          {facts.map((fact) => (
            <div key={fact.label} className="panel-account-fact">
              <dt>{fact.label}</dt>
              <dd>{fact.value || t('account.notLoaded')}</dd>
            </div>
          ))}
        </dl>
      ) : null}

      <LanguageList idPrefix="account" />

      {developerAccess ? (
        <div className="panel-setting-row">
          <div className="panel-setting-copy">
            <span className="panel-setting-title">{t('account.developer')}</span>
            <p className="panel-setting-hint">{developerAccess.hint}</p>
          </div>
          <button type="button" className="btn btn-secondary" onClick={developerAccess.onOpen}>
            {developerAccess.actionLabel}
          </button>
        </div>
      ) : null}

      <div className="panel-setting-row">
        <div className="panel-setting-copy">
          <span className="panel-setting-title">{t('account.theme')}</span>
          <p className="panel-setting-hint">{t('account.themeHint')}</p>
        </div>
        <div className="theme-toggle" role="group" aria-label={t('account.colourTheme')}>
          <button
            type="button"
            aria-pressed={theme === 'light'}
            aria-label={t('account.themeLight')}
            title={t('account.themeLight')}
            onClick={() => setTheme('light')}
          >
            <Icon name="lucide:sun" size={15} />
          </button>
          <button
            type="button"
            aria-pressed={theme === 'dark'}
            aria-label={t('account.themeDark')}
            title={t('account.themeDark')}
            onClick={() => setTheme('dark')}
          >
            <Icon name="lucide:moon" size={15} />
          </button>
        </div>
      </div>

      <p className="panel-setting-hint">{t('account.signOutHint')}</p>
    </Modal>
  );
}
