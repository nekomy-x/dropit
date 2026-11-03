# DROP IT Guest: Android Studio и повторная проверка 31.10.2026

Проверено Flutter-приложение `apps/mobile-guest`. DJ-приложение и web-панели
в этот прогон не входят. Базовые изменения адаптивности описаны в
[предыдущем отчёте](mobile-guest-2026-10-31.md).

## Среда

- Android Studio Panda 4 / 2025.3.4 запущена с исходным Flutter-проектом.
- Установлены Flutter 97.0.0, Dart 510.0.0 и JetBrains MCP Server 253.31033.23.
- MCP `dropit-android-studio` зарегистрирован в Codex через официальный stdio
  proxy JetBrains. Получение конфигураций и инспекции файлов проверены.
  Глобальное автоматическое разрешение команд (brave mode) не включалось.
- В `.run/` добавлены общие конфигурации `Guest - All tests`,
  `Guest - Responsive UI`, `Guest - Interactions`.
- Исправлена обработка Unicode в `android/local.properties`: Flutter пишет UTF-8,
  а загрузчик Gradle читает Java properties. Пути нормализуются в `\uXXXX`,
  чтобы сборка работала из каталога на томе «Хранилище».
- Android SDK остался на внешнем томе. Для Flutter настроен ASCII alias
  `/Users/nekomy/dropit-android-sdk`. Кэш и wrapper пользовательского Gradle
  перенесены с сохранением содержимого в
  `/Volumes/Хранилище/Dev/mobile-sdk/dropit-studio-user-gradle/`, старые пути
  заменены символическими ссылками. Отдельный QA Gradle home находится в
  `dropit-qa-gradle/` рядом с SDK. Резервные копии настроек IDE сохранены в
  `dropit-studio-backup-20261031/`.

## Исправление оформления заказа

Проверки обнаружили три воспроизводимые ошибки: создание backend-заказа без
способа оплаты, потерю уже созданного заказа при ошибке создания PaymentIntent
и повторную отправку при двух параллельных вызовах.

Теперь проверка способа оплаты предшествует сетевым изменениям, незавершённый
checkout сохраняет ключи идемпотентности и завершённые стадии, а параллельный
вызов получает `checkout_in_progress`. Повтор подтверждения после ошибки сервера
не открывает оплату заново. Окончательный отказ до создания заказа, например
HTTP 404 для удалённого трека, освобождает checkout: можно выбрать другой трек.
Состояние восстановления хранится в памяти процесса;
восстановление после принудительного закрытия приложения этим изменением не решено.

Добавлены шесть HTTP-тестов `test/checkout_recovery_test.dart`: отсутствие способа
оплаты, сбой PaymentIntent, параллельная отправка, неоднозначная ошибка создания
заказа, окончательный отказ и сбой подтверждения. До исправления три исходных проверки падали, после
исправления весь набор проходит. Реальных платежей эти тесты не выполняют.

## Результаты

- `flutter test --reporter expanded`: **220 passed**, включая 192 сценария
  адаптивности Android/iOS, взаимодействия, QR, guest identity и восстановление оплаты.
- `flutter analyze --no-pub`: **No issues found**. Из-за ранее воспроизведённого
  сбоя LSP на Unicode-пути анализ выполнен на обновлённой копии исходников
  `/tmp/dropit-guest-check`.
- Инспекции Android Studio через MCP: **25 Dart-файлов, 0 ошибок**.
- APK собран из исходного проекта после исправления checkout:
  `flutter build apk --debug --target-platform android-arm64` — успешно.
  `output/qa/dropit-guest-arm64-debug.apk`: 155 672 175 байт, engine ABI arm64-v8a,
  целостность ZIP и подпись APK v2 проверены. SHA-256:
  `8a998ecb1a131350ae378d64843b2db4b92e238d5691b433d908f5e5e536e138`.
- `plutil -lint`: Info.plist, entitlements и Xcode project — OK;
  `ruby -c ios/Podfile` — Syntax OK. Это проверка синтаксиса, не сборка iOS.
- Общий `./gradlew lintDebug` и отдельный `./gradlew :app:lintDebug` не завершились:
  модуль зависимости `stripe_android`
  требует недоступный в подключённых репозиториях
  `com.google.android.gms:play-services-tapandpay:18.8.0` через
  `stripe-android-issuing-push-provisioning:1.3.0`. Проверки не отключались,
  успешным общий Lint не считается.

Актуальные логи, JSON инспекций, снимок окна IDE и APK находятся в
`apps/mobile-guest/output/qa/`. На снимке IDE виден начатый тестовый прогон;
итог **220 passed** подтверждается CLI-логом `studio-flutter-test.log`.

## Незавершённая нативная проверка

Android-эмулятор останавливается до готовности ОС, до установки APK. Проверены
старый AVD, свежий профиль, созданный официальным `avdmanager`, разные режимы GPU
и software acceleration. Стабильный Emulator 37.2.13 скачан с сервера Google
отдельно от установленного 37.2.12; контрольная сумма проверена. Зависший старый
процесс qemu остаётся в состоянии `UEs` даже после SIGKILL. Наличие устройства
в `adb devices` не означает работоспособность: команды `getprop sys.boot_completed`
и `wm size` возвращают тайм-аут. Для продолжения нужен отвечающий Android-телефон
с USB-отладкой или восстановление среды эмулятора. Mac автоматически не перезагружался.
После освобождения диска (более 6 ГБ) повторён запуск свежего AVD с 2 ГБ RAM
и двумя CPU: снова зависание QEMU CPU0/CPU1, устройство остаётся `offline`.

Xcode отсутствует, выбран только `/Library/Developer/CommandLineTools`.
Сборка и запуск на iOS Simulator / iPhone не выполнены. Flutter-тесты с
`TargetPlatform.iOS` не заменяют нативный прогон.

Поэтому полное тестирование Android и iOS **не завершено**. На устройствах ещё
нужно проверить системную клавиатуру и Back, safe areas, реальные переходы по
ссылкам, Stripe/Apple Pay, secure storage, возврат из background и сетевые сбои.
Камера, некоторые формы, поддержка и другие демонстрационные функции, отмеченные
в предыдущем отчёте, не стали production-интеграциями от запуска этих тестов.

## Источники по инструментам

- [Установка IDE-плагинов из командной строки](https://www.jetbrains.com/help/idea/install-plugins-from-the-command-line.html).
- [JetBrains MCP Server](https://www.jetbrains.com/help/idea/mcp-server.html).
- [Официальные версии Android Emulator](https://developer.android.com/studio/releases/emulator).
