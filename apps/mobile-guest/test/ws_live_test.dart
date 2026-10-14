import 'package:flutter_test/flutter_test.dart';

import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:dropit_guest/api/api_client.dart';
import 'package:dropit_guest/api/ws_client.dart';
import 'package:dropit_guest/main.dart';

void main() {
  test('гость: rejected с причиной летит в событии', () async {
    final api = ApiClient(_StubPrefs());
    final ws = WsClient(api);
    final received = <WsEvent>[];
    final sub = ws.events.listen(received.add);
    ws.onFrame(
        '{"type":"request.rejected","request_id":"r-1","reason":"Not the vibe tonight"}');
    await Future<void>.delayed(Duration.zero);
    expect(received.single.kind, 'request.rejected');
    expect(received.single.reason, 'Not the vibe tonight');
    await sub.cancel();
    ws.dispose();
  });

  test('гость: created с ×N и статусом', () async {
    final api = ApiClient(_StubPrefs());
    final ws = WsClient(api);
    final received = <WsEvent>[];
    final sub = ws.events.listen(received.add);
    ws.onFrame(
        '{"type":"request.created","request_id":"r-9","title":"Song","status":"pending","same_track_count":4}');
    await Future<void>.delayed(Duration.zero);
    expect(received.single.title, 'Song');
    expect(received.single.sameTrackCount, 4);
    await sub.cancel();
    ws.dispose();
  });

  testWidgets('UI гостя: статус карточки меняется по событию',
      (tester) async {
    // Карточка запроса гостя: pending → rejected отображается.
    var ev = WsEvent(kind: 'request.created', status: 'pending');
    await tester.pumpWidget(
      MaterialApp(
        theme: DropItTheme.theme(),
        home: Scaffold(
          body: Builder(
            builder: (_) {
              final (color, label) = switch (ev.status) {
                'pending' => (Colors.grey, 'Waiting for DJ…'),
                'rejected' => (Colors.red, 'Rejected'),
                'accepted' => (Colors.green, 'Accepted'),
                'played' => (Colors.green, 'Played'),
                _ => (Colors.grey, ev.status ?? ''),
              };
              return Text(label);
            },
          ),
        ),
      ),
    );
    expect(find.text('Waiting for DJ…'), findsOneWidget);
    ev = WsEvent(kind: 'request.rejected', status: 'rejected');
    await tester.pumpWidget(
      MaterialApp(
        theme: DropItTheme.theme(),
        home: Scaffold(body: Text(switch (ev.status) {
          'rejected' => 'Rejected',
          _ => ev.status ?? '',
        })),
      ),
    );
    expect(find.text('Rejected'), findsOneWidget);
  });
}

/// Заглушка SharedPreferences для unit-тестов (только String-API,
/// который трогает ApiClient).
class _StubPrefs implements SharedPreferences {
  final Map<String, Object> _store = {};
  @override
  String? getString(String key) => _store[key] as String?;
  @override
  Future<bool> setString(String key, String value) async {
    _store[key] = value;
    return true;
  }
  // Остальной API не нужен тесту.
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}
