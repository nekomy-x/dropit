import 'package:flutter/material.dart';

/// Дизайн-токены DROP IT — значения взяты напрямую из макета (pen.dev),
/// файл `docs/ui/reference/dropit-design.pen`, набор переменных `$drop-*`
/// плюс брендбук (страницы 04 «Цвет» и 05 «Типографика»).
///
/// Ничего не «на глаз»: каждый hex и каждый размер имеют источник в макете.
/// Меняя токен здесь, вы меняете весь интерфейс — больше нигде цвета и
/// размеры не хардкодятся.
class DropTokens {
  DropTokens._();

  // ─── Палитра ───────────────────────────────────────────────────────────
  // Graphite surfaces shared with the web panels.
  static const bg = Color(0xFF000000);
  static const rail = Color(0xFF080808);
  static const surface = Color(0xFF191919);
  static const surface2 = Color(0xFF242424);
  static const input = Color(0xFF242424);
  // $drop-border — 1px hairline вокруг поверхностей.
  static const border = Color(0x17FFFFFF);
  // Мягкий разделитель внутри списков.
  static const borderSoft = Color(0x14FFFFFF);
  // Разделитель строк «комната/сессия».
  static const divider = Color(0x20FFFFFF);
  // DROP IT green tint for active controls and selected rows.
  static const tint = Color(0x1400FF87);
  static const tintStrong = Color(0x2600FF87);
  static const tintFaint = Color(0x0D00FF87);

  // Primary action accent.
  static const accent = Color(0xFF00FF87);
  // Нажатое состояние кнопки (DS · 02, pressed).
  static const accentPressed = Color(0xFF00D873);
  static const accentMint = Color(0xFF7CFFC0);

  // Текст на акценте — в экранах макета используется этот тёмный тон.
  static const onAccent = Color(0xFF0A0A0A);
  static const ink = Color(0xFF0A0A0A);

  // $drop-text — основной текст.
  static const text = Color(0xFFF2F5F3);
  static const textSoft = Color(0xFFE5E7E6);
  static const textTabActive = Color(0xFFF0F4F1);
  // $drop-muted — вторичный текст.
  static const muted = Color(0xFFA5AEA9);
  static const muted2 = Color(0xFF9A9F9C);
  static const muted3 = Color(0xFF929A95);
  static const muted4 = Color(0xFFD8E3DC);
  static const caption = Color(0xFFDEE5E1);
  static const chipIdle = Color(0xFFCED4D0);
  static const hint = Color(0xFFBAC5BF);
  // Невыбранный контрол (радио, рамки).
  static const inactive = Color(0xFF758079);
  // Обводка таблеток поверх камеры.
  static const ghostStroke = Color(0x26FFFFFF);
  static const ghostFill = Color(0x12FFFFFF);
  static const scrim = Color(0xB3000000);

  // $dj-red / ошибки и отклонения.
  static const red = Color(0xFFFF3B30);
  static const redSoft = Color(0x14FF3B30);
  static const live = Color(0xFF34D399);
  static const liveTint = Color(0x1A34D399);
  // Предупреждение (брендбук, стр. 04).
  static const amber = Color(0xFFFFD166);

  // Служебные поверхности шторок (guest 04b/04d).
  static const sheet = Color(0xFF191919);
  static const sheetRow = Color(0xFF242424);
  static const handle = Color(0xFF606060);
  // Карточка причины отказа (guest 06d).
  static const declineCard = Color(0xFF251C1E);
  static const declineTitle = Color(0xFFFFB1B5);
  static const declineBody = Color(0xFFE3CCCF);
  // Логотип Visa в бейджах оплаты (guest 04/04b/04d).
  static const visaBlue = Color(0xFF1434CB);
  // Невыбранная галочка в шторке способов оплаты.
  static const inactiveCheck = Color(0xFF777F79);
  // Конфетти и искры на экране «запрос отправлен» (guest 05).
  static const sparkle = Color(0xFFBC9AFF);
  static const sparkleSoft = Color(0xFFD8C5FF);
  static const sparklePale = Color(0xFFE6FFF1);

  // ─── Статусы запросов (guest 06·06b·06c·06d, 13) ─────────────────────
  static const statusQueue = Color(0xFF00FF87); // в очереди
  static const statusAwait = Color(0xFFF4CD7A); // ожидает DJ
  static const statusDeclined = Color(0xFFFF969B); // отклонено
  static const statusPlayed = Color(0xFFA5AEA9); // сыграно (muted)

  // ─── Чипы фильтров ───────────────────────────────────────────────────
  static const chipIdleBg = Color(0xFF242424);
  static const chipIdleText = Color(0xFFBDC5BF);

  // ─── Тёмные tint-варианты акцента (бейджи, выбранные строки) ─────────
  static const tint14 = Color(0x1400FF87); // active badge
  static const tint1A = Color(0x1A00FF87); // active room
  static const tint1F = Color(0x1F00FF87); // progress rail
  static const tintStrong2 = Color(0x2600FF87);

  // ─── Прочее из макета ────────────────────────────────────────────────
  static const railTrack = Color(0xFF343434); // рельс аудио-прогресса
  static const eqBarIdle = Color(0x2FFFFFFF); // спящие полосы эквалайзера
  static const ghostFill12 = Color(0x12FFFFFF); // круглые кнопки поверх камеры
  static const scanHint = Color(0xFFD8E3DC); // подсказки на камере
  static const scanHintBright = Color(0xFFE4EBE7);
  static const scanHintDim = Color(0xFFBAC5BF);
  static const scanHintFaint = Color(0xFFBECAC3);
  static const qrHint = Color(0xFF0A3D25); // подпись на зелёной плитке QR
  static const featuredMeta = Color(0xFFCDD5D0); // мета featured-карточки
  static const statLabel = Color(0xFFD2DAD5); // подписи статистики профиля
  static const divider26 = Color(0x26FFFFFF); // разделитель «or with email»
  static const dropLine = Color(0x1FFFFFFF); // тонкие линии (шаги, перфорация)
  static const tabInactive = Color(0xFF929A95);
  static const tabInactiveSoft = Color(0xFF919994);

  // ─── Радиусы ───────────────────────────────────────────────────────────
  static const rXs = 6.0; // фото заведения, мелкие значки
  static const rSm = 8.0; // обложка трека
  static const rInput = 10.0; // поля формы карты
  static const rMd = 12.0; // поиск, кнопки-контролы
  static const rLg = 16.0; // выбранная карта, бейдж
  static const rCard = 18.0; // карточки запросов
  static const rHero = 20.0; // герой-блоки
  static const rTile = 22.0; // плитки действий, таб-бар
  static const rSheet = 24.0; // верхние углы шторок
  static const rPill = 27.0; // главные кнопки (высота 54)

  // ─── Размеры элементов ─────────────────────────────────────────────────
  static const buttonHeight = 54.0;
  static const buttonHeightCompact = 52.0;
  static const searchHeight = 54.0;
  static const fieldHeight = 48.0;
  static const artwork = 50.0;
  static const tabBarHeight = 64.0;
  static const screenPadding = 20.0;
  static const screenTop = 24.0;
  static const gap = 24.0;

  // ─── Градиенты ─────────────────────────────────────────────────────────
  static const heroOverlay = LinearGradient(
    begin: Alignment.bottomCenter,
    end: Alignment.topCenter,
    colors: [Color(0xD9090909), Color(0x05090909)],
  );
  static const mapLabelOverlay = LinearGradient(
    begin: Alignment.centerLeft,
    end: Alignment.centerRight,
    colors: [Color(0x00101016), Color(0xE6101016)],
  );
  static const celebrationGlow = RadialGradient(
    colors: [Color(0x3800FF87), Color(0x0000FF87)],
  );

  // Brand gradient: #00FF87 → #7CFFC0 слева направо.
  static const brandGradient = LinearGradient(
    begin: Alignment.centerLeft,
    end: Alignment.centerRight,
    colors: [accent, accentMint],
  );
}

/// Шкала типографики. Источник — брендбук, стр. 05: базовые уровни
/// 40 / 28 / 20 / 16 / 14 и правило «Inter — заголовки и текст,
/// Geist Mono — данные».
class DropType {
  DropType._();

  static const _head = 'Inter';
  static const _body = 'Inter';
  static const mono = 'GeistMono';

  /// Крупный заголовок экрана (Inter 34, интерлиньяж 1.08).
  /// во всех экранах приложения гостя.
  static const screenTitle = TextStyle(
    fontFamily: _head,
    fontSize: 34,
    height: 1.08,
    fontWeight: FontWeight.w400,
    color: DropTokens.text,
  );

  /// Заголовок шторки (Inter 28).
  static const sheetTitle = TextStyle(
    fontFamily: _head,
    fontSize: 28,
    fontWeight: FontWeight.w400,
    color: DropTokens.text,
  );

  /// H1 (Inter 40) — витринные заголовки.
  static const hero = TextStyle(
    fontFamily: _head,
    fontSize: 40,
    height: 1.08,
    fontWeight: FontWeight.w400,
    color: DropTokens.text,
  );

  /// Крупнейший заголовок экрана «Now playing» (Inter 44, lh 0.95).
  static const nowPlayingTitle = TextStyle(
    fontFamily: _head,
    fontSize: 44,
    height: 0.95,
    fontWeight: FontWeight.w400,
    color: DropTokens.text,
  );

  /// Название featured-комнаты на карточке Explore (Inter 24).
  static const featuredRoom = TextStyle(
    fontFamily: _head,
    fontSize: 24,
    height: 1.0,
    fontWeight: FontWeight.w400,
    color: DropTokens.text,
  );

  /// Название трека в «Request detail» (Inter 22).
  static const requestTitle = TextStyle(
    fontFamily: _head,
    fontSize: 22,
    height: 1.08,
    fontWeight: FontWeight.w400,
    color: DropTokens.text,
  );

  /// Значения статистики профиля (Inter 30 SemiBold).
  static const statValue = TextStyle(
    fontFamily: _body,
    fontSize: 30,
    fontWeight: FontWeight.w600,
    color: DropTokens.text,
  );

  /// Название комнаты в шапке Room (Inter 17 SemiBold).
  static const roomName = TextStyle(
    fontFamily: _body,
    fontSize: 17,
    fontWeight: FontWeight.w600,
    color: DropTokens.text,
  );

  /// Номер позиции в очереди (Inter 12 SemiBold).
  static const stepPosition = TextStyle(
    fontFamily: _head,
    fontSize: 12,
    fontWeight: FontWeight.w600,
    color: DropTokens.muted,
  );

  /// H3 (Inter 18 Bold) — заголовки блоков («Set the mood»).
  static const sectionTitle = TextStyle(
    fontFamily: _body,
    fontSize: 18,
    fontWeight: FontWeight.w700,
    color: DropTokens.text,
  );

  /// Названия комнат/карточек (Inter 16 SemiBold).
  static const title = TextStyle(
    fontFamily: _body,
    fontSize: 16,
    fontWeight: FontWeight.w600,
    color: DropTokens.text,
  );

  /// Навигация/название экрана в шапке (Inter 15 SemiBold).
  static const navTitle = TextStyle(
    fontFamily: _body,
    fontSize: 15,
    fontWeight: FontWeight.w600,
    color: DropTokens.text,
  );

  /// Названия треков (Inter 14 SemiBold).
  static const trackTitle = TextStyle(
    fontFamily: _body,
    fontSize: 14,
    fontWeight: FontWeight.w600,
    color: DropTokens.text,
  );

  /// Основной текст (Inter 14, интерлиньяж 1.5).
  static const body = TextStyle(
    fontFamily: _body,
    fontSize: 14,
    height: 1.5,
    color: DropTokens.text,
  );

  /// Компактный текст (Inter 13).
  static const bodySm = TextStyle(
    fontFamily: _body,
    fontSize: 13,
    height: 1.5,
    color: DropTokens.text,
  );

  /// Подписи (Inter 12).
  static const caption = TextStyle(
    fontFamily: _body,
    fontSize: 12,
    color: DropTokens.muted,
  );

  /// Метаданные трека (Inter 10-11).
  static const meta = TextStyle(
    fontFamily: _body,
    fontSize: 11,
    color: DropTokens.muted,
  );
  static const metaSm = TextStyle(
    fontFamily: _body,
    fontSize: 10,
    color: DropTokens.muted,
  );

  /// Микро-лейбл с разрядкой: «YOUR NIGHT STARTS HERE».
  static const label = TextStyle(
    fontFamily: _body,
    fontSize: 11,
    fontWeight: FontWeight.w600,
    letterSpacing: 1,
    color: DropTokens.muted,
  );
  static const labelTight = TextStyle(
    fontFamily: _body,
    fontSize: 10,
    fontWeight: FontWeight.w700,
    letterSpacing: 1,
    color: DropTokens.muted,
  );
  static const labelWide = TextStyle(
    fontFamily: _body,
    fontSize: 10,
    fontWeight: FontWeight.w700,
    letterSpacing: 1.4,
    color: DropTokens.muted,
  );

  /// Текст на главной кнопке (Inter 16 SemiBold).
  static const button = TextStyle(
    fontFamily: _body,
    fontSize: 16,
    fontWeight: FontWeight.w600,
    height: 1.25,
    color: DropTokens.onAccent,
  );

  /// Данные: ISRC, время, суммы (Geist Mono).
  static const data = TextStyle(
    fontFamily: mono,
    fontSize: 12,
    color: DropTokens.muted,
  );
  static const dataSm = TextStyle(
    fontFamily: mono,
    fontSize: 11,
    color: DropTokens.muted,
  );
}
