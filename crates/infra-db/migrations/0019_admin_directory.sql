-- DROP IT — миграция 0019: админский каталог пользователей (§7.1).
--
-- Зачем: `/manage/users` в админке показывал список из `localStorage`
-- (`apps/web-admin/src/data/adminMock.ts`). Каталог `GET /v1/admin/users`
-- строится проекцией над четырьмя таблицами (`admin_user`, `club`, `dj`,
-- `guest_account` + `guest_device`), и ему не хватало:
--
--   * `admin_user.display_name` — у клубов есть `name`, у DJ —
--     `display_name`, у гостей — `guest_device.display_name`, а у админа
--     в имени хранился только e-mail. Колонка nullable: у существующих
--     строк имени нет, каталог показывает локальную часть e-mail.
--   * индексов под фильтры каталога (`role`+`status`, `club.status`,
--     `dj(club_id,status)`) и под keyset-пагинацию гостей/присутствия.
--
-- Только ADD COLUMN/INDEX (IF NOT EXISTS), существующие колонки, UNIQUE-
-- ограничения и порядок TRUNCATE в фикстурах e2e не меняются.
--
-- ВНИМАНИЕ: `guest_device` в каталоге используется как источник
-- `last_seen`/`display_name` для аккаунта гостя; отдельной сущности
-- «гость» здесь не заводится (`guest_account` — единственная строка
-- каталога, anonymous-устройства управляются клубом через
-- `GET /v1/admin/guests?club_id=`).

ALTER TABLE admin_user ADD COLUMN IF NOT EXISTS display_name TEXT;

-- Каталог фильтруется по роли и статусу; сортировка — (created_at, id).
CREATE INDEX IF NOT EXISTS idx_admin_user_role_status
    ON admin_user (role, status);
CREATE INDEX IF NOT EXISTS idx_admin_user_created
    ON admin_user (created_at DESC, id DESC);

-- Клуб: фильтр по статусу (pending_verification/active/suspended/blocked).
CREATE INDEX IF NOT EXISTS idx_club_status ON club (status);
CREATE INDEX IF NOT EXISTS idx_club_created ON club (created_at DESC, id DESC);

-- DJ: фильтр «состав клуба» + статус; keyset-пагинация каталога.
CREATE INDEX IF NOT EXISTS idx_dj_club_status ON dj (club_id, status);
CREATE INDEX IF NOT EXISTS idx_dj_created ON dj (created_at DESC, id DESC);

-- Гость: `last_seen` — источник `last_seen_at` в каталоге.
CREATE INDEX IF NOT EXISTS idx_guest_device_last_seen
    ON guest_device (last_seen DESC);
CREATE INDEX IF NOT EXISTS idx_guest_account_created
    ON guest_account (created_at DESC, id DESC);
