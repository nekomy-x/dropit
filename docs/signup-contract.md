# Открытая регистрация (Test Mode stand) — контракт

> **Legacy / demo-only.** Это не основной путь подключения. Клуб оставляет
> заявку (`POST /v1/club/onboarding/apply`), админ её одобряет, и владелец
> задаёт пароль по одноразовой ссылке — см. `docs/onboarding-contract.md`.
> Эндпоинты открытой регистрации **остаются в коде** под флагом
> `DROPIT_OPEN_SIGNUP` ради демо и тестов, но UI панелей их больше не
> вызывает. На PROD-стенде флаг по умолчанию выключен (`403
> signup_disabled`).

Версия: 0.2 (реализовано, см. `docs/openapi.yaml`). Действует для стенда `http://150.241.68.57` (Test Mode).
Единый источник правды для backend (crates) и панелей (apps) на время
реализации. После реализации контракт переносится в `docs/openapi.yaml`.

## Принципы

- Регистрация **открытая**: без подтверждения e-mail, без приглашения,
  без 2FA. Включена только при `DROPIT_OPEN_SIGNUP=1`; при выключенном
  флаге эндпоинты отвечают `403 signup_disabled`.
- Аккаунт сразу рабочий (в Test Mode): клуб создаётся `status='active'`
  (а не `pending_verification`), DJ — `status='active'` с `club_id=NULL`
  либо привязкой к клубу по `club_slug`.
- Ответ — сразу пара токенов (авто-логин), чтобы UI «в один клик»
  не делал второй запрос.
- Пароли — Argon2id, как в логине (переиспользовать `auth::hash_password`).
- Ошибки — существующий `ErrorEnvelope {code, message, details}`.

## `POST /v1/auth/club/register`

```json
{
  "name": "Barrio Latino",
  "email": "owner@barrio.test",
  "password": "demo-club-pass",
  "city": "Madrid",            // optional
  "country": "ES"              // optional, ISO-3166 alpha-2
}
```

- `201` → `TokenPair {access_token, refresh_token, token_type, expires_in}`
  (тот же формат, что у логина) + `club: {id, name, slug, email}` в теле
  (доп. поле, клиенты могут игнорировать).
- `400 validation_failed` — пустое имя, кривой e-mail, пароль короче 8.
- `409 email_taken` — e-mail уже занят (citext, регистр не важен).
- `429 rate_limited` — больше `DROPIT_SIGNUP_RATE_PER_HOUR` успешных
  регистраций с одного IP за час (по умолчанию 10; на стенде — 100).
- `403 signup_disabled` — флаг не включён.

Поведение:
- `slug` генерируется из `name` (транслит/латиница → `[a-z0-9-]`),
  при коллизии — суффикс `-2`, `-3`; `UNIQUE(slug)` гарантируется вставкой
  с ретраем.
- `status='active'` при открытой регистрации (в тестовом режиме клуб
  не ждёт верификации админом).
- Пишется `club_created` в `audit`/`activity`, если такой репозиторий есть;
  иначе просто `tracing::info!`.

## `POST /v1/auth/dj/register`

```json
{
  "display_name": "DJ Noche",
  "email": "noche@dj.test",
  "password": "demo-dj-pass",
  "club_slug": "barrio-latino" // optional
}
```

- `201` → `TokenPair` + `dj: {id, display_name, email, club_id}`.
- Без `club_slug`: `club_id = NULL` (фрилансер) — DJ-консоль увидит только
  те сессии, к которым привязан; в Test Mode допустимо.
- С `club_slug`: привязка к клубу; неизвестный slug → `404 club_not_found`.
- Те же коды ошибок, `409 email_taken` по `dj.email`.

## Ограничения

- Админ **не** регистрируется публично: `POST /v1/auth/admin/register`
  отвечает `403 forbidden`. Админ остаётся только из seed/админки.
- Гостю регистрация не нужна (device-token уже есть).

## UI-требования («просто по кликам»)

- `/register` в панели клуба: первый экран — выбор роли (Клуб / DJ).
- Кнопка **«Создать аккаунт в один клик»**: генерирует имя, e-mail
  (`demo+<rand>@<role>.test`) и пароль, отправляет форму, логинит и
  ведёт на рабочий экран. Ничего вводить не нужно.
- Рядом — обычная форма (имя, e-mail, пароль) для тех, кто хочет свои
  данные; кнопка «Сгенерировать» подставляет значения в поля.
- Баннер Test Mode: «Тестовый режим · платежи мок · данные демо».

## Совместимость

- Панели ходят на тот же origin (`/v1/...`), база API — `location.origin`.
- Никаких токенов в `localStorage` (правило проекта): токены в памяти.
- `/register` в панели клуба теперь отправляет **заявку**
  (`POST /v1/club/onboarding/apply`), а не регистрацию; `/login` не
  содержит ссылок на открытую регистрацию как основной путь. Этот контракт
  остаётся только для Test Mode и старых e2e.
