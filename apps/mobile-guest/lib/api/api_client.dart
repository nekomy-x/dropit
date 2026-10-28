import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:http/http.dart' as http;

/// API-клиент гостя. Контракты — docs/openapi.yaml (F1–F5).
///
/// Аутентификация гостя: device-token (UUIDv7) в Bearer — НЕ JWT
/// (техкарта Часть I §3). Токен выдаётся сервером при первом входе
/// и сохраняется в platform keystore через flutter_secure_storage.
class ApiClient {
  ApiClient(this._secureStorage);

  final FlutterSecureStorage _secureStorage;
  static const _deviceTokenKey = 'dropit_device_token';

  /// Base URL: задаётся через `--dart-define=DROPIT_API_URL=...`.
  /// 10.0.2.2 = хост-localhost из Android-эмулятора. API слушает 8080.
  String get baseUrl => const String.fromEnvironment(
        'DROPIT_API_URL',
        defaultValue: 'http://10.0.2.2:8080',
      );

  /// Выдаёт device-token, если ещё нет.
  Future<String> ensureDeviceToken() async {
    final existing = await _secureStorage.read(key: _deviceTokenKey);
    if (existing != null) return existing;
    final token = await _issueDeviceToken();
    await _secureStorage.write(key: _deviceTokenKey, value: token);
    return token;
  }

  Future<String> _issueDeviceToken() async {
    final resp = await _post(
      '/v1/guest/device',
      {
        'platform':
            defaultTargetPlatform == TargetPlatform.android ? 'android' : 'ios',
        'app_version': '0.1.0',
      },
    );
    if (resp.statusCode != 200) {
      throw ApiException(_parseError(resp));
    }
    return (jsonDecode(resp.body) as Map<String, dynamic>)['device_token']
        as String;
  }

  /// POST /v1/sessions/join: скан QR → session info + грант.
  Future<JoinResponse> joinByQr(String qrToken) async {
    final resp = await _post('/v1/sessions/join', {'qr_token': qrToken});
    if (resp.statusCode != 200) {
      throw ApiException(_parseError(resp));
    }
    return JoinResponse.fromJson(jsonDecode(resp.body));
  }

  /// GET /v1/catalog/search: живой поиск каталога для экрана Search.
  Future<List<CatalogTrack>> searchCatalog(String query,
      {int limit = 10}) async {
    final encoded = Uri.encodeQueryComponent(query.trim());
    final resp = await _get('/v1/catalog/search?q=$encoded&limit=$limit');
    if (resp.statusCode != 200) {
      throw ApiException(_parseError(resp));
    }
    final body = jsonDecode(resp.body) as Map<String, dynamic>;
    final items = (body['items'] as List<dynamic>? ?? const <dynamic>[]);
    return items
        .map((item) => CatalogTrack.fromJson(item as Map<String, dynamic>))
        .toList();
  }

  /// POST /v1/requests: запрос трека. Idempotency-Key обязателен.
  Future<RequestCreated> createRequest({
    required String trackId,
    String? style,
    required String idempotencyKey,
  }) async {
    final resp = await _post(
      '/v1/requests',
      {'track_id': trackId, if (style != null) 'style': style},
      idempotencyKey: idempotencyKey,
    );
    if (resp.statusCode != 201) {
      throw ApiException(_parseError(resp));
    }
    return RequestCreated.fromJson(jsonDecode(resp.body));
  }

  /// POST /v1/requests/:id/payment: создаёт PaymentIntent.
  Future<PaymentIntent> createPaymentIntent({
    required String requestId,
    required String idempotencyKey,
  }) async {
    final resp = await _post(
      '/v1/requests/$requestId/payment',
      const {},
      idempotencyKey: idempotencyKey,
    );
    if (resp.statusCode != 200 && resp.statusCode != 201) {
      throw ApiException(_parseError(resp));
    }
    return PaymentIntent.fromJson(jsonDecode(resp.body));
  }

  /// POST /v1/payments/:id/confirm: подтверждает оплату.
  Future<PaymentConfirmation> confirmPayment({
    required String paymentId,
    required String idempotencyKey,
    String? paymentMethod,
  }) async {
    final resp = await _post(
      '/v1/payments/$paymentId/confirm',
      {if (paymentMethod != null) 'payment_method': paymentMethod},
      idempotencyKey: idempotencyKey,
    );
    if (resp.statusCode != 200) {
      throw ApiException(_parseError(resp));
    }
    return PaymentConfirmation.fromJson(jsonDecode(resp.body));
  }

  /// GET /v1/my/requests: история + статусы.
  Future<List<GuestRequestItem>> myRequests() async {
    final resp = await _get('/v1/my/requests');
    if (resp.statusCode != 200) {
      throw ApiException(_parseError(resp));
    }
    final items =
        (jsonDecode(resp.body) as Map<String, dynamic>)['items'] as List;
    return items
        .map((e) => GuestRequestItem.fromJson(e as Map<String, dynamic>))
        .toList();
  }

  /// POST /v1/ws/ticket: одноразовый билет WS (гость с JWT не
  /// получается — device-token тоже принимается; см. openapi).
  Future<String> wsTicket() async {
    final resp = await _post('/v1/ws/ticket', {});
    if (resp.statusCode != 200) {
      throw ApiException(_parseError(resp));
    }
    return (jsonDecode(resp.body) as Map<String, dynamic>)['ticket'] as String;
  }

  // ---------------------------------------------------------
  // Хелперы
  // ---------------------------------------------------------

  Future<http.Response> _post(
    String path,
    Map<String, dynamic> body, {
    String? idempotencyKey,
  }) async {
    final headers = {'Content-Type': 'application/json'};
    final token = await _secureStorage.read(key: _deviceTokenKey);
    if (token != null) headers['Authorization'] = 'Bearer $token';
    if (idempotencyKey != null) headers['Idempotency-Key'] = idempotencyKey;
    return http.post(
      Uri.parse('$baseUrl$path'),
      headers: headers,
      body: jsonEncode(body),
    );
  }

  Future<http.Response> _get(String path) async {
    final headers = <String, String>{};
    final token = await _secureStorage.read(key: _deviceTokenKey);
    if (token != null) headers['Authorization'] = 'Bearer $token';
    return http.get(Uri.parse('$baseUrl$path'), headers: headers);
  }

  ErrorEnvelope _parseError(http.Response resp) {
    try {
      return ErrorEnvelope.fromJson(jsonDecode(resp.body));
    } catch (_) {
      return ErrorEnvelope(
        code: 'http_${resp.statusCode}',
        message: 'Unexpected server response',
      );
    }
  }
}

/// Единый envelope ошибок (код = i18n-ключ клиента).
class ErrorEnvelope {
  ErrorEnvelope({required this.code, required this.message, this.details});

  final String code;
  final String message;
  final Map<String, dynamic>? details;

  factory ErrorEnvelope.fromJson(Map<String, dynamic> json) => ErrorEnvelope(
        code: json['code'] as String,
        message: json['message'] as String,
        details: json['details'] as Map<String, dynamic>?,
      );
}

class ApiException implements Exception {
  ApiException(this.envelope);

  /// Convenience: из кода+сообщения (WS-клиент и пр.).
  ApiException.of(String code, String message)
      : envelope = ErrorEnvelope(code: code, message: message, details: null);

  final ErrorEnvelope envelope;

  String get code => envelope.code;
  String get message => envelope.message;

  @override
  String toString() => '[${envelope.code}] ${envelope.message}';
}

/// Ответ POST /v1/sessions/join (openapi: JoinResponse).
class JoinResponse {
  JoinResponse({
    required this.sessionId,
    required this.clubName,
    this.djDisplayName,
    required this.status,
    required this.timezone,
    this.priceMinor,
    required this.grantId,
  });

  final String sessionId;
  final String clubName;
  final String? djDisplayName;
  final String status;
  final String timezone;
  final int? priceMinor;
  final String grantId;

  factory JoinResponse.fromJson(Map<String, dynamic> json) => JoinResponse(
        sessionId: json['session_id'] as String,
        clubName: json['club_name'] as String,
        djDisplayName: json['dj_display_name'] as String?,
        status: json['status'] as String,
        timezone: json['timezone'] as String,
        priceMinor: json['price_minor'] as int?,
        grantId: json['grant_id'] as String,
      );

  /// Цена в € (минорные единицы → валютный формат; ADR-03 — без float
  /// в расчётах, здесь только отображение).
  String get priceLabel =>
      priceMinor == null ? '—' : '€${(priceMinor! / 100).toStringAsFixed(2)}';
}

/// Трек каталога (кэш-результат; поиск-заглушка до B5).
class CatalogTrack {
  CatalogTrack({
    required this.id,
    required this.title,
    required this.artist,
    this.isrc,
    this.coverUrl,
    this.previewUrl,
  });

  final String id;
  final String title;
  final String artist;
  final String? isrc;
  final String? coverUrl;
  final String? previewUrl;

  factory CatalogTrack.fromJson(Map<String, dynamic> json) => CatalogTrack(
        id: json['id']?.toString() ?? '',
        title: json['title'] as String? ?? 'Unknown track',
        artist: json['artist'] as String? ?? 'Unknown artist',
        isrc: json['isrc'] as String?,
        coverUrl: json['artwork_url'] as String?,
        previewUrl: json['preview_url'] as String?,
      );
}

/// Ответ POST /v1/requests.
class RequestCreated {
  RequestCreated({
    required this.requestId,
    required this.status,
    required this.sessionId,
  });

  final String requestId;
  final String status;
  final String sessionId;

  factory RequestCreated.fromJson(Map<String, dynamic> json) => RequestCreated(
        requestId: json['request_id'] as String,
        status: json['status'] as String,
        sessionId: json['session_id'] as String,
      );
}

class PaymentIntent {
  PaymentIntent({
    required this.paymentId,
    required this.requestId,
    required this.amountMinor,
    required this.currency,
    required this.clientSecret,
    required this.status,
  });

  final String paymentId;
  final String requestId;
  final int amountMinor;
  final String currency;
  final String clientSecret;
  final String status;

  factory PaymentIntent.fromJson(Map<String, dynamic> json) => PaymentIntent(
        paymentId: json['payment_id'] as String,
        requestId: json['request_id'] as String,
        amountMinor: json['amount_minor'] as int,
        currency: json['currency'] as String,
        clientSecret: json['client_secret'] as String? ?? '',
        status: json['status'] as String,
      );
}

class PaymentConfirmation {
  PaymentConfirmation({required this.paymentId, required this.status});

  final String paymentId;
  final String status;

  factory PaymentConfirmation.fromJson(Map<String, dynamic> json) =>
      PaymentConfirmation(
        paymentId: json['payment_id'] as String,
        status: json['status'] as String,
      );
}

/// Элемент GET /v1/my/requests (openapi: QueueItem).
class GuestRequestItem {
  GuestRequestItem({
    required this.id,
    required this.status,
    required this.title,
    required this.artist,
    required this.sameTrackCount,
    this.style,
    required this.createdAt,
  });

  final String id;
  final String status;
  final String title;
  final String artist;
  final int sameTrackCount;
  final String? style;
  final DateTime createdAt;

  factory GuestRequestItem.fromJson(Map<String, dynamic> json) =>
      GuestRequestItem(
        id: json['id'] as String,
        status: json['status'] as String,
        title: json['title'] as String,
        artist: json['artist'] as String,
        sameTrackCount: json['same_track_count'] as int,
        style: json['style'] as String?,
        createdAt: DateTime.parse(json['created_at'] as String),
      );
}
