-- DROP IT — миграция 0016: режим эксплуатации стенда (`runtime_mode`).
--
-- Зачем колонка: одна и та же сборка панелей стоит на тестовом стенде и в
-- проде, поэтому «какой это режим» (test / maintenance / normal) определяет
-- сервер, а не сборка. Владелец задаёт режим вручную в админ-панели
-- (`PATCH /v1/admin/platform-settings`), а публичный `GET /v1/status` отдаёт
-- уже разрешённое значение для нижнего статус-бара.
--
-- `auto` — ручного решения нет: `/v1/status` выводит режим из фактов стенда
-- (открытая регистрация или mock-платежи → test) либо из env
-- `DROPIT_MAINTENANCE` (→ maintenance). CHECK повторяет enum контракта:
-- невалидное значение не должно попасть в БД в обход API.
ALTER TABLE platform_settings
    ADD COLUMN runtime_mode TEXT NOT NULL DEFAULT 'auto'
    CHECK (runtime_mode IN ('auto', 'test', 'maintenance', 'normal'));