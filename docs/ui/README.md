# UI — макет DROP IT и дизайн-система

Источник истины — макет в pen.dev:

- живая ссылка: <https://app.pen.dev/s/hvQJrIKH-PEFV4KfRjc9J5ShcVyaZnoe1hFT8ziFKx4>
- локальная копия: `reference/dropit-design.pen` (структурный JSON, 71 фрейм, 5164 узла)
- обзорный скриншот канваса: `reference/design-canvas.png`
- выгрузка узлов экранов гостя: `reference/guest-flow-nodes.txt`

> Локальная копия нужна потому, что ссылка в инструментах не открывается напрямую
> (hostname резолвится в непубличный IP). Файл скачан из share-сессии целиком.

## Дизайн-токены

Значения — переменные из макета (`$drop-*` и `$dj-*`), плюс брендбук (стр. 04
«Цвет» и 05 «Типографика»). В коде приложения гостя они лежат в
`apps/mobile-guest/lib/theme/tokens.dart` — **это единственное место, где
заданы цвета и размеры**.

| Токен | Значение | Назначение |
|---|---|---|
| `$drop-bg` | `#0A0C0B` | фон «ночь» |
| `$drop-rail` | `#101413` | рельс/нижняя полоса |
| `$drop-surface` | `#141815` | карточки |
| `$drop-surface-2` | `#1B211D` | приподнятые поверхности |
| `$drop-border` | `#FFFFFF17` | hairline 1 px |
| `$drop-tint` | `#00FF8714` | 8 % акцента (бейджи) |
| `$drop-accent` | `#00FF87` | главное действие |
| `$drop-ink` | `#06130C` | текст на акценте |
| `$drop-text` | `#F2F5F3` | основной текст |
| `$drop-muted` | `#A5AEA9` | вторичный текст |
| `$dj-red` | `#FF3B30` | ошибка / отклонение |
| — | `#FFD166` | предупреждение (брендбук) |

Шрифты: **Anton** — заголовки (34 в приложении, 40 — H1 брендбука),
**Inter** — интерфейс (400/500/600/700), **Geist Mono** — данные (ISRC, время,
суммы). Радиусы: 6 / 8 / 10 / 12 / 16 / 18 / 20 / 22 / 24 / 27 (пилюли).

Состояния (DS · 02): default `#00FF87`, pressed `#00CC6A`, disabled `#1B211D`
с текстом muted, loading — «Processing…» + спиннер; destructive `#FF3B3026`
с текстом `#FF3B30`.

Логотип: «пиксельная волна» — 22 полосы, базовый уровень 0,49, амплитуды
0,20 и 0,12, множители времени 3,4 и −2,1, смягчение ±0,025, градиент
`#00FF87 → #66FFC2` слева направо (брендбук, стр. 10). GLSL-шейдер:
`apps/mobile-guest/shaders/brand_wave_mint.frag`.

## Что уже реализовано по макету

Приложение гостя (Flutter, `apps/mobile-guest`) — флоу 01 → 05:

| Кадр макета | Экран в коде |
|---|---|
| Guest · 01 · Home | `lib/screens/home_screen.dart` |
| Guest · 02 · Scan QR | `lib/screens/scan_screen.dart` |
| Guest · 04c · Manual entry | шторка в `lib/screens/scan_screen.dart` |
| Guest · 03 · Search | `lib/screens/search_screen.dart` |
| Guest · 04a · Terms before payment | `lib/screens/terms_screen.dart` |
| Guest · 04 · Payment | `lib/screens/payment_screen.dart` |
| Guest · 04b / 04d · Choose method / Add card | шторки в `lib/screens/payment_screen.dart` |
| Guest · 05 · Request sent | `lib/screens/request_sent_screen.dart` |
| Guest · 06 · My requests | `lib/screens/requests_screen.dart` |

Общие компоненты: `lib/widgets/` — кнопки и состояния, поля, чипы и фильтры,
строки треков, шапки и «комната», шторки, обложки, праздничный блок, знак.

## Чего в коде ещё нет

Фреймы макета, не перенесённые в код: Guest 07–15 (профиль, вход, регистрация,
сброс пароля, Explore, Room · Now playing, Request detail, Help, Identity),
DJ 01–10 (приложение диджея), Club 01–07 (панель клуба), Admin 01–05,
DS 01–02, брендбук 01–12, лендинг и сайт для клубов.

Панели (`apps/web-club`, `apps/web-admin`) сейчас на **других** токенах —
маджента `#c2185b` и янтарь `#d97706` в `tailwind.config.js`, шрифт Inter.
По макету у клуба — тот же неон `#00FF87`, у админа — служебный вариант.
Это первое, что нужно свести к общей палитре.

HTML-прототип консоли диджея (`apps/web-dj/index.html`) написан на **старой**
палитре (`#a6ff00`, Unbounded/Space Mono) — до макета. При переносе DJ-экранов
ориентироваться на `.pen`, а не на этот прототип.
