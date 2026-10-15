import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:dropit_guest/api/api_client.dart';
import 'package:dropit_guest/screens/home_screen.dart';
import 'package:dropit_guest/screens/request_sent_screen.dart';
import 'package:dropit_guest/screens/search_screen.dart';
import 'package:dropit_guest/screens/terms_screen.dart';
import 'package:dropit_guest/theme/app_theme.dart';
import 'package:dropit_guest/theme/tokens.dart';

/// Тесты флоу гостя: проверяем, что реализация держит контракт макета
/// (`docs/ui/reference/dropit-design.pen`) и токены, а не «похожий»
/// произвольный вид.
void main() {
  Widget wrap(Widget child) =>
      MaterialApp(theme: DropTheme.theme(), home: child);

  JoinResponse session({int priceMinor = 300}) => JoinResponse(
        sessionId: 's-1',
        clubName: 'Kater Blau',
        djDisplayName: 'DJ Markus',
        status: 'active',
        timezone: 'Europe/Madrid',
        priceMinor: priceMinor,
        grantId: 'g-1',
      );

  testWidgets('Home (01): заголовок, обе плитки действий и бренд-знак',
      (tester) async {
    await tester.pumpWidget(wrap(HomeScreen(onScan: () {}, onNearby: () {})));
    await tester.pump();

    // Фирменный знак как в макете (в тестах — статичный кадр).
    expect(find.text('DROP IT'), findsOneWidget);
    // Заголовок герой-блока Anton 34 из кадра Guest · 01.
    expect(find.text('Your track.\nTheir next move.'), findsOneWidget);
    // Пара плиток действий.
    expect(find.text('Scan to enter'), findsOneWidget);
    expect(find.text('Nearby rooms'), findsOneWidget);
    // Парящий таб-бар.
    expect(find.text('Listen'), findsOneWidget);
    expect(find.text('Requests'), findsOneWidget);
  });

  testWidgets('Home (01): плитка «Scan to enter» открывает скан',
      (tester) async {
    var scanned = false;
    await tester.pumpWidget(
      wrap(HomeScreen(onScan: () => scanned = true, onNearby: () {})),
    );
    await tester.pump();

    await tester.tap(find.text('Scan to enter'));
    await tester.pump();
    expect(scanned, isTrue);
  });

  testWidgets('Search (03): цена из сессии, а не хардкод', (tester) async {
    CatalogTrack? requested;
    await tester.pumpWidget(wrap(SearchScreen(
      session: session(priceMinor: 350),
      onRequestTrack: (t) => requested = t,
    )));
    await tester.pump();

    expect(find.text('Choose your track.'), findsOneWidget);
    // До выбора трека закреплённого низа нет.
    expect(find.text('SELECTED TRACK'), findsNothing);

    await tester.tap(find.text('Move Your Body'));
    await tester.pump();

    // Цена приходит из сессии (3,50 €), а не из вёрстки.
    expect(find.text('€3.50'), findsOneWidget);
    expect(find.text('SELECTED TRACK'), findsOneWidget);

    await tester.tap(find.text('Continue to payment'));
    await tester.pump();
    expect(requested, isNotNull);
    expect(requested!.title, 'Move Your Body');
  });

  testWidgets('Terms (04a): три правила и подтверждение', (tester) async {
    var confirmed = false;
    await tester.pumpWidget(wrap(TermsScreen(
      session: session(),
      track: CatalogTrack(id: 't-1', title: 'Move', artist: 'Adam Port, Stryv'),
      onConfirmed: () => confirmed = true,
    )));
    await tester.pump();

    expect(find.text('Know the rules.'), findsOneWidget);
    // Правило 1 подставляет цену сессии.
    expect(find.textContaining('You pay €3.00 to send this request'), findsOneWidget);
    expect(find.textContaining('each request is paid separately'), findsOneWidget);
    expect(find.textContaining('only be played once per night'), findsOneWidget);

    // Подтверждение — обязательный шаг перед оплатой.
    await tester.tap(find.text('I understand — Continue'));
    await tester.pump();
    expect(confirmed, isTrue);
  });

  testWidgets('Request sent (05): чек с номером, статусом и оплатой',
      (tester) async {
    await tester.pumpWidget(wrap(RequestSentScreen(
      session: session(),
      track: CatalogTrack(id: 't-1', title: 'Move', artist: 'Adam Port, Stryv'),
      created: RequestCreated(
        requestId: '9f2b7c14-0000-4000-8000-00000000ab12',
        status: 'pending',
        sessionId: 's-1',
      ),
      onViewRequest: () {},
      onFindAnother: () {},
    )));
    await tester.pump();

    expect(find.text('Your pick is in.'), findsOneWidget);
    expect(find.text('Sent to DJ'), findsOneWidget);
    expect(find.text('#DI-AB12'), findsOneWidget);
    expect(find.text('€3.00'), findsOneWidget);
  });

  test('Токены: акцент и фон — из макета, а не прежние значения', () {
    // Значения переменных $drop-accent / $drop-bg из .pen.
    expect(DropTokens.accent, const Color(0xFF00FF87));
    expect(DropTokens.bg, const Color(0xFF0A0C0B));
    expect(DropTokens.surface, const Color(0xFF141815));
    expect(DropTokens.muted, const Color(0xFFA5AEA9));
    // Прежний «мятный» акцент из ранней версии приложения — не должен вернуться.
    expect(DropTokens.accent, isNot(const Color(0xFF39FF88)));
  });
}
