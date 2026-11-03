# Онбординг клуба и DJ по заявке — контракт

Версия: 1.0 (октябрь 2026). Источник правды для backend (`crates/`) и
панелей (`apps/`) на время реализации. После реализации переносится в
`docs/openapi.yaml`.

Заменяет открытую регистрацию как основной путь: клуб оставляет заявку,
админ её рассматривает, и только после одобрения клуб получает одноразовую
ссылку, где задаёт пароль. DJ подключается так же — по приглашению клуба,
которое выдаёт ссылку на активацию.

## Принципы

- **Пароль не задаёт никто, кроме владельца аккаунта.** Ни заявка, ни
  админский `approve` не содержат пароля: заявка его не принимает, approve
  его больше не требует. Пароль задаётся один раз на странице активации.
- **Ссылка — единственный вход в активацию.** Токен 128 бит (32 hex-байта,
  64 символа), в БД только `sha256`, TTL 7 дней, одноразовый. Повторное
  использование и чужая ссылка дают один и тот же `404 invite_not_found`.
- **Данные заявки не теряются и не переспрашиваются.** Страница активации
  показывает название клуба, город и e-mail из заявки; вводится только
  пароль (и его подтверждение).
- **Аккаунт после активации рабочий.** `club.status` становится `active`:
  владение e-mail подтверждено самой ссылкой, одобрение — решением админа.
- **Ошибки — существующий конверт** `ErrorEnvelope {code, message, details}`.
- **Секретов в заявке нет.** Тело `apply` по-прежнему не принимает и не
  хранит паролей.

## Поток

1. Клуб оставляет заявку: `POST /v1/club/onboarding/apply` (публично).
   Источники: компактная форма `/register` в панели и визард `/connect`.
2. Админ открывает очередь `/v1/admin/onboarding-applications` и
   одобряет заявку: `POST …/:id/approve`. Клуб создаётся **без пароля**
   (`password_hash = '!'`), с одноразовым инвайт-токеном. Ответ содержит
   `invite_url`; то же письмо уходит на e-mail заявки, если он есть.
3. Владелец открывает `/club/invite/<token>`, видит данные заявки,
   задаёт пароль: `POST /v1/club/onboarding/invite/:token/accept`.
   Ответ — `TokenPair` (авто-логин), клуб становится `active`.
4. DJ: клуб приглашает DJ (`POST /v1/club/djs/invite`, существует),
   DJ открывает `/dj/invite/<token>`, видит имя и клуб, задаёт пароль:
   `POST /v1/dj/invite/accept` (существует).

## Клуб: заявка

`POST /v1/club/onboarding/apply` — без авторизации (`docs/signup-contract.md`,
`onboarding_routes::apply`). Обязательные поля: `club.name`, `club.contact`,
`venue.name`, `venue.capacity` (1..100000). Блок `qr` обязателен, но
`qr.price_minor` в нём — нет: компактная форма `/register` цену не спрашивает,
тариф клуб задаёт в панели (`/pricing`) после активации. Если цена пришла
(визард `/connect`), она проверяется по границам 100..100000 и попадает в
карточку модерации; без неё в карточке прочерк, а не выдуманное число. Ответ
`201` `{application_id, status: "pending_moderation", submitted_at}`.

## Клуб: approve (изменяется)

`POST /v1/admin/onboarding-applications/:id/approve` — роль Admin, мутация
только для не-readonly.

```json
{
  "email": "owner@club.test",   // optional: по умолчанию email заявки
  "slug": "barrio-latino",      // optional: иначе из названия
  "password": "…"               // deprecated, игнорируется новым UI
}
```

Что делает (одна транзакция):

- создаёт `club` со `status='pending_verification'`;
- `password_hash = '!'`, если `password` не передан (вход по паролю
  невозможен до активации); если передан — старое поведение сохраняется;
- генерирует `invite_token` (64 hex), пишет `sha256` в
  `club.invite_token_hash`, `now() + 7 days` в `club.invite_expires_at`;
- `qr_master_token` по-прежнему создаётся и возвращается (админский вход);
- заявка → `approved`, `reviewed_by/at`, `audit_log`;
- если у заявки/запроса есть e-mail — ставит в очередь письмо
  `club_invite` (best effort; ошибка почты не отменяет approve).

Ответ `200`:

```json
{
  "application_id": "…",
  "club_id": "…",
  "status": "approved",
  "email": "owner@club.test",
  "invite_url": "http://host/club/invite/<token>",
  "invite_expires_at": "2026-11-05T12:00:00Z",
  "mail_queued": true,
  "qr_master_token": "…"
}
```

- `422 validation_error` + `details.field`: `email` (нет ни в теле, ни в
  заявке), `slug`, `password` (если передан, но короче 8).
- `409 already_reviewed`, `409 email_taken`, `409 slug_taken`.
- `404 not_found` — неизвестная заявка.

## Клуб: просмотр ссылки

`GET /v1/club/onboarding/invite/:token` — публично.

`200`:

```json
{
  "club_name": "Barrio Latino",
  "city": "Madrid",
  "email": "owner@club.test",
  "contact": "owner@club.test",
  "slug": "barrio-latino",
  "expires_at": "2026-11-05T12:00:00Z"
}
```

`404 invite_not_found` — неизвестный, использованный или истёкший токен
(один ответ на все случаи).

## Клуб: активация

`POST /v1/club/onboarding/invite/:token/accept` — публично.

```json
{ "password": "new-club-pass" }
```

- `200` → `TokenPair {access_token, refresh_token, token_type, expires_in}`
  плюс `club: {id, name, slug, email}`.
- Пароль — Argon2id (`auth::hash_password`), 8..128 символов.
- `club.password_hash` ← хеш, `invite_token_hash/invite_expires_at` ← NULL,
  `status='active'`; `audit_log` `club.invite.accept`.
- `404 invite_not_found`; `422 validation_failed` с `details.field=password`.
- Повторное использование ссылки — `404`.

## Клуб: админский список (дополняется)

`GET /v1/admin/onboarding-applications?status=&limit=&cursor=` и
`GET …/:id` добавляют в каждую строку:

```json
{
  "club_id": "…",              // null, пока заявка не одобрена
  "invite_state": "pending"    // none | pending | accepted | expired
}
```

`invite_state` выводится из клуба:

- `club_id = NULL` → `none`;
- `invite_token_hash IS NULL` → `accepted`;
- `invite_expires_at > now()` → `pending`;
- иначе → `expired`.

Связь «заявка → созданный клуб» хранится в `club_application.club_id`
(миграция 0022): approve проставляет её в той же транзакции, что и создание
клуба. Искать клуб по e-mail нельзя — адрес можно сменить.

## Клуб: перевыпуск ссылки

`POST /v1/admin/onboarding-applications/:id/invite` — роль Admin, мутация.

- Только для `approved`-заявки с клубом, который ещё не активирован:
  новый токен, новый TTL, письмо `club_invite` (best effort).
- `200` → `{invite_url, invite_expires_at, email, mail_queued}`.
- `409 not_approved` — заявка не одобрена;
- `409 invite_already_accepted` — клуб уже активирован
  (`invite_token_hash IS NULL`, то есть ссылку уже приняли);
- `404 not_found`.

`mail_queued` — правда «строка письма `club_invite` реально в очереди»:
`true`, только если шаблон отрендерился и письмо записалось в `mail_message`.

## DJ: приглашение (дополняется)

`GET /v1/dj/invite/:token` — публично.

`200`:

```json
{
  "display_name": "DJ Noche",
  "club_name": "Barrio Latino",
  "expires_at": "2026-11-05T12:00:00Z"
}
```

`404 invite_not_found` — неизвестный/истёкший/принятый токен.

`POST /v1/dj/invite/accept {invite_token, password}` — существует,
контракт не меняется.

Письмо `dj_invite` должно ссылаться на маршрут панели
`{MAIL_APP_BASE_URL}/dj/invite/<token>` (path-параметр). Старая форма
`/dj/invite?token=…` поддерживается страницей как fallback.

## Почта

Новый шаблон `TemplateKey::ClubInvite` (`club_invite`):

- subject: `Приглашение в {{club_name}}`;
- vars: `club_name`, `invite_url`, `expires_at`;
- `idempotency_key = club_invite:<club_id>:<token prefix>`, aggregate
  `club`.

## Экранные требования

- `/register` (панель клуба): **заявка**, не регистрация. Компактная форма
  в один столбец — название клуба, город, e-mail, вместимость, цена
  запроса; одна кнопка «Отправить заявку»; после отправки — короткое
  состояние «Заявка отправлена» с указанием e-mail. Никаких абзацев про
  «открытую регистрацию».
- `/club/invite/:token`: заголовок «Завершите регистрацию», строки
  «Клуб / Город / E-mail» из заявки только для чтения, поля «Пароль» и
  «Повторите пароль», кнопка «Активировать». Ошибки — конвертом API.
- `/dj/invite/:token`: имя DJ и клуб из заявки, только пароль.
- `/login`, `/dj/login`: два поля и одна кнопка; ссылки — «Забыли пароль»
  и «Заявка на подключение»; без вступительных абзацев и подписей под
  формой.
- Все экраны: только токены `--c-*` / `panel-*`, оба темы, мобильная
  ширина, фокус и контраст по `docs/ui/PANEL-DESIGN.md`.

## Совместимость

- `POST /v1/auth/club/register` и `/v1/auth/dj/register` остаются в коде и
  под флагом `DROPIT_OPEN_SIGNUP` (демо/тесты), но **UI их больше не
  вызывает**. Флаг по умолчанию выключен на PROD-стенде.
- `POST /v1/admin/onboarding-applications/:id/approve` без `password`
  совместим с новым потоком; с `password` — сохраняет прежнее поведение
  для старых клиентов и e2e.
- `docs/signup-contract.md` помечается как legacy-путь Test Mode.
