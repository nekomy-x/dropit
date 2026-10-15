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
  // $drop-bg — «ночь», основной фон.
  static const bg = Color(0xFF0A0C0B);
  // $drop-rail — полоса навигации/рельс.
  static const rail = Color(0xFF101413);
  // $drop-surface — карточки и панели.
  static const surface = Color(0xFF141815);
  // $drop-surface-2 — приподнятые поверхности (шторки, строки).
  static const surface2 = Color(0xFF1B211D);
  // $drop-input — поля ввода.
  static const input = Color(0xFF1B211D);
  // $drop-border — 1px hairline вокруг поверхностей.
  static const border = Color(0x17FFFFFF);
  // Мягкий разделитель внутри списков.
  static const borderSoft = Color(0x14FFFFFF);
  // Разделитель строк «комната/сессия».
  static const divider = Color(0x20FFFFFF);
  // $drop-tint — 8 % зелёного (бейджи, выбранные строки).
  static const tint = Color(0x1400FF87);
  // Заливка выбранного чипа (15 %).
  static const tintStrong = Color(0x2600FF87);
  // Заливка выбранной карты в шторке (5 %).
  static const tintFaint = Color(0x0D00FF87);

  // $drop-accent — главное действие («Signal»).
  static const accent = Color(0xFF00FF87);
  // Нажатое состояние кнопки (DS · 02, pressed).
  static const accentPressed = Color(0xFF00CC6A);
  // Мятный конец фирменного градиента.
  static const accentMint = Color(0xFF66FFC2);

  // Текст на акценте — в экранах макета используется этот тёмный тон.
  static const onAccent = Color(0xFF101813);
  // $drop-ink — токен тёмного текста на акценте (тот же смысл, шире охват).
  static const ink = Color(0xFF06130C);

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
  // Предупреждение (брендбук, стр. 04).
  static const amber = Color(0xFFFFD166);

  // Служебные поверхности шторок (guest 04b/04d).
  static const sheet = Color(0xFF1C1E1D);
  static const sheetRow = Color(0xFF252826);
  static const handle = Color(0xFF606762);
  // Конфетти и искры на экране «запрос отправлен» (guest 05).
  static const sparkle = Color(0xFFBC9AFF);
  static const sparkleSoft = Color(0xFFD8C5FF);
  static const sparklePale = Color(0xFFE6FFF1);

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

  // Фирменный градиент знака: #00FF87 → #66FFC2 слева направо.
  static const brandGradient = LinearGradient(
    begin: Alignment.centerLeft,
    end: Alignment.centerRight,
    colors: [accent, accentMint],
  );
}

/// Шкала типографики. Источник — брендбук, стр. 05: базовые уровни
/// 40 / 28 / 20 / 16 / 14 и правило «Anton — заголовки», «Inter — текст и
/// кнопки», «Geist Mono — данные».
class DropType {
  DropType._();

  static const _head = 'Anton';
  static const _body = 'Inter';
  static const mono = 'GeistMono';

  /// Крупный заголовок экрана (Anton 34, интерлиньяж 1.08) — так он задан
  /// во всех экранах приложения гостя.
  static const screenTitle = TextStyle(
    fontFamily: _head,
    fontSize: 34,
    height: 1.08,
    fontWeight: FontWeight.w400,
    color: DropTokens.text,
  );

  /// Заголовок шторки (Anton 28).
  static const sheetTitle = TextStyle(
    fontFamily: _head,
    fontSize: 28,
    fontWeight: FontWeight.w400,
    color: DropTokens.text,
  );

  /// H1 брендбука (Anton 40) — витринные заголовки.
  static const hero = TextStyle(
    fontFamily: _head,
    fontSize: 40,
    height: 1.08,
    fontWeight: FontWeight.w400,
    color: DropTokens.text,
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
