-- DROP IT — миграция 0012: управление DJ в клубной панели.
--
-- Три поля, которых не хватало странице `/djs`:
--
--   * `note` — рабочий комментарий клуба к DJ ("не ставить в пятницу",
--     "свой контроллер", "ждёт Stripe"). Это НЕ заметка DJ о себе и не
--     видна гостям/DJ-консоли; источник истины — клуб.
--   * `last_seen_at` — присутствие DJ в консоли. Пишется heartbeatem
--     `POST /v1/dj/presence` (30 с, только пока вкладка видима), читается
--     клубом как «в сети / не в сети» + «был(а) N назад». NULL = DJ ни
--     разу не открывал консоль с момента миграции (честное «неизвестно»,
--     а не «офлайн»).
--   * `removed_at` — мягкое удаление из состава. Удалять строку нельзя:
--     на `dj` ссылаются `session` и `split` (история смен и выплат,
--     техкарта), поэтому «удалить» = снять с состава: строка остаётся,
--     из списка скрывается, вход блокируется, восстановление возможно.
--
-- Только ADD COLUMN/INDEX — существующие колонки, UNIQUE(email) и
-- порядок TRUNCATE в фикстурах e2e не меняются.

ALTER TABLE dj ADD COLUMN IF NOT EXISTS note         TEXT;          -- ≤500 символов, валидируется в handler'е
ALTER TABLE dj ADD COLUMN IF NOT EXISTS last_seen_at TIMESTAMPTZ;   -- присутствие в консоли
ALTER TABLE dj ADD COLUMN IF NOT EXISTS removed_at   TIMESTAMPTZ;   -- мягкое удаление из состава клуба

-- Список клуба читается по club_id и почти всегда с фильтром «не удалён».
CREATE INDEX IF NOT EXISTS idx_dj_club_active
    ON dj (club_id, display_name) WHERE removed_at IS NULL;

-- Присутствие джойнится к списку DJ клуба; индекс по времени нужен
-- только если появится глобальный presence-экран (админка).
CREATE INDEX IF NOT EXISTS idx_dj_last_seen
    ON dj (last_seen_at DESC) WHERE last_seen_at IS NOT NULL;
