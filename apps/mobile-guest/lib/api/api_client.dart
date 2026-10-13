import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:shared_preferences/shared_preferences.dart';

/// API-клиент гостя. Контракты — docs/openapi.yaml (F1–F5).
///
/// Аутентификация гостя: device-token (UUIDv7) в Bearer — НЕ JWT
/// (техкарта Часть I §3). Токен выдаётся сервером при первом входе
/// и сохраняется в platform keystore через SharedPreferences
/// (прод: flutter_secure_storage — TODO F9).
class ApiClient {
  ApiClient(this._prefs);

  final SharedPreferences _prefs;
  static const _deviceTokenKey = 'dropit_device_token';

  /// Base URL: прод — CDN; dev — локальный API.
  String baseUrl = 'http://127.0.0.1:8080';

  String? get deviceToken => _prefs.getString(_deviceTokenKey);

  /// Выдаёт device-token, если ещё нет.
  Future<String> ensureDeviceToken() async {
    final existing = deviceToken;
    if (existing != null) return existing;
    final token = await _issueDeviceToken();
    await _prefs.setString(_deviceTokenKey, token);
    return token;
  }

  Future<String> _issueDeviceToken() async {
    final resp = await _post(
      '/v1/guest/device',
      {'platform': 'ios', 'app_version': '0.1.0'},
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
    final token = deviceToken;
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
    final token = deviceToken;
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

  final ErrorEnvelope envelope;

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

  factory RequestCreated.fromJson(Map<String, dynamic> json) =>
      RequestCreated(
        requestId: json['request_id'] as String,
        status: json['status'] as String,
        sessionId: json['session_id'] as String,
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
