import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:dropit_guest/main.dart';
import 'package:dropit_guest/api/api_client.dart';

void main() {
  testWidgets('QR-скан: пустой ввод показывает подсказку', (tester) async {
    await tester.pumpWidget(const DropItApp());
    await tester.pumpAndSettle();

    // Заголовок приложения.
    expect(find.text('DROP IT'), findsOneWidget);

    // Кнопка join без ввода → подсказка.
    await tester.tap(find.text('Join the night'));
    await tester.pump();
    expect(find.text('Enter the code from the club QR'), findsOneWidget);
  });

  testWidgets('Сессия: цена из ответа join (не хардкод €5)', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: DropItTheme.theme(),
        home: SessionScreen(
          session: JoinResponse(
            sessionId: 's-1',
            clubName: 'Test Club',
            djDisplayName: 'DJ Test',
            status: 'active',
            timezone: 'Europe/Madrid',
            priceMinor: 300, // 3,00 € — цена заведения
            grantId: 'g-1',
          ),
          onSearch: () {},
          onMyRequests: () {},
        ),
      ),
    );

    expect(find.text('Test Club'), findsOneWidget);
    expect(find.text('DJ Test'), findsOneWidget);
    // Динамическая цена (dizayn-zadanie §0.1: убрать хардкоды).
    expect(find.text('€3.00'), findsWidgets);
    // Обязательный дисклеймер (§0.2).
    expect(find.textContaining('stays with the DJ'), findsOneWidget);
  });

  testWidgets('Условия оплаты: чекбокс блокирует кнопку до принятия',
      (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: DropItTheme.theme(),
        home: ConditionsScreen(
          session: JoinResponse(
            sessionId: 's-1',
            clubName: 'Test Club',
            status: 'active',
            timezone: 'Europe/Madrid',
            priceMinor: 300,
            grantId: 'g-1',
          ),
          track: CatalogTrack(
            id: 't-1',
            title: 'Vivir Mi Vida',
            artist: 'Marc Anthony',
          ),
          onConfirmed: () {},
        ),
      ),
    );

    // Три обязательных пункта (§0.2).
    expect(find.textContaining('each request'), findsOneWidget);
    expect(find.textContaining('only be played once'), findsOneWidget);

    // Кнопка заблокирована до принятия условий.
    final button = find.text('Pay €3.00 & send request');
    await tester.tap(button);
    await tester.pump();
    // Чекбокс не нажат → подтверждение не сработало (экран не ушёл).
    expect(find.textContaining('I understand'), findsOneWidget);

    await tester.tap(find.text('I understand and accept'));
    await tester.pump();
    expect(find.text('I understand and accept'), findsOneWidget);
  });
}
