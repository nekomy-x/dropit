/// DROP IT — приложение гостя.
///
/// Перенос 1:1 макета pen.dev (Guest 01–15): Listen (Home), Explore,
/// Requests (My requests + Request detail), Profile (+ авторизация),
/// флоу Scan → Search → Terms → Payment (+ шторки) → Request sent.
///
/// Токены, шрифты (Anton / Inter / Geist Mono) и изображения — из
/// дизайн-системы макета; изображения встроены в `assets/images`,
/// поэтому APK работает без сети (реальные запросы уходят в бэкенд,
/// когда он доступен).
library;

import 'package:flutter/material.dart';
import 'package:flutter_stripe/flutter_stripe.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import 'api/api_client.dart';
import 'data/models.dart';
import 'screens/auth_screens.dart';
import 'screens/explore_screen.dart';
import 'screens/help_screen.dart';
import 'screens/home_screen.dart';
import 'screens/identity_screen.dart';
import 'screens/payment_screen.dart';
import 'screens/profile_screen.dart';
import 'screens/request_detail_screen.dart';
import 'screens/request_sent_screen.dart';
import 'screens/requests_screen.dart';
import 'screens/room_screen.dart';
import 'screens/scan_screen.dart';
import 'screens/search_screen.dart';
import 'screens/terms_screen.dart';
import 'state/app_state.dart';
import 'theme/app_theme.dart';
import 'theme/tokens.dart';
import 'widgets/brand_wave_logo.dart';
import 'widgets/ui.dart';

const _stripePublishableKey = String.fromEnvironment(
  'DROPIT_STRIPE_PUBLISHABLE_KEY',
);

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  if (_stripePublishableKey.isNotEmpty) {
    Stripe.publishableKey = _stripePublishableKey;
    Stripe.urlScheme = 'dropit';
    await Stripe.instance.applySettings();
  }
  runApp(const DropItApp());
}

class DropItApp extends StatefulWidget {
  const DropItApp({super.key});

  @override
  State<DropItApp> createState() => _DropItAppState();
}

class _DropItAppState extends State<DropItApp> {
  AppState? _state;
  final AppState _placeholder = AppState();

  @override
  void initState() {
    super.initState();
    _init();
  }

  Future<void> _init() async {
    const secureStorage = FlutterSecureStorage();
    final api = ApiClient(secureStorage);
    final state = AppState(api: api);
    await state.probeBackend();
    if (!mounted) return;
    setState(() => _state = state);
  }

  @override
  Widget build(BuildContext context) {
    final state = _state;
    // AppStateScope — НАД MaterialApp: тогда состояние доступно всем
    // маршрутам навигатора (push-экраны флоу), а не только home.
    return AppStateScope(
      state: state ?? _placeholder,
      child: MaterialApp(
        title: 'DROP IT',
        debugShowCheckedModeBanner: false,
        theme: DropTheme.theme(),
        home: state == null ? const _Splash() : const RootScreen(),
      ),
    );
  }
}

/// Стартовый экран, пока грузится device-token.
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

/// Корневой экран: 4 таба (Listen · Explore · Requests · Profile)
/// и флоу запроса трека в навигационном стеке. Плавающий таб-бар — как
/// в макете (12 px от краёв, 64 px высота).
class RootScreen extends StatelessWidget {
  const RootScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final state = AppState.of(context);
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (didPop) return;
        if (state.tab != 0) {
          state.setTab(0);
        } else {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('Press back again to exit')),
          );
        }
      },
      child: Scaffold(
        backgroundColor: DropTokens.bg,
        body: Stack(
          children: [
            Positioned.fill(
              child: IndexedStack(
                index: state.tab,
                children: [
                  HomeScreen(
                    onScan: () => _push(context, const ScanRoute()),
                    onExplore: () => state.setTab(1),
                    onOpenRoom: (room) => _join(context, room),
                    onRequestTrack: (track) => _requestFromHome(context, track),
                  ),
                  ExploreScreen(
                    onOpenRoom: (room) => _openRoom(context, room),
                  ),
                  RequestsScreen(
                    onOpenRequest: (request) =>
                        _push(context, RequestDetailRoute(request: request)),
                  ),
                  state.signedIn
                      ? ProfileScreen(
                          onOpenIdentity: () =>
                              _push(context, const IdentityRoute()),
                          onOpenHelp: () => _push(context, const HelpRoute()),
                          onOpenPrivacy: () => _showPrivacy(context),
                          onOpenPaymentMethods: () =>
                              showPaymentMethodsSheet(context),
                        )
                      : ProfileGuestScreen(
                          onSignIn: () => _push(context, const SignInRoute()),
                          onCreateAccount: () =>
                              _push(context, const CreateAccountRoute()),
                        ),
                ],
              ),
            ),
            Positioned(
              left: 12,
              right: 12,
              bottom: 12,
              child: DropTabBar(
                current: state.tab,
                onChanged: (index) => state.setTab(index),
              ),
            ),
          ],
        ),
      ),
    );
  }

  // ─── Переходы флоу ───────────────────────────────────────────────────

  void _push(BuildContext context, Widget screen) {
    Navigator.of(context).push(MaterialPageRoute(builder: (_) => screen));
  }

  /// Home → рекомендация: нужна комната, иначе сначала скан.
  void _requestFromHome(BuildContext context, Track track) {
    final state = AppState.of(context);
    if (state.inRoom) {
      state.selectTrack(track);
      _push(context, TermsRoute(session: state.session!));
    } else {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Join a room first — scan the club QR')),
      );
      _push(context, const ScanRoute());
    }
  }

  /// Join комнаты из Home/Explore.
  void _join(BuildContext context, Room room) {
    final state = AppState.of(context);
    state.joinRoom(room);
    _push(context, SearchRoute(session: state.session!));
  }

  void _openRoom(BuildContext context, Room room) {
    final state = AppState.of(context);
    if (!state.inRoom || state.session!.room.id != room.id) {
      state.joinRoom(room);
    }
    _push(context, RoomRoute(room: state.session!.room));
  }

  static void _showPrivacy(BuildContext context) {
    showDialog(
      context: context,
      builder: (_) => AlertDialog(
        backgroundColor: DropTokens.sheet,
        title: const Text('Privacy & terms', style: DropType.title),
        content: SingleChildScrollView(
          child: Text(
            'DROP IT stores your device token, requests and payment '
            'methods to operate the service. Rooms see only your handle. '
            'Payments are processed by our PCI-compliant partner. '
            'Declined requests are refunded to the original payment '
            'method. Full terms — dropit.club/terms.',
            style: DropType.body.copyWith(color: DropTokens.muted),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child:
                const Text('Close', style: TextStyle(color: DropTokens.accent)),
          ),
        ],
      ),
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// Маршруты флоу: каждый owns свою логику переходов.
// ────────────────────────────────────────────────────────────────────────

class ScanRoute extends StatelessWidget {
  const ScanRoute({super.key});

  @override
  Widget build(BuildContext context) {
    final state = AppState.of(context);
    return ScanScreen(
      onClose: () => Navigator.of(context).maybePop(),
      onJoined: (room) {
        state.joinRoom(room);
        Navigator.of(context).pushReplacement(
          MaterialPageRoute(
            builder: (_) => SearchRoute(session: state.session!),
          ),
        );
      },
      onCode: (code) async {
        try {
          await state.joinByQr(code);
          if (!context.mounted) return;
          Navigator.of(context).pushReplacement(
            MaterialPageRoute(
              builder: (_) => SearchRoute(session: state.session!),
            ),
          );
        } on ApiException catch (error) {
          if (!context.mounted) return;
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text(error.message)),
          );
        } catch (_) {
          if (!context.mounted) return;
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(
                content: Text('Could not join this room. Check the code.')),
          );
        }
      },
    );
  }
}

class SearchRoute extends StatelessWidget {
  const SearchRoute({super.key, required this.session});

  final JoinSession session;

  @override
  Widget build(BuildContext context) {
    return SearchScreen(
      session: session,
      onBack: () => Navigator.of(context).maybePop(),
      onContinue: (_) {
        Navigator.of(context).push(
          MaterialPageRoute(
            builder: (_) => TermsRoute(session: session),
          ),
        );
      },
    );
  }
}

class TermsRoute extends StatelessWidget {
  const TermsRoute({super.key, required this.session});

  final JoinSession session;

  @override
  Widget build(BuildContext context) {
    return TermsScreen(
      session: session,
      onBack: () => Navigator.of(context).maybePop(),
      onAccept: () {
        Navigator.of(context).push(
          MaterialPageRoute(
            builder: (_) => PaymentRoute(session: session),
          ),
        );
      },
    );
  }
}

class PaymentRoute extends StatefulWidget {
  const PaymentRoute({super.key, required this.session});

  final JoinSession session;

  @override
  State<PaymentRoute> createState() => _PaymentRouteState();
}

class _PaymentRouteState extends State<PaymentRoute> {
  @override
  Widget build(BuildContext context) {
    return PaymentScreen(
      session: widget.session,
      onBack: () => Navigator.of(context).maybePop(),
      onPaid: (track) async {
        if (_stripePublishableKey.isEmpty) {
          throw ApiException.of(
            'stripe_unavailable',
            'Secure card payments are not configured for this environment.',
          );
        }
        final state = AppState.of(context);
        final created = await state.submitRequest(
          track,
          widget.session.room,
          onPaymentIntent: (intent) async {
            if (intent.clientSecret.isEmpty ||
                intent.clientSecret.contains('_mock')) {
              throw ApiException.of(
                'stripe_unavailable',
                'Secure card payments are not configured for this environment.',
              );
            }
            await Stripe.instance.initPaymentSheet(
              paymentSheetParameters: SetupPaymentSheetParameters(
                paymentIntentClientSecret: intent.clientSecret,
                merchantDisplayName: 'DROP IT',
                primaryButtonLabel: 'Pay now',
                returnURL: 'dropit://stripe-redirect',
              ),
            );
            await Stripe.instance.presentPaymentSheet();
          },
        );
        if (!context.mounted) return;
        Navigator.of(context).push(
          MaterialPageRoute(
            builder: (_) => RequestSentRoute(
              session: widget.session,
              request: created,
            ),
          ),
        );
      },
    );
  }
}

class RequestSentRoute extends StatelessWidget {
  const RequestSentRoute(
      {super.key, required this.session, required this.request});

  final JoinSession session;
  final GuestRequest request;

  @override
  Widget build(BuildContext context) {
    final state = AppState.of(context);
    return RequestSentScreen(
      session: session,
      request: request,
      onViewRequest: () {
        // Возврат к корню и на таб «Requests».
        Navigator.of(context).popUntil((route) => route.isFirst);
        state.setTab(2);
      },
      onFindAnother: () {
        // Возрат к поиску в той же комнате.
        Navigator.of(context).popUntil((route) => route.isFirst);
        Navigator.of(context).push(
          MaterialPageRoute(
            builder: (_) => SearchRoute(session: session),
          ),
        );
      },
    );
  }
}

class RoomRoute extends StatelessWidget {
  const RoomRoute({super.key, required this.room});

  final Room room;

  @override
  Widget build(BuildContext context) {
    return RoomScreen(
      room: room,
      onBack: () => Navigator.of(context).maybePop(),
      onRequestTrack: () {
        final state = AppState.of(context);
        if (!state.inRoom) state.joinRoom(room);
        Navigator.of(context).push(
          MaterialPageRoute(
            builder: (_) => SearchRoute(session: state.session!),
          ),
        );
      },
    );
  }
}

class RequestDetailRoute extends StatelessWidget {
  const RequestDetailRoute({super.key, required this.request});

  final GuestRequest request;

  @override
  Widget build(BuildContext context) {
    return RequestDetailScreen(
      request: request,
      onBack: () => Navigator.of(context).maybePop(),
    );
  }
}

class SignInRoute extends StatelessWidget {
  const SignInRoute({super.key});

  @override
  Widget build(BuildContext context) {
    return SignInScreen(
      onBack: () => Navigator.of(context).maybePop(),
      onDone: () => Navigator.of(context).maybePop(),
      onCreateAccount: () {
        Navigator.of(context).pushReplacement(
          MaterialPageRoute(builder: (_) => const CreateAccountRoute()),
        );
      },
      onForgotPassword: () {
        Navigator.of(context).push(
          MaterialPageRoute(builder: (_) => const ResetPasswordRoute()),
        );
      },
    );
  }
}

class CreateAccountRoute extends StatelessWidget {
  const CreateAccountRoute({super.key});

  @override
  Widget build(BuildContext context) {
    return CreateAccountScreen(
      onBack: () => Navigator.of(context).maybePop(),
      onDone: () => Navigator.of(context).maybePop(),
      onSignIn: () => Navigator.of(context).maybePop(),
    );
  }
}

class ResetPasswordRoute extends StatelessWidget {
  const ResetPasswordRoute({super.key});

  @override
  Widget build(BuildContext context) {
    return ResetPasswordScreen(
      onBack: () => Navigator.of(context).maybePop(),
      onBackToSignIn: () => Navigator.of(context).maybePop(),
    );
  }
}

class IdentityRoute extends StatelessWidget {
  const IdentityRoute({super.key});

  @override
  Widget build(BuildContext context) {
    return IdentityScreen(
      onBack: () => Navigator.of(context).maybePop(),
    );
  }
}

class HelpRoute extends StatelessWidget {
  const HelpRoute({super.key});

  @override
  Widget build(BuildContext context) {
    return HelpScreen(
      onBack: () => Navigator.of(context).maybePop(),
    );
  }
}
