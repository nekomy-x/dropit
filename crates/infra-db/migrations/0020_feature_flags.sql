-- DROP IT — миграция 0020: фича-флаги платформы (админка, §7.7).
--
-- Зачем: `/manage/settings` в админ-панели держал переключатели фич в
-- `localStorage` (`apps/web-admin/src/data/adminMock.ts`), то есть в проде
-- «включить фичу» ничего не включало и не выключало. Флаги становятся
-- состоянием платформы: их читает обзор/настройки, меняет только админ с
-- правом записи (`PATCH /v1/admin/feature-flags/:key`), каждая мутация
-- пишется в `audit_log`.
--
-- Контракт (`docs/audit/web-admin-gaps.md` §7.7):
--   * `key` — стабильный строковый код (он же id для UI и audit.entity);
--   * `description` NOT NULL: флаг без объяснения нельзя показать оператору;
--   * `updated_by` — `admin_user.id` последнего изменения (NULL у seed);
--   * набор ключей фиксирован: PATCH умеет менять только засеянные строки
--     (неизвестный ключ → 404), поэтому seed здесь — часть контракта.
--
-- Стартовое состояние: все четыре флага выключены. Это безопасный дефолт:
-- live-платежи, открытая регистрация гостей и самозапись DJ включаются
-- осознанно, а не «сами» после миграции. Флаги без продукта (выплаты/модерация
-- из старого UI) не засеваются: переключатель без эффекта хуже его отсутствия.

CREATE TABLE IF NOT EXISTS feature_flag (
    key         TEXT PRIMARY KEY,
    enabled     BOOLEAN NOT NULL DEFAULT FALSE,
    description TEXT NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID
);

INSERT INTO feature_flag (key, enabled, description) VALUES
    ('payments_live',     FALSE, 'Accept real payments through the live provider instead of the mock'),
    ('guest_signup_open', FALSE, 'Allow guests to create an account with an e-mail address'),
    ('dj_self_signup',    FALSE, 'Allow DJs to register themselves without a club invitation'),
    ('maintenance_banner', FALSE, 'Show the maintenance banner to club staff and DJs')
ON CONFLICT (key) DO NOTHING;