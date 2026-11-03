-- DROP IT — миграция 0018: heartbeat фоновых воркеров (статус-страница).
--
-- Зачем: публичная `/status` должна честно отвечать «воркеры работают»,
-- а не «процесс когда-то запускался». Воркер-супервизор пишет строку
-- каждые 15 секунд; статус-страница читает возраст последней записи:
--   < 45 с  — ok;  < 180 с — degraded;  иначе / нет строки — down.
--
-- Одна строка на роль воркера: сейчас `dropit-workers`, в будущем можно
-- добавить отдельные роли без изменения схемы.

CREATE TABLE worker_heartbeat (
    worker       TEXT PRIMARY KEY,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    detail       JSONB
);

INSERT INTO worker_heartbeat (worker, last_seen_at)
VALUES ('dropit-workers', now() - interval '1 day')
ON CONFLICT (worker) DO NOTHING;
