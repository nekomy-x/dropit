# DROP IT — приложение гостя (Flutter)

Полный перенос макета pen.dev **1:1**: все 24 кадра гостевого приложения
(`Guest · 01…15` + шторки 04b/04c/04d + neon-ticket). Сеть нужна для реальных
запросов и оплаты; без сети приложение остаётся рабочим на встроенных данных
(демо-режим), поэтому APK запускается и показывает весь флоу в любом окружении.

## Экраны и файлы

| Кадр макета | Файл |
|---|---|
| Guest · 01 · Home (Listen) | `lib/screens/home_screen.dart` |
| Guest · 02 · Scan QR | `lib/screens/scan_screen.dart` |
| Guest · 03 · Search | `lib/screens/search_screen.dart` |
| Guest · 04a · Terms before payment | `lib/screens/terms_screen.dart` |
| Guest · 04 · Payment (+ 04b, 04d) | `lib/screens/payment_screen.dart` |
| Guest · 05 · Request sent (+ neon-ticket) | `lib/screens/request_sent_screen.dart` |
| Guest · 06 · My requests (All/Upcoming/Played/Declined) | `lib/screens/requests_screen.dart` |
| Guest · 07 · Profile / 07b · Profile · Guest | `lib/screens/profile_screen.dart` |
| Guest · 08 · Sign in / 09 · Create account / 10 · Reset | `lib/screens/auth_screens.dart` |
| Guest · 11 · Explore | `lib/screens/explore_screen.dart` |
| Guest · 12 · Room · Now playing | `lib/screens/room_screen.dart` |
| Guest · 13 · Request detail | `lib/screens/request_detail_screen.dart` |
| Guest · 14 · Help & support | `lib/screens/help_screen.dart` |
| Guest · 15 · Identity | `lib/screens/identity_screen.dart` |

Описание каждой кнопки (вид + действие) — **`docs/ui/guest-buttons.md`**.

## Архитектура

- `lib/main.dart` — `AppStateScope` над `MaterialApp` (состояние доступно всем
  маршрутам), корневой таб-бар (Listen · Explore · Requests · Profile) и
  маршруты флоу (`ScanRoute` → `SearchRoute` → `TermsRoute` → `PaymentRoute` →
  `RequestSentRoute`, `RoomRoute`, `RequestDetailRoute`, auth/help/identity).
- `lib/state/app_state.dart` — `ChangeNotifier`: сессия комнаты, выбранный
  трек, запросы, авторизация, способы оплаты; `probeBackend()` проверяет
  API и включает демо-режим при недоступности.
- `lib/data/` — модели (`models.dart`) и демо-данные 1:1 из макета
  (`demo_data.dart`: Kater Blau, DJ Markus, Move, 5 запросов со статусами, FAQ…).
- `lib/api/api_client.dart` — контракты бэкенда (`docs/openapi.yaml`):
  device-token, join, create request, my requests.

## Дизайн-система

- `lib/theme/tokens.dart` — **единственное** место цветов/радиусов/размеров
  (`$drop-*` из макета + статусы, шторки, формы).
- `lib/theme/app_theme.dart` — `ThemeData` из токенов.
- `lib/widgets/ui.dart` — общие компоненты: таб-бар, бренд-локуп, карточка
  комнаты, кнопки, чипсы, поля и строки настроек.
- `lib/widgets/receipt_ticket.dart` — перфорированный neon-чек.
- `assets/fonts/` — Inter (400/500/600/700), Geist Mono, plus legacy display assets.
- `assets/images/` — изображения макета (hero, фото заведений, обложки,
  карта, сцена сканера) — встроены, работают без сети.
- `shaders/brand_wave_mint.frag` — фирменный знак «пиксельная волна» в фирменной зелёной палитре DROP IT.

## Запуск и сборка

```bash
cd apps/mobile-guest
flutter pub get
flutter test                 # экраны, адаптивность и сценарии без сети
flutter run                  # эмулятор/устройство
flutter test --update-goldens tool/capture_layouts.dart # снимки в output/layouts

# APK для телефона:
flutter build apk --release
# (если путь SDK содержит не-ASCII — см. раздел ниже)
```

### Обход не-ASCII путей (этот репозиторий)

Gradle читает `local.properties` как ISO-8859-1 и коверкает кириллицу
(`/Volumes/Хранилище/...`). Обход: ASCII-симлинк на Flutter SDK и сборка через
gradlew с ASCII-путём в `android/local.properties`:

```bash
ln -sf "/Volumes/Хранилище/Dev/mobile-sdk/flutter" ~/flutter-sdk
export PATH=~/flutter-sdk/bin:$PATH
flutter build apk --release        # перезапишет local.properties
printf 'sdk.dir=%s\nflutter.sdk=/Users/nekomy/flutter-sdk\nflutter.buildMode=release\n' \
  "/Volumes/Хранилище/Dev/mobile-sdk/android-sdk" > android/local.properties
(cd android && ./gradlew assembleRelease)
# → build/app/outputs/flutter-apk/app-release.apk
```

## Режимы данных

- **Сеть доступна:** `probeBackend()` получает device-token, запросы и оплата
  уходят в бэкенд (`POST /v1/requests` и т.д.).
- **Сети нет / бэкенд недоступен:** приложение использует демо-данные;
  отправка демо-запроса не выполняет оплату и явно помечается «Not charged · demo».

## Демо-поведение (что работает в APK)

- QR-скан: камера-сцена из макета; «From photos» эмулирует распознавание QR,
  ручной ввод кода — шторка 04c.
- Поиск: фильтр по названию/исполнителю, жанровые чипы, превью с прогрессом.
- Оплата: Stripe PaymentSheet для выбора метода и ввода карты; состояние
  request/payment показывается отдельно по ответу backend.
- Запросы: создание нового запроса, фильтры All/Upcoming/Played/Declined,
  карточка причины отказа, детальный таймлайн статусов.
- Профиль: статистика, разделы, вход/регистрация/сброс пароля (валидация),
  редактирование деталей, любимые жанры, help с FAQ и чатом поддержки.

## Известные ограничения

- Поток камеры и image-picker не подключены (нет пакетов) — «From photos»
  эмулирует успешное распознавание.
- Для реальной оплаты нужны Stripe publishable key и backend с настроенным
  Stripe provider. Без ключа checkout отклоняется до создания request.
- При закрытии PaymentSheet или сетевой ошибке после создания PaymentIntent
  backend не предоставляет отмену intent или выдачу исходного `client_secret`;
  может остаться pending request/payment, который требует серверного recovery.
- Android и iOS используют URL scheme `dropit` для возврата из Stripe.
  Нативный проект iOS находится в `ios/`: минимум iOS 15, CocoaPods,
  keychain entitlement для secure storage. Перед запуском на устройстве
  нужно выбрать свою signing team в Xcode. Apple Pay merchant ID и
  capabilities пока не настроены; оплата картой использует PaymentSheet.
  Нативная сборка iOS требует Xcode и установленный iOS Simulator.
- Push-уведомления и WS-live статусы — прод-задача (каркас API готов).

## Проверка адаптивности

`flutter test` проверяет 16 экранов в режимах Android и iOS: 320×568,
390×844, 430×932, 844×390, увеличенный текст (150%) и клавиатура.
Проверяется первый экран и содержимое после прокрутки, отсутствие
переполнений и пересечений нижней навигации с системной областью.
Отдельные сценарии проверяют ручной ввод кода, шторки, поиск с запоздавшим
ответом, показ пароля и демонстрационное оформление без списания.
HTTP-тесты оформления проверяют повтор после сетевой ошибки, сохранение
ключей идемпотентности и защиту от параллельных отправок. Незавершённый checkout
сохраняется в памяти до успешного подтверждения; после перезапуска приложения
для его восстановления всё ещё нужна поддержка backend.

В Android Studio доступны общие конфигурации `Guest - All tests`,
`Guest - Responsive UI` и `Guest - Interactions` из `.run/`.
Результаты прогона и ограничения нативной проверки приведены в
[отчёте Android Studio](../../docs/audit/mobile-guest-android-studio-2026-10-31.md).

`flutter test --update-goldens tool/capture_layouts.dart` сохраняет 20
снимков в `output/layouts/`. Это снимки Flutter-виджетов с заданными
параметрами платформы: они не подтверждают нативную сборку, клавиатуру ОС,
работу платёжного SDK или запуск на реальном телефоне.
