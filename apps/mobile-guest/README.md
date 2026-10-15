# DROP IT — приложение гостя (Flutter)

Home → QR-скан → поиск трека → условия → оплата → «запрос отправлен» → мои запросы.

## Статус

Экраны 01–05 приложения гостя перенесены **по макету** (pen.dev):
`docs/ui/reference/dropit-design.pen`, спецификация — `docs/ui/guest-flow-spec.md`.
Токены, шрифты и фирменный знак взяты из макета и брендбука, а не подобраны
на глаз.

| Кадр макета | Файл |
|---|---|
| Guest · 01 · Home | `lib/screens/home_screen.dart` |
| Guest · 02 · Scan QR (+ 04c Manual entry) | `lib/screens/scan_screen.dart` |
| Guest · 03 · Search | `lib/screens/search_screen.dart` |
| Guest · 04a · Terms before payment | `lib/screens/terms_screen.dart` |
| Guest · 04 · Payment (+ 04b, 04d) | `lib/screens/payment_screen.dart` |
| Guest · 05 · Request sent | `lib/screens/request_sent_screen.dart` |
| Guest · 06 · My requests | `lib/screens/requests_screen.dart` |

## Дизайн-система

- `lib/theme/tokens.dart` — **единственное** место с цветами, радиусами и
  размерами (`$drop-*` из макета: фон `#0A0C0B`, акцент `#00FF87`,
  поверхности `#141815` / `#1B211D`, текст `#F2F5F3`, muted `#A5AEA9`).
- `lib/theme/app_theme.dart` — `ThemeData` из токенов.
- `lib/widgets/` — общие компоненты: кнопки со всеми состояниями, поля и
  строка комнаты, чипы и фильтры, строки треков и сводки, шапки, шторки,
  обложки, праздничный блок.
- `assets/fonts/` — Anton, Inter (400/500/600/700), Geist Mono (400/500).
- `shaders/brand_wave_mint.frag` — фирменный знак «пиксельная волна»
  (22 полосы, уровень 0,49, амплитуды 0,20/0,12, множители 3,4/−2,1,
  смягчение ±0,025, градиент `#00FF87 → #66FFC2`). Рисуется в
  `lib/widgets/brand_wave_logo.dart` через `ShaderMask`; при
  «уменьшить движение» и в тестах — статичный кадр.

## Запуск

```bash
cd apps/mobile-guest
flutter pub get
flutter test          # виджет-тесты (сеть не нужна)
flutter run           # эмулятор/устройство; API по умолчанию 10.0.2.2:8081
```

Сборка под Android требует ASCII-путей: gradle читает `local.properties` как
ISO-8859-1 и коверкает кириллицу в пути SDK. Обход — ASCII-симлинки на SDK и
каталог сборки (см. `IMPLEMENTATION_STATUS.md`, журнал проверок).

## WS-live (gateway-ws 0.7.0)

`lib/api/ws_client.dart` — guest-канал: подписка
`{"subscribe":"guest","id":"<device>"}`; статусы запросов
(accepted / rejected с причиной DJ / played) приходят без обновления экрана.
Транспорт — web_socket_channel в прод-сборке; каркас деградирует на ручной
refresh, контракт покрыт мок-тестами.

## Заглушки (правило 5 — помечены в коде)

- поиск: демо-каталог до интеграции (B5 — лицензии);
- QR-скан: экран камеры нарисован, поток камеры не подключён — вход через
  шторку ручного кода;
- оплата: карта не уходит провайдеру (hosted-форма Stripe после ADR-004);
  серверный `POST /v1/requests` при этом настоящий;
- «My requests»: демо-список, если API не передан.

Прод-задачи: secure storage вместо SharedPreferences, camera-сканер,
push-уведомления (макеты Guest/DJ уже есть в `.pen`).
