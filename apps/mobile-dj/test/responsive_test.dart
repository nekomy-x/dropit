import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:dropit_dj/api/api_client.dart';
import 'package:dropit_dj/main.dart';
import 'package:dropit_dj/screens/login_screen.dart';
import 'package:dropit_dj/screens/queue_screen.dart';
import 'package:dropit_dj/screens/settings_screen.dart';
import 'package:dropit_dj/screens/stats_screen.dart';

class _ResponsiveApi extends ApiClient {
  final item = QueueItem(
    id: 'request-1',
    status: 'pending',
    title: 'A track with a longer title',
    artist: 'Artist and featured artist',
    createdAt: DateTime.now().subtract(const Duration(minutes: 12)),
    style: 'Reggaeton / Latin house',
    sameTrackCount: 27,
    isrc: 'USRC17607839',
  );
  @override
  Future<List<QueueItem>> queue({String sort = 'new'}) async => [item];
  @override
  Future<ShiftInfo?> shift() async => null;
  @override
  Future<DjStats> statsToday() async => DjStats(
        requestsToday: 1234,
        accepted: 1120,
        played: 1024,
        rejected: 114,
        earnedTotalMinor: 1234567,
      );
}

Future<void> _pumpScreen(WidgetTester tester, Widget screen, Size size,
    {double scale = 1}) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  tester.view.viewPadding = const FakeViewPadding(top: 24, bottom: 24);
  tester.view.padding = const FakeViewPadding(top: 24, bottom: 24);
  addTearDown(tester.view.reset);
  await tester.pumpWidget(MaterialApp(
    theme: DjTheme.theme(),
    builder: (context, child) => MediaQuery(
      data:
          MediaQuery.of(context).copyWith(textScaler: TextScaler.linear(scale)),
      child: child!,
    ),
    home: screen,
  ));
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 100));
}

void main() {
  final layouts = <String, (Size, double)>{
    'compact': (const Size(320, 568), 1),
    'standard': (const Size(390, 844), 1),
    'wide-phone': (const Size(430, 932), 1),
    'large-text': (const Size(320, 568), 1.5),
    'landscape': (const Size(844, 390), 1),
  };
  for (final layout in layouts.entries) {
    for (final name in ['login', 'queue', 'settings', 'stats']) {
      testWidgets(
          '${layout.key} $name uses the available width without overflow',
          (tester) async {
        final api = _ResponsiveApi();
        final screen = switch (name) {
          'login' => LoginScreen(api: api, onLoggedIn: () {}),
          'queue' => QueueScreen(api: api),
          'settings' => SettingsScreen(api: api),
          _ => StatsScreen(api: api),
        };
        await _pumpScreen(tester, screen, layout.value.$1,
            scale: layout.value.$2);
        expect(tester.takeException(), isNull);
        if (name == 'queue' && layout.value.$1.width <= 430) {
          expect(tester.getSize(find.text(api.item.title)).width,
              greaterThanOrEqualTo(layout.value.$1.width * 0.6),
              reason: 'Track text should use most of the phone width');
        }
        final scrolls =
            tester.stateList<ScrollableState>(find.byType(Scrollable)).toList();
        for (final scroll in scrolls) {
          if (!scroll.mounted ||
              axisDirectionToAxis(scroll.axisDirection) != Axis.vertical) {
            continue;
          }
          scroll.position.jumpTo(scroll.position.maxScrollExtent);
          await tester.pump();
          expect(tester.takeException(), isNull);
        }
        await tester.pumpWidget(const SizedBox());
      });
    }
  }
  for (final (size, scale) in [
    (const Size(320, 568), 1.5),
    (const Size(844, 390), 1.0)
  ]) {
    testWidgets(
        'request actions remain reachable at $size and text scale $scale',
        (tester) async {
      final api = _ResponsiveApi();
      await _pumpScreen(tester, QueueScreen(api: api), size, scale: scale);
      expect(tester.takeException(), isNull);
      await tester.tap(find.text(api.item.title));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.ensureVisible(find.text('Played'));
      expect(find.text('Played').hitTestable(), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    });
  }
}
