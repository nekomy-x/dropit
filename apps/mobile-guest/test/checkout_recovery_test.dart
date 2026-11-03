import 'dart:async';
import 'dart:convert';

import 'package:dropit_guest/api/api_client.dart';
import 'package:dropit_guest/data/demo_data.dart';
import 'package:dropit_guest/data/models.dart';
import 'package:dropit_guest/state/app_state.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';

const track = Track(
  id: '019a0000-0000-7000-8000-000000000001',
  title: 'Recovery test',
  artist: 'Test',
  genre: 'Test',
  art: '',
  priceLabel: '3,00 €',
);
const requestId = '019a0000-0000-7000-8000-000000000002';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUp(() => FlutterSecureStorage.setMockInitialValues({}));

  test('missing payment method does not create an unpaid backend request',
      () async {
    var calls = 0;
    await http.runWithClient(() async {
      final state = AppState(api: ApiClient(const FlutterSecureStorage()))
        ..backendAvailable = true;
      addTearDown(state.dispose);
      await expectLater(state.submitRequest(track, DemoData.rooms.first),
          throwsA(isA<ApiException>()));
      expect(calls, 0);
    },
        () => MockClient((_) async {
              calls++;
              return responseFor('/v1/requests');
            }));
  });

  test('payment intent retry reuses the already created request', () async {
    var requests = 0;
    var intents = 0;
    final intentKeys = <String?>[];
    await http.runWithClient(() async {
      final state = AppState(api: ApiClient(const FlutterSecureStorage()))
        ..backendAvailable = true;
      addTearDown(state.dispose);
      Future<GuestRequest> submit() =>
          state.submitRequest(track, DemoData.rooms.first,
              onPaymentIntent: (_) async {});
      await expectLater(submit(), throwsA(isA<ApiException>()));
      expect(state.hasPendingCheckout, isTrue);
      final result = await submit();
      expect(result.track.id, track.id);
      expect(requests, 1);
      expect(intents, 2);
      expect(intentKeys[0], intentKeys[1]);
      expect(state.hasPendingCheckout, isFalse);
    },
        () => MockClient((request) async {
              if (request.url.path == '/v1/requests') requests++;
              if (request.url.path.endsWith('/payment')) {
                intents++;
                intentKeys.add(request.headers['Idempotency-Key']);
                if (intents == 1) return unavailable();
              }
              return responseFor(request.url.path);
            }));
  });

  test('concurrent submission cannot create a second backend request',
      () async {
    final started = Completer<void>();
    final release = Completer<void>();
    var requests = 0;
    await http.runWithClient(() async {
      final state = AppState(api: ApiClient(const FlutterSecureStorage()))
        ..backendAvailable = true;
      addTearDown(state.dispose);
      final first = state.submitRequest(track, DemoData.rooms.first,
          onPaymentIntent: (_) async {});
      await started.future;
      final duplicate = state.submitRequest(track, DemoData.rooms.first,
          onPaymentIntent: (_) async {});
      final rejected = expectLater(
          duplicate,
          throwsA(isA<ApiException>()
              .having((e) => e.code, 'code', 'checkout_in_progress')));
      release.complete();
      await first;
      await rejected;
      expect(requests, 1);
    },
        () => MockClient((request) async {
              if (request.url.path == '/v1/requests') {
                requests++;
                if (!started.isCompleted) started.complete();
                await release.future;
              }
              return responseFor(request.url.path);
            }));
  });

  test('ambiguous request failure retries with the same idempotency key',
      () async {
    final keys = <String?>[];
    await http.runWithClient(() async {
      final state = AppState(api: ApiClient(const FlutterSecureStorage()))
        ..backendAvailable = true;
      addTearDown(state.dispose);
      Future<GuestRequest> submit() =>
          state.submitRequest(track, DemoData.rooms.first,
              onPaymentIntent: (_) async {});
      await expectLater(submit(), throwsA(isA<ApiException>()));
      await submit();
      expect(keys, hasLength(2));
      expect(keys.first, isNotEmpty);
      expect(keys.first, keys.last);
    },
        () => MockClient((request) async {
              if (request.url.path == '/v1/requests') {
                keys.add(request.headers['Idempotency-Key']);
                if (keys.length == 1) return unavailable();
              }
              return responseFor(request.url.path);
            }));
  });

  test('rejected request does not lock the guest into an invalid checkout',
      () async {
    var calls = 0;
    await http.runWithClient(() async {
      final state = AppState(api: ApiClient(const FlutterSecureStorage()))
        ..backendAvailable = true;
      addTearDown(state.dispose);
      await expectLater(
          state.submitRequest(track, DemoData.rooms.first,
              onPaymentIntent: (_) async {}),
          throwsA(isA<ApiException>()));
      expect(state.hasPendingCheckout, isFalse);
      await state.submitRequest(
          const Track(
            id: '019a0000-0000-7000-8000-000000000099',
            title: 'Another track',
            artist: 'Test',
            genre: 'Test',
            art: '',
            priceLabel: '3,00 €',
          ),
          DemoData.rooms.first,
          onPaymentIntent: (_) async {});
      expect(calls, 2);
    },
        () => MockClient((request) async {
              if (request.url.path == '/v1/requests' && ++calls == 1) {
                return http.Response(
                    '{"code":"track_not_found","message":"Unavailable"}', 404);
              }
              return responseFor(request.url.path);
            }));
  });

  test('confirmation retry does not authorize payment twice', () async {
    var authorizations = 0;
    var requests = 0;
    var confirmations = 0;
    await http.runWithClient(() async {
      final state = AppState(api: ApiClient(const FlutterSecureStorage()))
        ..backendAvailable = true;
      addTearDown(state.dispose);
      Future<GuestRequest> submit() =>
          state.submitRequest(track, DemoData.rooms.first,
              onPaymentIntent: (_) async {
            authorizations++;
          });
      await expectLater(submit(), throwsA(isA<ApiException>()));
      await submit();
      expect(authorizations, 1);
      expect(requests, 1);
      expect(confirmations, 2);
    },
        () => MockClient((request) async {
              if (request.url.path == '/v1/requests') requests++;
              if (request.url.path.endsWith('/confirm') &&
                  ++confirmations == 1) {
                return unavailable();
              }
              return responseFor(request.url.path);
            }));
  });
}

http.Response unavailable() =>
    http.Response('{"code":"unavailable","message":"Retry"}', 503);

http.Response responseFor(String path) {
  if (path == '/v1/requests') {
    return http.Response(
        jsonEncode({
          'request_id': requestId,
          'session_id': DemoData.rooms.first.id,
          'status': 'pending',
        }),
        201);
  }
  if (path.endsWith('/payment')) {
    return http.Response(
        jsonEncode({
          'payment_id': 'payment-test',
          'request_id': requestId,
          'amount_minor': 300,
          'currency': 'EUR',
          'client_secret': 'test-only',
          'status': 'requires_payment_method',
        }),
        201);
  }
  return http.Response(
      '{"payment_id":"payment-test","status":"captured"}', 200);
}
