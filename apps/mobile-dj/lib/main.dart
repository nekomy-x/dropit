import 'package:flutter/material.dart';

import 'screens/login_screen.dart';
import 'screens/queue_screen.dart';
import 'screens/settings_screen.dart';
import 'screens/stats_screen.dart';
import 'api/api_client.dart';

/// DROP IT — приложение диджея.
///
/// Экраны читаются **в темноте, одним взглядом, под музыку**:
/// крупная типографика, минимум мелких элементов (dizayn-zadanie §1).

void main() {
  runApp(const DropItDjApp());
}

/// Тема: те же токены дизайн-системы, что у приложения гостя (§5).
class DjTheme {
  static const black = Color(0xFF0A0A0A);
  static const surface = Color(0xFF141414);
  static const neon = Color(0xFF39FF88);
  static const white = Color(0xFFF5F5F5);
  static const grey = Color(0xFF8A8A8A);
  static const red = Color(0xFFFF4D4D);

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
      // Крупная типографика для чтения в темноте (§1).
      textTheme: base.textTheme
          .apply(
            bodyColor: white,
            displayColor: white,
          )
          .copyWith(
            bodyLarge: const TextStyle(fontSize: 20),
            titleLarge: const TextStyle(fontSize: 26, fontWeight: FontWeight.w800),
            titleMedium: const TextStyle(fontSize: 22, fontWeight: FontWeight.w700),
          ),
      appBarTheme: const AppBarTheme(
        backgroundColor: black,
        foregroundColor: white,
        elevation: 0,
      ),
      // A field is a line under the value, not a box: the same treatment the
      // guest app and the web panels use. `UnderlineInputBorder` is a straight
      // 1px rule at the bottom edge; the accent colour marks focus.
      inputDecorationTheme: const InputDecorationTheme(
        filled: false,
        border: UnderlineInputBorder(
          borderRadius: BorderRadius.zero,
          borderSide: BorderSide(color: grey),
        ),
        enabledBorder: UnderlineInputBorder(
          borderRadius: BorderRadius.zero,
          borderSide: BorderSide(color: grey),
        ),
        focusedBorder: UnderlineInputBorder(
          borderRadius: BorderRadius.zero,
          borderSide: BorderSide(color: neon),
        ),
        errorBorder: UnderlineInputBorder(
          borderRadius: BorderRadius.zero,
          borderSide: BorderSide(color: red),
        ),
        focusedErrorBorder: UnderlineInputBorder(
          borderRadius: BorderRadius.zero,
          borderSide: BorderSide(color: red),
        ),
      ),
      elevatedButtonTheme: ElevatedButtonThemeData(
        style: ElevatedButton.styleFrom(
          backgroundColor: neon,
          foregroundColor: black,
          textStyle: const TextStyle(fontSize: 18, fontWeight: FontWeight.w800),
          minimumSize: const Size.fromHeight(56),
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

class DropItDjApp extends StatelessWidget {
  const DropItDjApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'DROP IT DJ',
      debugShowCheckedModeBanner: false,
      theme: DjTheme.theme(),
      home: const DjFlow(),
    );
  }
}

/// Корневой флоу: логин → очередь (главный экран) с навигацией
/// в настройки вечера и статистику.
class DjFlow extends StatefulWidget {
  const DjFlow({super.key});

  @override
  State<DjFlow> createState() => _DjFlowState();
}

class _DjFlowState extends State<DjFlow> {
  final ApiClient _api = ApiClient();
  bool _loggedIn = false;
  int _tab = 0;

  void _onLoggedIn() => setState(() => _loggedIn = true);

  @override
  Widget build(BuildContext context) {
    if (!_loggedIn) {
      return LoginScreen(api: _api, onLoggedIn: _onLoggedIn);
    }
    return Scaffold(
      body: IndexedStack(
        index: _tab,
        children: [
          QueueScreen(api: _api),
          SettingsScreen(api: _api),
          StatsScreen(api: _api),
        ],
      ),
      bottomNavigationBar: NavigationBar(
        backgroundColor: DjTheme.black,
        selectedIndex: _tab,
        onDestinationSelected: (i) => setState(() => _tab = i),
        destinations: const [
          NavigationDestination(icon: Icon(Icons.queue_music), label: 'Queue'),
          NavigationDestination(icon: Icon(Icons.tune), label: 'Night'),
          NavigationDestination(icon: Icon(Icons.bar_chart), label: 'Stats'),
        ],
      ),
    );
  }
}
