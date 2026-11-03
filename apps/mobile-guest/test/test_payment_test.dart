import 'dart:convert';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:dropit_guest/api/api_client.dart';
import 'package:dropit_guest/data/models.dart';
import 'package:dropit_guest/state/app_state.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUp(() => FlutterSecureStorage.setMockInitialValues({}));

  test(
      'server test method creates the same request and explicit no-charge receipt',
      () async {
    final calls = <String>[];
    var authorizations = 0;
    await http.runWithClient(() async {
      final state = AppState(api: ApiClient(const FlutterSecureStorage()));
      await state.probeBackend();
      await state.joinByQr('ab' * 32);
      expect(state.session!.testPayments, isTrue);
      final result = await state.submitRequest(
          const Track(
            id: '019a0000-0000-7000-8000-000000000002',
            title: 'Track',
            artist: 'Artist',
            genre: 'House',
            art: '',
            priceLabel: '3,00 €',
          ),
          state.session!.room, onPaymentIntent: (intent) async {
        expect(intent.testMode, isTrue);
        authorizations++;
      });
      expect(authorizations, 1);
      expect(result.paymentLabel, 'Test payment · Not charged');
      expect(result.status, RequestStatus.awaiting);
      expect(calls.where((path) => path.endsWith('/test-confirm')).length, 1);
      expect(calls.where((path) => path.endsWith('/confirm')), isEmpty);
      state.dispose();
    },
        () => MockClient((request) async {
              final path = request.url.path;
              calls.add(path);
              Object body;
              if (path.endsWith('/device')) {
                body = {'device_token': '019a0000-0000-7000-8000-000000000001'};
              } else if (path.endsWith('/join')) {
                body = {
                  'test_payments': true,
                  'session_id': '019a0000-0000-7000-8000-000000000003',
                  'club_name': 'Test room',
                  'status': 'active',
                  'timezone': 'Europe/Berlin',
                  'price_minor': 300,
                  'grant_id': '019a0000-0000-7000-8000-000000000004'
                };
              } else if (path.endsWith('/requests')) {
                body = {
                  'request_id': '019a0000-0000-7000-8000-000000000005',
                  'status': 'pending',
                  'session_id': '019a0000-0000-7000-8000-000000000003'
                };
              } else if (path.endsWith('/payment')) {
                body = {
                  'test_mode': true,
                  'payment_id': '019a0000-0000-7000-8000-000000000006',
                  'request_id': '019a0000-0000-7000-8000-000000000005',
                  'client_secret': 'pi_mock_test_secret_mock',
                  'amount_minor': 300,
                  'currency': 'EUR',
                  'status': 'intent'
                };
              } else {
                expect(path.endsWith('/test-confirm'), isTrue);
                body = {
                  'test_mode': true,
                  'payment_id': '019a0000-0000-7000-8000-000000000006',
                  'status': 'captured'
                };
              }
              return http.Response(
                  jsonEncode(body),
                  path.endsWith('/requests') || path.endsWith('/payment')
                      ? 201
                      : 200);
            }));
  });

  test('a generic captured response cannot impersonate a test confirmation',
      () async {
    await http.runWithClient(() async {
      final api = ApiClient(const FlutterSecureStorage());
      await expectLater(
          api.confirmPayment(
              paymentId: 'payment', idempotencyKey: 'key', testMode: true),
          throwsA(isA<ApiException>()));
    },
        () => MockClient((_) async => http.Response(
            '{"payment_id":"payment","status":"captured"}', 200)));
  });
}
