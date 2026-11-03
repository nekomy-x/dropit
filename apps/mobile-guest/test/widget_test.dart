import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import 'package:dropit_guest/data/demo_data.dart';
import 'package:dropit_guest/data/models.dart';
import 'package:dropit_guest/main.dart';
import 'package:dropit_guest/screens/help_screen.dart';
import 'package:dropit_guest/screens/home_screen.dart';
import 'package:dropit_guest/screens/identity_screen.dart';
import 'package:dropit_guest/screens/room_screen.dart';
import 'package:dropit_guest/screens/profile_screen.dart';
import 'package:dropit_guest/screens/requests_screen.dart';
import 'package:dropit_guest/screens/scan_screen.dart';
import 'package:dropit_guest/screens/search_screen.dart';
import 'package:dropit_guest/state/app_state.dart';
import 'package:dropit_guest/theme/app_theme.dart';

/// Тесты гостевого приложения: проверяем, что экраны держат контракт
/// макета pen.dev (Guest 01–15) — тексты, кнопки, флоу.
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUp(() {
    FlutterSecureStorage.setMockInitialValues({});
  });

  Widget wrap(Widget child, {AppState? state}) {
    final s = state ?? AppState();
    return AppStateScope(
      state: s,
      child: MaterialApp(theme: DropTheme.theme(), home: child),
    );
  }

  const session = JoinSession(
    room: DemoData.katerBlau,
    dj: 'DJ Markus',
    priceLabel: '3,00 €',
  );

  testWidgets('Home (01): заголовок, плитки действий и таб-бар',
      (tester) async {
    await tester.pumpWidget(wrap(const HomeScreen(
      onScan: _noop,
      onExplore: _noop,
      onOpenRoom: _noopRoom,
      onRequestTrack: _noopTrack,
    )));
    await tester.pump();

    expect(find.text('Your track.\nTheir next move.'), findsOneWidget);
    expect(find.text('12 ROOMS LIVE TONIGHT'), findsOneWidget);
    expect(find.text('Scan to enter'), findsOneWidget);
    expect(find.text('Nearby rooms'), findsOneWidget);
    expect(find.text('Set the mood'), findsOneWidget);
    expect(
        find.text('Join a room to send your pick to the DJ.'), findsOneWidget);
  });

  testWidgets('Scan (02): заголовок, инструменты и ручной ввод',
      (tester) async {
    await tester.pumpWidget(wrap(const ScanScreen(
      onClose: _noop,
      onJoined: _noopRoom,
    )));
    await tester.pump();

    expect(find.text('Scan in. Tune in.'), findsOneWidget);
    expect(find.text('Scanning automatically'), findsOneWidget);
    expect(find.text('Flash'), findsOneWidget);
    expect(find.text('From photos'), findsOneWidget);
    expect(find.text('Type room code manually'), findsOneWidget);
  });

  testWidgets('Search (03): три трека, выбор и нижний блок', (tester) async {
    final state = AppState()..joinRoom(DemoData.katerBlau);
    await tester.pumpWidget(wrap(
      const SearchScreen(
        session: session,
        onBack: _noop,
        onContinue: _noopTrack,
      ),
      state: state,
    ));
    await tester.pump();

    expect(find.text('Choose your track.'), findsOneWidget);
    expect(find.text('3 TRACKS · SELECT ONE'), findsOneWidget);
    expect(DemoData.tracks.length, 3);
    // Нижний блок выбранного трека скрыт до выбора.
    expect(find.text('SELECTED TRACK'), findsNothing);

    // Тапаем по подписи исполнителя первой строки (hint «Move» в поиске
    // тоже содержит текст «Move», поэтому берём уникальный текст строки).
    await tester.tap(find.text('Adam Port, Stryv · Afro house').first);
    await tester.pump();
    await tester.scrollUntilVisible(find.text('SELECTED TRACK'), 150,
        scrollable: find.byType(Scrollable).first);
    expect(find.text('SELECTED TRACK'), findsOneWidget);
    expect(find.text('Continue to payment'), findsOneWidget);
    expect(state.selectedTrack?.title, 'Move');
  });

  testWidgets('My requests (06): счётчики статусов и карточки', (tester) async {
    await tester
        .pumpWidget(wrap(const RequestsScreen(onOpenRequest: _noopRequest)));
    await tester.pump();

    expect(find.text('My requests'), findsOneWidget);
    expect(find.text('All · 5'), findsOneWidget);
    expect(find.text('Upcoming · 2'), findsOneWidget);
    expect(find.text('Played · 2'), findsOneWidget);
    expect(find.text('Declined · 1'), findsOneWidget);
    expect(find.text('In queue · #3'), findsOneWidget);
    expect(find.text('Awaiting DJ'), findsOneWidget);

    // Фильтр Declined — один запрос и пояснение.
    await tester.tap(find.text('Declined · 1'));
    await tester.pump();
    expect(find.text('1 REQUESTS'), findsOneWidget);
    expect(
      find.text(
          'Declined requests are refunded to your original payment method.'),
      findsOneWidget,
    );
  });

  testWidgets('Profile · Guest (07b): кнопки входа и преимущества',
      (tester) async {
    await tester.pumpWidget(wrap(const ProfileGuestScreen(
      onSignIn: _noop,
      onCreateAccount: _noop,
    )));
    await tester.pump();

    expect(find.text('Your night.\nYour profile.'), findsOneWidget);
    expect(find.text('Sign in'), findsOneWidget);
    expect(find.text('Create an account'), findsOneWidget);
    expect(find.text('All your requests, in one place'), findsOneWidget);
  });

  testWidgets('Полное приложение: Home → таб Requests → Profile',
      (tester) async {
    await tester.pumpWidget(const DropItApp());
    // Дать время на инициализацию secure storage/бэкенда.
    for (var i = 0; i < 10; i++) {
      await tester.pump(const Duration(milliseconds: 200));
    }

    expect(find.text('Your track.\nTheir next move.'), findsOneWidget);

    await tester.tap(find.text('Requests').first);
    await tester.pump();
    expect(find.text('My requests'), findsOneWidget);

    await tester.tap(find.text('Profile').first);
    await tester.pump();
    expect(find.text('Your night.\nYour profile.'), findsOneWidget);
  });

  test('AppState: оплата создаёт запрос со статусом «в очереди»', () async {
    final state = AppState();
    final before = state.requests.length;
    final created = await state.submitRequest(
      DemoData.trackMove,
      DemoData.katerBlau,
    );
    expect(state.requests.length, before + 1);
    expect(created.status, RequestStatus.queue);
    expect(state.requests.first.id, created.id);
  });

  testWidgets('Room (12): now playing and queue', (tester) async {
    await tester.pumpWidget(wrap(const RoomScreen(
      room: DemoData.katerBlau,
      onBack: _noop,
      onRequestTrack: _noop,
    )));
    await tester.pump();

    expect(find.text('NOW PLAYING'), findsOneWidget);
    expect(find.text('Move'), findsOneWidget);
    expect(find.text('UP NEXT'), findsOneWidget);
  });

  testWidgets('Help (14): FAQ accordion', (tester) async {
    await tester.pumpWidget(wrap(const HelpScreen(onBack: _noop)));
    await tester.pump();

    expect(find.text('How can we help?'), findsOneWidget);
    expect(find.text('How do track requests work?'), findsOneWidget);
  });

  testWidgets('Identity (15): personal details fields', (tester) async {
    await tester.pumpWidget(wrap(const IdentityScreen(onBack: _noop)));
    await tester.pump();

    expect(find.text('Personal details'), findsOneWidget);
    expect(find.text('mara@nightmail.de'), findsOneWidget);
    expect(find.text('MK-4821'), findsOneWidget);
    expect(find.text('Berlin, DE'), findsOneWidget);
    expect(find.text('Save changes'), findsOneWidget);
  });
}

void _noop() {}
void _noopRoom(Room room) {}
void _noopTrack(Track track) {}
void _noopRequest(GuestRequest request) {}
