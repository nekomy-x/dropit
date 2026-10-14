import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import 'api_client.dart';

/// WS-клиент DJ: live-очередь (gateway-ws, openapi 0.7.0).
///
/// Флоу: JWT → POST /v1/ws/ticket (одноразовый, TTL 30 c) →
/// WS-соединение → первый кадр-подписка
/// {"subscribe":"queue","id":"<session>"} → сервер шлёт события
/// (request.created с title и ×N, accepted/rejected/played,
/// session.paused/resumed).
///
/// Транспорт: web_socket_channel (WebSocketChannel.connect) —
/// подключается в connectSocket; в widget-тестах подменяется моком.
class WsClient {
  WsClient(this.api);

  final ApiClient api;

  final _events = StreamController<WsEvent>.broadcast();
  StreamSubscription<String>? _socketSub;
  dynamic _channel; // WebSocketChannel (web_socket_channel; dynamic —
  // зависимость подключается в прод-сборке, каркас не тянет её).

  /// Поток событий (broadcast: несколько слушателей).
  Stream<WsEvent> get events => _events.stream;

  /// Подключение к queue-каналу сессии DJ.
  Future<void> connectQueue(String sessionId) =>
      _connect({'subscribe': 'queue', 'id': sessionId});

  /// Подключение, безопасное для каркаса: возвращает false, если
  /// транспорт недоступен (web_socket_channel не подключён), — экран
  /// деградирует на pull-to-refresh.
  Future<bool> connectQueueLive(String sessionId) async {
    try {
      await connectQueue(sessionId);
      return true;
    } on UnimplementedError {
      return false; // каркас: транспорт в прод-сборке
    }
  }

  Future<void> _connect(Map<String, dynamic> subscribeFrame) async {
    final token = api.accessToken;
    if (token == null) {
      throw ApiException('unauthorized', 'login required for ws ticket');
    }
    // Одноразовый билет.
    final resp = await http.post(
      Uri.parse('${api.baseUrl}/v1/ws/ticket'),
      headers: {
        'Content-Type': 'application/json',
        'Authorization': 'Bearer $token',
      },
    );
    if (resp.statusCode != 200) {
      throw ApiException('http_${resp.statusCode}', 'ticket failed');
    }
    final ticket =
        (jsonDecode(resp.body) as Map<String, dynamic>)['ticket'] as String;
    // ws:// (dev; прод — wss за TLS-терминацией).
    final uri = Uri.parse(api.baseUrl.replaceFirst('http', 'ws'))
        .replace(path: '/v1/ws', queryParameters: {'ticket': ticket});
    await connectSocket(uri, subscribeFrame);
  }

  /// Транспорт: WebSocketChannel (прод) / мок (тесты).
  Future<void> connectSocket(
    Uri uri,
    Map<String, dynamic> subscribeFrame,
  ) async {
    // ЗАГЛУШКА-КАРКАС (правило 5): транспорт подключается пакетом
    // web_socket_channel при сборке под устройство; мок-тесты
    // подменяют этот метод. Контракт кадра зафиксирован.
    throw UnimplementedError('connectSocket: web_socket_channel in prod build');
  }

  /// Обработка текстового кадра от сокета (вызывается транспортом).
  void onFrame(String text) {
    try {
      final raw = jsonDecode(text) as Map<String, dynamic>;
      _events.add(WsEvent.fromJson(raw));
    } catch (_) {
      // Не-JSON кадр — игнор.
    }
  }

  /// Отправка кадра подписки (после апгрейда).
  void sendSubscribe(dynamic sink, Map<String, dynamic> frame) {
    // ignore: avoid_dynamic_calls
    sink.sink.add(jsonEncode(frame));
  }

  void dispose() {
    _socketSub?.cancel();
    _events.close();
  }
}

/// Событие WS (контракт ws.rs WsEvent).
class WsEvent {
  WsEvent({
    required this.kind,
    this.sessionId,
    this.requestId,
    this.guestDeviceId,
    this.title,
    this.status,
    this.reason,
    this.sameTrackCount,
  });

  /// request.created | request.accepted | request.rejected |
  /// request.played | session.paused | session.resumed
  final String kind;
  final String? sessionId;
  final String? requestId;
  final String? guestDeviceId;
  final String? title;
  final String? status;
  final String? reason;
  final int? sameTrackCount;

  factory WsEvent.fromJson(Map<String, dynamic> json) => WsEvent(
        kind: json['type'] as String,
        sessionId: json['session_id'] as String?,
        requestId: json['request_id'] as String?,
        guestDeviceId: json['guest_device_id'] as String?,
        title: json['title'] as String?,
        status: json['status'] as String?,
        reason: json['reason'] as String?,
        sameTrackCount: json['same_track_count'] as int?,
      );
}
