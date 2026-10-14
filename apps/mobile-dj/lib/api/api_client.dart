import 'dart:convert';

import 'package:http/http.dart' as http;

/// API-клиент DJ. Контракты — docs/openapi.yaml (F1, F4, панели).
///
/// Аутентификация: Bearer JWT (TokenPair; 2FA обязателен если секрет
/// задан — ответ шага 1 TotpChallenge {mfa_required}). Токены в памяти
/// (прод: secure storage; refresh-ротация — см. F1).
class ApiClient {
  String baseUrl = 'http://127.0.0.1:8080';

  String? _accessToken;
  String? _refreshToken;

  String? get accessToken => _accessToken;

  // ---------------------------------------------------------
  // Логин (2FA)
  // ---------------------------------------------------------

  /// Шаг 1: POST /v1/auth/dj/login. Возвращает true если сразу вошёл
  /// (2FA не настроена), false — нужен шаг 2 (код из authenticator).
  Future<bool> login(String email, String password) async {
    final resp = await _post('/v1/auth/dj/login', {
      'email': email,
      'password': password,
    });
    _throwIfNotOk(resp, 200);
    final body = jsonDecode(resp.body) as Map<String, dynamic>;
    if (body['mfa_required'] == true) {
      return false; // шаг 2
    }
    _storePair(body);
    return true;
  }

  /// Шаг 2: POST /v1/auth/dj/2fa {email, totp_code}.
  Future<void> login2fa(String email, String totpCode) async {
    final resp = await _post('/v1/auth/dj/2fa', {
      'email': email,
      'totp_code': totpCode,
    });
    _throwIfNotOk(resp, 200);
    _storePair(jsonDecode(resp.body) as Map<String, dynamic>);
  }

  void _storePair(Map<String, dynamic> body) {
    _accessToken = body['access_token'] as String?;
    _refreshToken = body['refresh_token'] as String?;
  }

  // ---------------------------------------------------------
  // Очередь (F4)
  // ---------------------------------------------------------

  /// GET /v1/dj/queue?sort=new|hot.
  Future<List<QueueItem>> queue({String sort = 'new'}) async {
    final resp = await _get('/v1/dj/queue?sort=$sort');
    _throwIfNotOk(resp, 200);
    final items =
        (jsonDecode(resp.body) as Map<String, dynamic>)['items'] as List;
    return items.map((e) => QueueItem.fromJson(e as Map<String, dynamic>)).toList();
  }

  /// POST /v1/dj/requests/:id/accept. No-op-семантика дубля — 200.
  Future<ActionResult> accept(String requestId) => _action(requestId, 'accept');

  /// POST /v1/dj/requests/:id/reject {reason} — шаблон причины гостю.
  Future<ActionResult> reject(String requestId, String? reason) async {
    final resp = await _post(
      '/v1/dj/requests/$requestId/reject',
      reason == null ? <String, dynamic>{} : {'reason': reason},
      auth: true,
      idempotencyKey: 'reject-$requestId',
    );
    _throwIfNotOk(resp, 200);
    return ActionResult.fromJson(jsonDecode(resp.body));
  }

  /// POST /v1/dj/requests/:id/played.
  Future<ActionResult> played(String requestId) => _action(requestId, 'played');

  Future<ActionResult> _action(String requestId, String action) async {
    final resp = await _post(
      '/v1/dj/requests/$requestId/$action',
      {},
      auth: true,
      idempotencyKey: '$action-$requestId',
    );
    _throwIfNotOk(resp, 200);
    return ActionResult.fromJson(jsonDecode(resp.body));
  }

  // ---------------------------------------------------------
  // Приём: пауза/открыт (крупный тумблер §1.3)
  // ---------------------------------------------------------

  Future<void> pauseAcceptance() async {
    final resp = await _post('/v1/dj/acceptance/pause', {}, auth: true);
    _throwIfNotOk(resp, 200);
  }

  Future<void> resumeAcceptance() async {
    final resp = await _post('/v1/dj/acceptance/resume', {}, auth: true);
    _throwIfNotOk(resp, 200);
  }

  // ---------------------------------------------------------
  // Смена и статистика (панельные роуты)
  // ---------------------------------------------------------

  /// GET /v1/dj/shift — текущая сессия (null если нет смены).
  Future<ShiftInfo?> shift() async {
    final resp = await _get('/v1/dj/shift');
    _throwIfNotOk(resp, 200);
    final body = jsonDecode(resp.body) as Map<String, dynamic>;
    final s = body['session'] as Map<String, dynamic>?;
    if (s == null) return null;
    return ShiftInfo.fromJson(s);
  }

  /// GET /v1/dj/stats/today.
  Future<DjStats> statsToday() async {
    final resp = await _get('/v1/dj/stats/today');
    _throwIfNotOk(resp, 200);
    return DjStats.fromJson(jsonDecode(resp.body));
  }

  // ---------------------------------------------------------
  // Хелперы
  // ---------------------------------------------------------

  Future<http.Response> _post(
    String path,
    Map<String, dynamic> body, {
    bool auth = false,
    String? idempotencyKey,
  }) async {
    final headers = {'Content-Type': 'application/json'};
    if (auth && _accessToken != null) {
      headers['Authorization'] = 'Bearer $_accessToken';
    }
    if (idempotencyKey != null) headers['Idempotency-Key'] = idempotencyKey;
    return http.post(
      Uri.parse('$baseUrl$path'),
      headers: headers,
      body: jsonEncode(body),
    );
  }

  Future<http.Response> _get(String path) async {
    final headers = <String, String>{};
    if (_accessToken != null) {
      headers['Authorization'] = 'Bearer $_accessToken';
    }
    return http.get(Uri.parse('$baseUrl$path'), headers: headers);
  }

  void _throwIfNotOk(http.Response resp, int expected) {
    if (resp.statusCode != expected) {
      String code = 'http_${resp.statusCode}';
      String message = 'Request failed';
      try {
        final err = jsonDecode(resp.body) as Map<String, dynamic>;
        code = err['code'] as String? ?? code;
        message = err['message'] as String? ?? message;
      } catch (_) {}
      throw ApiException(code, message);
    }
  }
}

class ApiException implements Exception {
  ApiException(this.code, this.message);

  final String code;
  final String message;

  @override
  String toString() => '[$code] $message';
}

/// Карточка очереди (QueueItemDto в openapi).
class QueueItem {
  QueueItem({
    required this.id,
    required this.status,
    this.style,
    required this.createdAt,
    required this.title,
    required this.artist,
    this.coverUrl,
    this.previewUrl,
    this.isrc,
    required this.sameTrackCount,
  });

  final String id;
  final String status;
  final String? style;
  final DateTime createdAt;
  final String title;
  final String artist;
  final String? coverUrl;
  final String? previewUrl;
  final String? isrc;

  /// Сколько раз эту же песню запросили (социальный сигнал, §1.4).
  final int sameTrackCount;

  factory QueueItem.fromJson(Map<String, dynamic> json) => QueueItem(
        id: json['id'] as String,
        status: json['status'] as String,
        style: json['style'] as String?,
        createdAt: DateTime.parse(json['created_at'] as String),
        title: json['title'] as String,
        artist: json['artist'] as String,
        coverUrl: json['cover_url'] as String?,
        previewUrl: json['preview_url'] as String?,
        isrc: json['isrc'] as String?,
        sameTrackCount: json['same_track_count'] as int,
      );

  /// «2 мин назад» (§1.4).
  String get timeAgo {
    final diff = DateTime.now().difference(createdAt);
    if (diff.inMinutes < 1) return 'just now';
    if (diff.inMinutes < 60) return '${diff.inMinutes} min ago';
    return '${diff.inHours} h ago';
  }
}

class ActionResult {
  ActionResult({
    required this.requestId,
    required this.status,
    required this.noop,
  });

  final String requestId;
  final String status;
  final bool noop;

  factory ActionResult.fromJson(Map<String, dynamic> json) => ActionResult(
        requestId: json['request_id'] as String,
        status: json['status'] as String,
        noop: json['noop'] as bool,
      );
}

class ShiftInfo {
  ShiftInfo({required this.id, required this.status, required this.timezone});

  final String id;
  final String status;
  final String timezone;

  factory ShiftInfo.fromJson(Map<String, dynamic> json) => ShiftInfo(
        id: json['id'] as String,
        status: json['status'] as String,
        timezone: json['timezone'] as String,
      );
}

class DjStats {
  DjStats({
    required this.requestsToday,
    required this.accepted,
    required this.played,
    required this.rejected,
    required this.earnedTotalMinor,
  });

  final int requestsToday;
  final int accepted;
  final int played;
  final int rejected;
  final int earnedTotalMinor;

  factory DjStats.fromJson(Map<String, dynamic> json) => DjStats(
        requestsToday: json['requests_today'] as int,
        accepted: json['accepted'] as int,
        played: json['played'] as int,
        rejected: json['rejected'] as int,
        earnedTotalMinor: json['earned_total_minor'] as int,
      );

  String get earnedLabel =>
      '€${(earnedTotalMinor / 100).toStringAsFixed(2)}';
}
