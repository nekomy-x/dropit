-- DROP IT — миграция 0011: открытая регистрация (Test Mode stand).
-- Контракт: docs/signup-contract.md. Схему клубов/DJ не меняем: `club.status`
-- уже допускает 'active' (его и ставит открытая регистрация), `dj.club_id`
-- уже NULL-able (фрилансер).
--
-- Зачем таблица: лимит «не больше 10 регистраций с одного IP в час».
-- Это тот же учётный паттерн, что `device_token_issue` (ip INET + at):
-- счётчик живёт в БД, а не в памяти процесса, поэтому лимит переживает
-- рестарт и работает при нескольких узлах gateway.
CREATE TABLE signup_attempt (
    id   BIGSERIAL PRIMARY KEY,
    ip   INET NOT NULL,                              -- IP клиента (TRUST_PROXY — как в gateway)
    role TEXT NOT NULL,                              -- 'club' | 'dj' (аудит/разбивка)
    at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_signup_attempt_ip ON signup_attempt (ip, at DESC);
