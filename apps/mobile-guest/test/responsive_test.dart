import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:dropit_guest/data/demo_data.dart';
import 'package:dropit_guest/data/models.dart';
import 'package:dropit_guest/main.dart';
import 'package:dropit_guest/screens/auth_screens.dart';
import 'package:dropit_guest/screens/explore_screen.dart';
import 'package:dropit_guest/screens/help_screen.dart';
import 'package:dropit_guest/screens/identity_screen.dart';
import 'package:dropit_guest/screens/payment_screen.dart';
import 'package:dropit_guest/screens/profile_screen.dart';
import 'package:dropit_guest/screens/request_detail_screen.dart';
import 'package:dropit_guest/screens/request_sent_screen.dart';
import 'package:dropit_guest/screens/room_screen.dart';
import 'package:dropit_guest/screens/scan_screen.dart';
import 'package:dropit_guest/screens/search_screen.dart';
import 'package:dropit_guest/screens/terms_screen.dart';
import 'package:dropit_guest/state/app_state.dart';
import 'package:dropit_guest/theme/app_theme.dart';
import 'package:dropit_guest/widgets/ui.dart';

void noop() {}
const session = JoinSession(
    room: DemoData.katerBlau, dj: 'DJ Markus', priceLabel: '3,00 €');

void main() {
  setUpAll(() async {
    final loader = FontLoader('Inter')
      ..addFont(rootBundle.load('assets/fonts/Inter-Regular.ttf'));
    await loader.load();
  });
  final screens = <String, Widget Function()>{
    'root': () => const RootScreen(),
    'explore': () => ExploreScreen(onOpenRoom: (_) {}),
    'scan': () => ScanScreen(onClose: noop, onJoined: (_) {}),
    'search': () =>
        SearchScreen(session: session, onBack: noop, onContinue: (_) {}),
    'terms': () =>
        const TermsScreen(session: session, onBack: noop, onAccept: noop),
    'payment': () =>
        PaymentScreen(session: session, onBack: noop, onPaid: (_) async {}),
    'sent': () => RequestSentScreen(
        session: session,
        request: DemoData.myRequests.first,
        onViewRequest: noop,
        onFindAnother: noop),
    'detail': () =>
        RequestDetailScreen(request: DemoData.myRequests.first, onBack: noop),
    'room': () => const RoomScreen(
        room: DemoData.katerBlau, onBack: noop, onRequestTrack: noop),
    'profile': () => const ProfileScreen(
        onOpenIdentity: noop,
        onOpenHelp: noop,
        onOpenPrivacy: noop,
        onOpenPaymentMethods: noop),
    'guest': () =>
        const ProfileGuestScreen(onSignIn: noop, onCreateAccount: noop),
    'help': () => const HelpScreen(onBack: noop),
    'identity': () => const IdentityScreen(onBack: noop),
    'sign-in': () => const SignInScreen(
        onBack: noop,
        onDone: noop,
        onCreateAccount: noop,
        onForgotPassword: noop),
    'register': () =>
        const CreateAccountScreen(onBack: noop, onDone: noop, onSignIn: noop),
    'reset': () =>
        const ResetPasswordScreen(onBack: noop, onBackToSignIn: noop),
  };
  final layouts = <String, (Size, double, double)>{
    'compact': (const Size(320, 568), 1, 0),
    'standard': (const Size(390, 844), 1, 0),
    'tall': (const Size(430, 932), 1, 0),
    'landscape': (const Size(844, 390), 1, 0),
    'large-text': (const Size(320, 568), 1.5, 0),
    'keyboard': (const Size(390, 844), 1, 310),
  };
  for (final platform in [TargetPlatform.android, TargetPlatform.iOS]) {
    for (final layout in layouts.entries) {
      for (final screen in screens.entries) {
        testWidgets('${platform.name} ${layout.key} ${screen.key}',
            (tester) async {
          final (size, scale, keyboard) = layout.value;
          tester.view.devicePixelRatio = 1;
          tester.view.physicalSize = size;
          final top = size.width > size.height
              ? 0.0
              : platform == TargetPlatform.iOS
                  ? 59.0
                  : 24.0;
          final bottom = platform == TargetPlatform.iOS ? 34.0 : 24.0;
          tester.view.viewPadding = FakeViewPadding(top: top, bottom: bottom);
          tester.view.padding =
              FakeViewPadding(top: top, bottom: keyboard > 0 ? 0 : bottom);
          tester.view.viewInsets = FakeViewPadding(bottom: keyboard);
          addTearDown(tester.view.reset);
          final state = AppState()
            ..joinRoom(DemoData.katerBlau)
            ..selectTrack(DemoData.trackMove);
          addTearDown(state.dispose);
          await tester.pumpWidget(AppStateScope(
              state: state,
              child: MaterialApp(
                theme: DropTheme.theme().copyWith(platform: platform),
                builder: (context, child) => MediaQuery(
                    data: MediaQuery.of(context)
                        .copyWith(textScaler: TextScaler.linear(scale)),
                    child: child!),
                home: screen.value(),
              )));
          await tester.pump(const Duration(milliseconds: 100));
          expect(tester.takeException(), isNull);
          // Every rendered tab bar must clear the system navigation area.
          for (final element in find.byType(DropTabBar).evaluate()) {
            final rect = tester.getRect(find.byWidget(element.widget));
            expect(rect.bottom, lessThanOrEqualTo(size.height - bottom));
          }
          // Exercise lazily built content as well as the first viewport.
          final scrolls = tester
              .stateList<ScrollableState>(find.byType(Scrollable))
              .toList();
          for (final scroll in scrolls) {
            if (!scroll.mounted ||
                axisDirectionToAxis(scroll.axisDirection) != Axis.vertical) {
              continue;
            }
            final end = scroll.position.maxScrollExtent;
            for (final offset in [end / 2, end]) {
              if (!scroll.mounted) break;
              scroll.position.jumpTo(offset);
              await tester.pump();
              expect(tester.takeException(), isNull);
            }
          }
          await tester.pumpWidget(const SizedBox());
          await tester.pump();
        }, variant: TargetPlatformVariant({platform}));
      }
    }
  }
}
