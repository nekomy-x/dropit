import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:dropit_dj/api/queue_state.dart';
import 'package:dropit_dj/api/api_client.dart';
import 'package:dropit_dj/api/ws_client.dart';

void main() {
  // Редьюсер тестируется без UI: контракт gateway-ws → состояние.

  test('request.created вставляет карточку сверху (sort=new)', () {
    final state = QueueState(items: [
      QueueItem(
        id: 'r-1',
        status: 'accepted',
        createdAt: DateTime.now().subtract(const Duration(minutes: 5)),
        title: 'Old Song',
        artist: 'A',
        sameTrackCount: 1,
      ),
    ]);

    final next = state.apply(
      WsEvent(
        kind: 'request.created',
        requestId: 'r-2',
        title: 'New Song',
        sameTrackCount: 1,
      ),
      'new',
    );

    expect(next.items.length, 2);
    expect(next.items.first.id, 'r-2', reason: 'новая сверху (new)');
    expect(next.items.first.title, 'New Song');
    expect(next.items.first.status, 'pending');
  });

  test('request.accepted меняет статус карточки', () {
    final state = QueueState(items: [
      QueueItem(
        id: 'r-1',
        status: 'pending',
        createdAt: DateTime.now(),
        title: 'T',
        artist: 'A',
        sameTrackCount: 1,
      ),
    ]);

    final next = state.apply(
      WsEvent(kind: 'request.accepted', requestId: 'r-1'),
      'new',
    );
    expect(next.items.single.status, 'accepted');
    // Пауза не тронута.
    expect(next.paused, isFalse);
  });

  test('session.paused/resumed управляет баннером', () {
    final state = QueueState(items: const []);
    final paused = state.apply(
      WsEvent(kind: 'session.paused'),
      'new',
    );
    expect(paused.paused, isTrue);
    final resumed = paused.apply(
      WsEvent(kind: 'session.resumed'),
      'new',
    );
    expect(resumed.paused, isFalse);
  });

  test('sort=hot: сортировка по ×N', () {
    final state = QueueState(items: [
      QueueItem(
        id: 'r-1',
        status: 'pending',
        createdAt: DateTime.now(),
        title: 'A',
        artist: '',
        sameTrackCount: 2,
      ),
    ]);
    final next = state.apply(
      WsEvent(
        kind: 'request.created',
        requestId: 'r-2',
        title: 'B',
        sameTrackCount: 7, // горячая песня
      ),
      'hot',
    );
    expect(next.items.first.id, 'r-2', reason: '×7 выше ×2 (hot)');
    expect(next.items.first.sameTrackCount, 7);
  });

  test('мок-стрим WsClient: injectEvent доходит до слушателя', () async {
    // Контракт: события транслируются всем подписчикам.
    final api = ApiClient();
    final ws = WsClient(api);
    final received = <String>[];
    final sub = ws.events.listen((e) => received.add(e.kind));
    ws.onFrame('{"type":"request.created","request_id":"r-1","title":"T"}');
    ws.onFrame('not json — игнор');
    ws.onFrame('{"type":"request.played","request_id":"r-1"}');
    await Future<void>.delayed(Duration.zero);
    expect(received, ['request.created', 'request.played']);
    await sub.cancel();
    ws.dispose();
  });

  testWidgets('UI: карточка получает статус из live-события', (tester) async {
    final state = QueueState(items: [
      QueueItem(
        id: 'r-1',
        status: 'pending',
        createdAt: DateTime.now(),
        title: 'Vivir Mi Vida',
        artist: 'Marc Anthony',
        sameTrackCount: 3,
      ),
    ]);
    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData.dark(),
        home: Scaffold(
          body: Builder(
            builder: (context) {
              final next = state.apply(
                WsEvent(kind: 'request.accepted', requestId: 'r-1'),
                'new',
              );
              return ListView(
                children: next.items
                    .map((i) => ListTile(
                          title: Text(i.title),
                          trailing: Text(i.status.toUpperCase()),
                        ))
                    .toList(),
              );
            },
          ),
        ),
      ),
    );
    expect(find.text('Vivir Mi Vida'), findsOneWidget);
    expect(find.text('ACCEPTED'), findsOneWidget);
  });
}
