// Run with: flutter test --update-goldens tool/capture_layouts.dart
// Images go to output/layouts (widget renderer, not native device captures).

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:dropit_guest/data/demo_data.dart';
import 'package:dropit_guest/data/models.dart';
import 'package:dropit_guest/main.dart';
import 'package:dropit_guest/screens/payment_screen.dart';
import 'package:dropit_guest/screens/request_sent_screen.dart';
import 'package:dropit_guest/screens/scan_screen.dart';
import 'package:dropit_guest/screens/search_screen.dart';
import 'package:dropit_guest/state/app_state.dart';
import 'package:dropit_guest/theme/app_theme.dart';

void noop() {}
const session = JoinSession(
    room: DemoData.katerBlau, dj: 'DJ Markus', priceLabel: '3,00 €');

void main() {
  setUpAll(() async {
    await (FontLoader('MaterialIcons')
          ..addFont(rootBundle.load('fonts/MaterialIcons-Regular.otf')))
        .load();
    await (FontLoader('Inter')
          ..addFont(rootBundle.load('assets/fonts/Inter-Regular.ttf')))
        .load();
  });
  final screens = <String, Widget Function()>{
    'home': () => const RootScreen(),
    'scan': () => ScanScreen(onClose: noop, onJoined: (_) {}),
    'search': () =>
        SearchScreen(session: session, onBack: noop, onContinue: (_) {}),
    'payment': () =>
        PaymentScreen(session: session, onBack: noop, onPaid: (_) async {}),
    'sent': () => RequestSentScreen(
        session: session,
        request: DemoData.myRequests.first,
        onViewRequest: noop,
        onFindAnother: noop),
  };
  for (final platform in [TargetPlatform.android, TargetPlatform.iOS]) {
    for (final size in [const Size(320, 568), const Size(390, 844)]) {
      for (final screen in screens.entries) {
        testWidgets('${platform.name} ${size.width} ${screen.key}',
            (tester) async {
          tester.view.devicePixelRatio = 1;
          tester.view.physicalSize = size;
          final top = platform == TargetPlatform.iOS ? 59.0 : 24.0;
          final bottom = platform == TargetPlatform.iOS ? 34.0 : 24.0;
          tester.view.padding = FakeViewPadding(top: top, bottom: bottom);
          tester.view.viewPadding = FakeViewPadding(top: top, bottom: bottom);
          addTearDown(tester.view.reset);
          final state = AppState()
            ..joinRoom(DemoData.katerBlau)
            ..selectTrack(DemoData.trackMove);
          addTearDown(state.dispose);
          final key = GlobalKey();
          await tester.pumpWidget(RepaintBoundary(
              key: key,
              child: AppStateScope(
                  state: state,
                  child: MaterialApp(
                    debugShowCheckedModeBanner: false,
                    theme: DropTheme.theme().copyWith(platform: platform),
                    builder: (context, child) => MediaQuery(
                        data: MediaQuery.of(context)
                            .copyWith(disableAnimations: true),
                        child: child!),
                    home: screen.value(),
                  ))));
          await tester.runAsync(
              () => Future<void>.delayed(const Duration(milliseconds: 150)));
          await tester.pump(const Duration(milliseconds: 100));
          final name =
              '${platform.name}-${size.width.toInt()}x${size.height.toInt()}-${screen.key}.png';
          await expectLater(
              find.byKey(key), matchesGoldenFile('../output/layouts/$name'));
          expect(tester.takeException(), isNull);
          await tester.pumpWidget(const SizedBox());
        }, variant: TargetPlatformVariant({platform}));
      }
    }
  }
}
