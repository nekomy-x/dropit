-- DROP IT — миграция 0022: одноразовая ссылка активации клуба.
--
-- Зачем: approve перестаёт задавать пароль за владельца. Клуб создаётся с
-- `password_hash = '!'` (вход по паролю невозможен) и одноразовым
-- invite-токеном; владелец открывает `/club/invite/<token>`, видит данные
-- заявки и задаёт пароль сам. Контракт — `docs/onboarding-contract.md`.
--
-- ПРАВИЛА, которые держит схема:
--   * в БД только `sha256(invite_token)` (`invite_token_hash`); plaintext
--     существует ровно в одном ответе approve/reissue и в письме
--     `club_invite`;
--   * `invite_expires_at` — TTL 7 дней от выпуска. Просроченный, уже
--     использованный и «чужой» токен неразличимы снаружи: все три дают
--     один `404 invite_not_found`;
--   * одноразовость — не флаг, а NULL: accept пишет пароль и зануляет
--     `invite_token_hash`/`invite_expires_at` одним UPDATE, поэтому
--     повторная ссылка не проходит условие `invite_expires_at > now()`;
--   * partial UNIQUE: два клуба не могут делить один токен, но NULL
--     (ещё не приглашённые и уже активированные клубы) в индексе не
--     участвует и не мешает;
--   * CHECK «хеш и TTL живут вместе»: не бывает токена без срока и срока
--     без токена. Иначе проверка одноразовости разъехалась бы на два
--     независимых условия, и NULL-комбинации пришлось бы трактовать
--     отдельно в каждом запросе;
--   * `club_application.club_id` — связь заявки с созданным клубом.
--     Approve — единственное место, где она появляется. Без этой колонки
--     админский список не может показать `invite_state` (искать клуб по
--     e-mail нельзя: адрес можно сменить), а перевыпуск ссылки — найти
--     клуб заявки. ON DELETE SET NULL: удаление клуба не должно ломать
--     историю заявки.
--
-- DDL идемпотентен (IF NOT EXISTS / DROP IF EXISTS): как 0021, применяется
-- и `psql -f`, и sqlx Migrator; повторный прогон не падает на уже
-- созданных объектах.

ALTER TABLE club ADD COLUMN IF NOT EXISTS invite_token_hash TEXT;
ALTER TABLE club ADD COLUMN IF NOT EXISTS invite_expires_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS idx_club_invite_token_hash
    ON club (invite_token_hash) WHERE invite_token_hash IS NOT NULL;

ALTER TABLE club_application ADD COLUMN IF NOT EXISTS club_id UUID
    REFERENCES club(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_club_application_club
    ON club_application (club_id) WHERE club_id IS NOT NULL;

-- Инвариант «токен и срок — одно целое». Проверка на существующих строках
-- проходит: у клубов до этой миграции обе колонки NULL.
ALTER TABLE club DROP CONSTRAINT IF EXISTS club_invite_consistent;
ALTER TABLE club ADD CONSTRAINT club_invite_consistent
    CHECK ((invite_token_hash IS NULL) = (invite_expires_at IS NULL));