import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:dropit_guest/data/demo_data.dart';
import 'package:dropit_guest/data/models.dart';
import 'package:dropit_guest/main.dart';
import 'package:dropit_guest/screens/help_screen.dart';
import 'package:dropit_guest/screens/auth_screens.dart';
import 'package:dropit_guest/screens/scan_screen.dart';
import 'package:dropit_guest/screens/search_screen.dart';
import 'package:dropit_guest/screens/terms_screen.dart';
import 'package:dropit_guest/state/app_state.dart';
import 'package:dropit_guest/theme/app_theme.dart';

void noop() {}
const session = JoinSession(
    room: DemoData.katerBlau, dj: 'DJ Markus', priceLabel: '3,00 €');

Widget wrap(Widget screen, AppState state) => AppStateScope(
    state: state, child: MaterialApp(theme: DropTheme.theme(), home: screen));

void main() {
  setUpAll(() async {
    await (FontLoader('Inter')
          ..addFont(rootBundle.load('assets/fonts/Inter-Regular.ttf')))
        .load();
  });

  testWidgets('Password reveal works on sign-in and registration',
      (tester) async {
    final state = AppState();
    addTearDown(state.dispose);
    for (final screen in [
      const SignInScreen(
          onBack: noop,
          onDone: noop,
          onCreateAccount: noop,
          onForgotPassword: noop),
      const CreateAccountScreen(onBack: noop, onDone: noop, onSignIn: noop),
    ]) {
      await tester.pumpWidget(wrap(screen, state));
      await tester.scrollUntilVisible(find.byTooltip('Show password'), 120,
          scrollable: find.byType(Scrollable).first);
      await tester.pump();
      expect(
          tester.widgetList<TextField>(find.byType(TextField)).last.obscureText,
          isTrue);
      await tester.tap(find.byTooltip('Show password'));
      await tester.pump();
      expect(
          tester.widgetList<TextField>(find.byType(TextField)).last.obscureText,
          isFalse);
      await tester.tap(find.byTooltip('Hide password'));
      await tester.pump();
      expect(
          tester.widgetList<TextField>(find.byType(TextField)).last.obscureText,
          isTrue);
    }
  });

  testWidgets('Manual code remains reachable above keyboard on a compact phone',
      (tester) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(320, 568);
    tester.view.padding = const FakeViewPadding(top: 24, bottom: 24);
    tester.view.viewPadding = const FakeViewPadding(top: 24, bottom: 24);
    addTearDown(tester.view.reset);
    final state = AppState();
    addTearDown(state.dispose);
    String? joined;
    await tester.pumpWidget(wrap(
        ScanScreen(
            onClose: noop, onJoined: (_) {}, onCode: (value) => joined = value),
        state));
    await tester.scrollUntilVisible(find.text('Type room code manually'), 180,
        scrollable: find.byType(Scrollable).first);
    await tester.tap(find.text('Type room code manually'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    await tester.enterText(find.byType(TextField), 'ROOM-123');
    tester.view.viewInsets = const FakeViewPadding(bottom: 290);
    tester.view.padding = const FakeViewPadding(top: 24);
    await tester.pump();
    await tester.scrollUntilVisible(find.text('Join room'), 100,
        scrollable: find.byType(Scrollable).last);
    expect(tester.takeException(), isNull);
    final button = tester.getRect(find.text('Join room'));
    expect(button.bottom, lessThan(568 - 290));
    await tester.tap(find.text('Join room'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(joined, 'ROOM-123');
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('Full terms can scroll to Close on a short screen',
      (tester) async {
    await tester.binding.setSurfaceSize(const Size(568, 320));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final state = AppState()..selectTrack(DemoData.trackMove);
    addTearDown(state.dispose);
    await tester.pumpWidget(wrap(
        const TermsScreen(session: session, onBack: noop, onAccept: noop),
        state));
    await tester.tap(find.text('Read the full terms'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    await tester.scrollUntilVisible(find.text('Close'), 100,
        scrollable: find.byType(Scrollable).last);
    await tester.tap(find.text('Close'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('Full terms'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Support chat clears the keyboard on a compact phone',
      (tester) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(320, 568);
    addTearDown(tester.view.reset);
    final state = AppState();
    addTearDown(state.dispose);
    await tester.pumpWidget(wrap(const HelpScreen(onBack: noop), state));
    await tester.scrollUntilVisible(find.text('Chat with us'), 160,
        scrollable: find.byType(Scrollable).first);
    await tester.pump();
    await tester.ensureVisible(find.text('Chat with us'));
    await tester.pump();
    await tester.tap(find.text('Chat with us'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    tester.view.viewInsets = const FakeViewPadding(bottom: 290);
    await tester.pump();
    await tester.scrollUntilVisible(find.byType(TextField).last, 100,
        scrollable: find.byType(Scrollable).at(1));
    await tester.enterText(find.byType(TextField).last, 'Where is my request?');
    await tester.ensureVisible(find.byIcon(Icons.send));
    await tester.tap(find.byIcon(Icons.send));
    await tester.pump();
    expect(find.text('Where is my request?'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('An old catalog response cannot replace the latest search',
      (tester) async {
    final state = SearchState();
    addTearDown(state.dispose);
    await tester.pumpWidget(wrap(
        SearchScreen(session: session, onBack: noop, onContinue: (_) {}),
        state));
    await tester.enterText(find.byType(TextField), 'old');
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    await tester.enterText(find.byType(TextField), 'new');
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    state.searches['new']!.complete([
      const Track(
          id: 'new',
          title: 'new result',
          artist: 'Artist',
          genre: 'House',
          art: '',
          priceLabel: '3,00 €')
    ]);
    await tester.pump();
    state.searches['old']!.completeError(Exception('old request failed'));
    await tester.pump();
    expect(find.text('Could not search the catalog. Try again.'), findsNothing);
    await tester.scrollUntilVisible(find.text('new result'), 150,
        scrollable: find.byType(Scrollable).first);
    expect(find.text('new result'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
      'Demo checkout completes without a Stripe key and is marked not charged',
      (tester) async {
    final state = AppState()
      ..joinRoom(DemoData.katerBlau)
      ..selectTrack(DemoData.trackMove);
    addTearDown(state.dispose);
    final before = state.requests.length;
    await tester.pumpWidget(wrap(const PaymentRoute(session: session), state));
    expect(find.text('Demo mode · No payment will be taken'), findsOneWidget);
    await tester.tap(find.text('Send demo request'));
    await tester.pump();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(state.requests.length, before + 1);
    expect(state.requests.first.paymentLabel, 'Not charged · demo');
    expect(find.text('Your pick is in.'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });
}

class SearchState extends AppState {
  final searches = <String, Completer<List<Track>>>{};
  @override
  Future<List<Track>> searchTracks(String query) =>
      (searches[query] = Completer<List<Track>>()).future;
}
