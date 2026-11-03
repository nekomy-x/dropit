import 'dart:async';
import 'dart:convert';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:dropit_guest/api/api_client.dart';
import 'package:dropit_guest/state/app_state.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUp(() => FlutterSecureStorage.setMockInitialValues({}));

  test('startup probe and cold QR join share one guest identity', () async {
    const identity = '019a0000-0000-7000-8000-000000000001';
    var issued = 0;
    final requested = Completer<void>();
    final release = Completer<void>();
    await http.runWithClient(() async {
      final state = AppState(api: ApiClient(const FlutterSecureStorage()));
      final probe = state.probeBackend();
      final join = state.joinByQr('ab' * 32);
      await requested.future;
      release.complete();
      await Future.wait([probe, join]);
      expect(issued, 1);
      expect(state.backendAvailable, isTrue);
      expect(state.session!.room.name, 'QR room');
      state.dispose();
    }, () => MockClient((request) async {
      if (request.url.path == '/v1/guest/device') {
        issued++;
        if (!requested.isCompleted) requested.complete();
        await release.future;
        return http.Response(jsonEncode({'device_token': identity}), 200);
      }
      expect(request.headers['Authorization'], 'Bearer $identity');
      return http.Response(jsonEncode({
        'session_id': '019a0000-0000-7000-8000-000000000002',
        'club_name': 'QR room', 'status': 'active', 'timezone': 'Europe/Berlin',
        'price_minor': 300, 'grant_id': '019a0000-0000-7000-8000-000000000003',
      }), 200);
    }));
  });

  test('failed device issuance can be retried', () async {
    var attempts = 0;
    await http.runWithClient(() async {
      final api = ApiClient(const FlutterSecureStorage());
      await expectLater(api.ensureDeviceToken(), throwsA(isA<ApiException>()));
      expect(await api.ensureDeviceToken(), 'guest-token');
      expect(attempts, 2);
    }, () => MockClient((_) async {
      attempts++;
      return attempts == 1
          ? http.Response('{"code":"unavailable","message":"Retry"}', 503)
          : http.Response('{"device_token":"guest-token"}', 200);
    }));
  });
}
