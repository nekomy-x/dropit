import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'api/api_client.dart';
import 'screens/home_screen.dart';
import 'screens/payment_screen.dart';
import 'screens/request_sent_screen.dart';
import 'screens/requests_screen.dart';
import 'screens/scan_screen.dart';
import 'screens/search_screen.dart';
import 'screens/terms_screen.dart';
import 'theme/app_theme.dart';
import 'theme/tokens.dart';
import 'widgets/brand_wave_logo.dart';

/// DROP IT — приложение гостя.
///
/// Флоу по макету (`docs/ui/reference/dropit-design.pen`):
///   Home (01) → Scan QR (02) → Search (03) → Terms before payment (04a)
///   → Payment (04, шторки 04b/04d) → Request sent (05) → My requests (06).
///
/// Оформление — токены дизайн-системы: чёрная «ночь», неон `#00FF87`,
/// Anton / Inter / Geist Mono.

void main() {
  runApp(const DropItApp());
}

class DropItApp extends StatelessWidget {
  const DropItApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'DROP IT',
      debugShowCheckedModeBanner: false,
      theme: DropTheme.theme(),
      home: const GuestFlow(),
    );
  }
}

/// Корневой флоу: хранит session-контекст (грант после скана QR)
/// и выбранный трек.
class GuestFlow extends StatefulWidget {
  const GuestFlow({super.key});

  @override
  State<GuestFlow> createState() => _GuestFlowState();
}

class _GuestFlowState extends State<GuestFlow> {
  ApiClient? _api;
  JoinResponse? _session;

  @override
  void initState() {
    super.initState();
    _initApi();
  }

  Future<void> _initApi() async {
    final prefs = await SharedPreferences.getInstance();
    final api = ApiClient(prefs);
    try {
      await api.ensureDeviceToken();
    } catch (_) {
      // Сервер недоступен (офлайн/демо): экраны без сетевых вызовов
      // (Home, скан, условия) работают, сетевые покажут состояние ошибки.
    }
    if (mounted) setState(() => _api = api);
  }

  void _push(Widget screen) {
    Navigator.of(context).push(MaterialPageRoute(builder: (_) => screen));
  }

  void _pushScan() {
    final api = _api;
    if (api == null) return;
    _push(ScanScreen(
      api: api,
      onClose: () => Navigator.of(context).maybePop(),
      onJoined: (session) {
        setState(() => _session = session);
        _pushSearch();
      },
    ));
  }

  void _pushSearch() {
    final session = _session;
    if (session == null) return;
    _push(SearchScreen(
      session: session,
      onOpenRequests: _pushRequests,
      onRequestTrack: (track) => _push(TermsScreen(
        session: session,
        track: track,
        onConfirmed: () => _push(PaymentScreen(
          api: _api!,
          session: session,
          track: track,
          onSent: (created) => _push(RequestSentScreen(
            session: session,
            track: track,
            created: created,
            onViewRequest: _pushRequests,
            onFindAnother: () => Navigator.of(context).maybePop(),
          )),
        )),
      )),
    ));
  }

  void _pushRequests() {
    final session = _session;
    if (session == null) return;
    _push(RequestsScreen(session: session, api: _api));
  }

  @override
  Widget build(BuildContext context) {
    final api = _api;
    if (api == null) return const _Splash();
    return HomeScreen(
      onScan: _pushScan,
      onNearby: _pushScan,
      onRequests: _pushRequests,
    );
  }
}

/// Стартовый экран, пока грузится device-token: фон «ночь» и фирменный знак.
class _Splash extends StatelessWidget {
  const _Splash();

  @override
  Widget build(BuildContext context) {
    return const Scaffold(
      backgroundColor: DropTokens.bg,
      body: Center(child: BrandWaveLogo(fontSize: 22, markSize: 18)),
    );
  }
}
