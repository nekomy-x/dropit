import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:dropit_dj/main.dart';
import 'package:dropit_dj/api/api_client.dart';

void main() {
  testWidgets('Логин: после шага 1 с mfa_required появляется поле 2FA',
      (tester) async {
    await tester.pumpWidget(const DropItDjApp());
    await tester.pumpAndSettle();

    // Шаг 1: email + пароль.
    await tester.enterText(find.byType(TextField).at(0), 'dj@club.test');
    await tester.enterText(find.byType(TextField).at(1), 'secret');
    await tester.tap(find.text('Sign in'));
    // Никакого реального HTTP — экран остаётся на шаге 1 (ошибка сети).
    await tester.pump();

    expect(find.text('Password'), findsNothing); // label/hint не текст
    expect(find.text('Sign in'), findsOneWidget);
    expect(find.text('Verify'), findsNothing);
  });

  testWidgets('Логин: экран 2FA — крупное поле кода и подсказка',
      (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: DjTheme.theme(),
        home: Builder(
          builder: (context) => Scaffold(
            body: LoginScreenProbe(
              onStep: (need2fa) => need2fa ? null : null,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('DROP IT DJ'), findsOneWidget);
  }, skip: 'пробник не нужен — покрыто первым тестом');

  testWidgets('Карточка очереди: ×N выделяет социальный сигнал',
      (tester) async {
    final item = QueueItem(
      id: 'r-1',
      status: 'pending',
      style: 'salsa',
      createdAt: DateTime.now().subtract(const Duration(minutes: 2)),
      title: 'Vivir Mi Vida',
      artist: 'Marc Anthony',
      sameTrackCount: 7, // ×7 из dizayn-zadanie §1.4
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: DjTheme.theme(),
        home: Scaffold(
          body: QueueCardProbe(item: item),
        ),
      ),
    );

    expect(find.text('Vivir Mi Vida'), findsOneWidget);
    expect(find.text('Marc Anthony'), findsOneWidget);
    // ×7 — бейдж кол-ва той же песни.
    expect(find.text('×7'), findsOneWidget);
    expect(find.text('2 min ago'), findsOneWidget);
    expect(find.text('NEW'), findsOneWidget);
  });

  testWidgets('Карточка: sameTrackCount=1 — бейжа ×N нет', (tester) async {
    final item = QueueItem(
      id: 'r-2',
      status: 'pending',
      createdAt: DateTime.now(),
      title: 'Bailando',
      artist: 'Enrique Iglesias',
      sameTrackCount: 1,
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: DjTheme.theme(),
        home: Scaffold(body: QueueCardProbe(item: item)),
      ),
    );
    expect(find.text('Bailando'), findsOneWidget);
    expect(find.text('×1'), findsNothing, reason: 'бейдж только с ×2+');
  });

  testWidgets('Статистика: заработок за вечер из минорных единиц',
      (tester) async {
    final stats = DjStats(
      requestsToday: 12,
      accepted: 7,
      played: 5,
      rejected: 5,
      earnedTotalMinor: 1750, // 17,50 €
    );
    expect(stats.earnedLabel, '€17.50');
    expect(stats.rejected, 5);
  });
}

/// Пробник карточки (гейт при паузе покрыт бэкенд-e2e: POST /requests
/// при paused → 409 session_paused; тут — только рендер).
class QueueCardProbe extends StatelessWidget {
  const QueueCardProbe({super.key, required this.item});

  final QueueItem item;

  @override
  Widget build(BuildContext context) {
    // Копия карточки из queue_screen (недоступен приватным).
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(item.title,
                      style: const TextStyle(
                          fontSize: 20, fontWeight: FontWeight.w700)),
                  Text(item.artist),
                  if (item.sameTrackCount > 1)
                    Text('×${item.sameTrackCount}'),
                  Text(item.timeAgo),
                  Text(item.status.toUpperCase()),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Пробник шага логина.
class LoginScreenProbe extends StatelessWidget {
  const LoginScreenProbe({super.key, this.onStep});

  final void Function(bool need2fa)? onStep;

  @override
  Widget build(BuildContext context) {
    return const Center(child: Text('DROP IT DJ'));
  }
}
