import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'screens/scan_screen.dart';
import 'screens/session_screen.dart';
import 'screens/search_screen.dart';
import 'screens/conditions_screen.dart';
import 'screens/requests_screen.dart';
import 'api/api_client.dart';

/// DROP IT — приложение гостя.
///
/// Флоу (dizayn-zadanie §0, техкарта Часть IV):
/// QR-скан → session info (клуб/DJ/цена) → поиск трека → экран
/// условий оплаты (ОБЯЗАТЕЛЕН по ТЗ) → запрос → статус.
///
/// Стиль: чёрный фон, белый текст, неоново-зелёный акцент (§5).

void main() {
  runApp(const DropItApp());
}

/// Тема приложения: токены дизайн-системы (dizayn-zadanie §5).
class DropItTheme {
  static const black = Color(0xFF0A0A0A);
  static const surface = Color(0xFF141414);
  static const neon = Color(0xFF39FF88); // неоново-зелёный акцент
  static const white = Color(0xFFF5F5F5);
  static const grey = Color(0xFF8A8A8A);
  static const red = Color(0xFFFF4D4D); // ошибки/отклонения

  static ThemeData theme() {
    final base = ThemeData.dark(useMaterial3: true);
    return base.copyWith(
      scaffoldBackgroundColor: black,
      colorScheme: base.colorScheme.copyWith(
        primary: neon,
        secondary: neon,
        error: red,
        surface: surface,
      ),
      textTheme: base.textTheme.apply(
        bodyColor: white,
        displayColor: white,
      ),
      appBarTheme: const AppBarTheme(
        backgroundColor: black,
        foregroundColor: white,
        elevation: 0,
      ),
      elevatedButtonTheme: ElevatedButtonThemeData(
        style: ElevatedButton.styleFrom(
          backgroundColor: neon,
          foregroundColor: black,
          textStyle: const TextStyle(fontSize: 16, fontWeight: FontWeight.w700),
          minimumSize: const Size.fromHeight(52),
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
        ),
      ),
      cardTheme: CardThemeData(
        color: surface,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
        margin: const EdgeInsets.symmetric(vertical: 6),
      ),
    );
  }
}

class DropItApp extends StatelessWidget {
  const DropItApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'DROP IT',
      debugShowCheckedModeBanner: false,
      theme: DropItTheme.theme(),
      home: const GuestFlow(),
    );
  }
}

/// Корневой флоу: хранит session-контекст (грант после скана QR).
class GuestFlow extends StatefulWidget {
  const GuestFlow({super.key});

  @override
  State<GuestFlow> createState() => _GuestFlowState();
}

class _GuestFlowState extends State<GuestFlow> {
  ApiClient? _api;

  /// Доступ к API для потомков (WS-подключения экранов).
  ApiClient? get api => _api;
  JoinResponse? _session;

  @override
  void initState() {
    super.initState();
    _initApi();
  }

  Future<void> _initApi() async {
    final prefs = await SharedPreferences.getInstance();
    final api = ApiClient(prefs);
    await api.ensureDeviceToken();
    setState(() => _api = api);
  }

  void _onJoined(JoinResponse session) {
    setState(() => _session = session);
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => SessionScreen(
          session: session,
          onSearch: () => _pushSearch(),
          onMyRequests: () => _pushRequests(),
        ),
      ),
    );
  }

  void _pushSearch() {
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => SearchScreen(
          session: _session!,
          onRequestTrack: (track) => _pushConditions(track),
        ),
      ),
    );
  }

  void _pushConditions(CatalogTrack track) {
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => ConditionsScreen(
          session: _session!,
          track: track,
          onConfirmed: () => _pushRequests(),
        ),
      ),
    );
  }

  void _pushRequests() {
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => RequestsScreen(session: _session!, api: _api),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    if (_api == null) {
      return const Scaffold(
        body: Center(child: CircularProgressIndicator()),
      );
    }
    return ScanScreen(api: _api!, onJoined: _onJoined);
  }
}
