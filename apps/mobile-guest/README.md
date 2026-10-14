# DROP IT — приложение гостя (Flutter)

QR → поиск трека → экран условий → запрос → статус.

## Статус

Каркас F-UI (см. `IMPLEMENTATION_STATUS.md`): структура экранов, тема
(чёрный/неон — dizayn-zadanie §5), API-клиент по контрактам
`docs/openapi.yaml` (F1–F5). Widget-тесты покрывают флоу скана,
динамическую цену и обязательный экран условий.

## Запуск

```bash
cd apps/mobile-guest
flutter pub get
flutter test          # виджет-тесты (не требуют API)
flutter run           # эмулятор/устройство; API по умолчанию 127.0.0.1:8080
```

## Структура

```
lib/
  main.dart                  # тема (токены), корневой флоу
  api/api_client.dart        # device-token, join, requests, ws-ticket
  screens/
    scan_screen.dart         # QR-скан (B5: камера-сканер — при лицензиях)
    session_screen.dart      # клуб/DJ/цена (динамическая, §0.1)
    search_screen.dart       # поиск (ЗАГЛУШКА до B5, правило 5)
    conditions_screen.dart   # ОБЯЗАТЕЛЬНЫЙ экран условий (§0.2)
    requests_screen.dart     # статусы запросов
```

## WS-live (gateway-ws 0.7.0)

`lib/api/ws_client.dart` — guest-канал: подписка
`{"subscribe":"guest","id":"<device>"}`; статусы запросов
(accepted / rejected с причиной DJ / played) приходят без
обновления экрана. Транспорт — web_socket_channel в прод-сборке;
каркас деградирует на ручной refresh, контракт покрыт мок-тестами.

## Заглушки (правило 5 — помечены в коде)

- поиск: демо-список до интеграции каталога (B5 — лицензии);
- QR-скан: ручной ввод токена до подключения камеры;
- requests: демо-данные до integration-тестов с живым API;
- оплата: F5 Test Mode на бэкенде (мок-провайдер) — Stripe SDK
  подключается после ADR-004/Q11–Q15.

Прод-токены: secure storage вместо SharedPreferences, camera-сканер,
push (после первой оплаты — техкарта §3).
