import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import 'api_client.dart';

/// WS-клиент гостя: live-статусы своих запросов (gateway-ws 0.7.0).
///
/// Гость аутентифицируется device-token (не JWT) — билет выдаётся
/// JWT-субъектам; guest-канал авторизуется по id == subject билета.
/// В каркасе: подключение к guest-каналу своего device-id;
/// транспорт — web_socket_channel в прод-сборке (мок в тестах).
class WsClient {
  WsClient(this.api);

  final ApiClient api;

  final _events = StreamController<WsEvent>.broadcast();
  StreamSubscription<String>? _socketSub;
  dynamic _channel; // WebSocketChannel (web_socket_channel; dynamic —
  // зависимость подключается в прод-сборке, каркас не тянет её).

  /// Поток событий (broadcast: несколько слушателей).
  Stream<WsEvent> get events => _events.stream;

  /// Подключение к своему guest-каналу.
  Future<void> connectGuest(String deviceId) =>
      _connect({'subscribe': 'guest', 'id': deviceId});

  /// Безопасное подключение каркаса: false → экран работает
  /// без live-обновлений (ручной refresh).
  Future<bool> connectGuestLive(String deviceId) async {
    try {
      await connectGuest(deviceId);
      return true;
    } on UnimplementedError {
      return false;
    }
  }

  Future<void> _connect(Map<String, dynamic> subscribeFrame) async {
    // Гость: device-token (не JWT). Билет на device-гранте —
    // gateway-ws разрешает guest-канал только с id == субъекту.
    final token = api.deviceToken;
    if (token == null) {
      throw ApiException.of('unauthorized', 'device token required');
    }
    // Одноразовый билет.
    final resp = await http.post(
      Uri.parse('${api.baseUrl}/v1/ws/ticket'),
      headers: {
        'Content-Type': 'application/json',
        'Authorization': 'Bearer $token',
      },
      body: '{}',
    );
    if (resp.statusCode != 200) {
      throw ApiException.of('http_${resp.statusCode}', 'ticket failed');
    }
    final ticket =
        (jsonDecode(resp.body) as Map<String, dynamic>)['ticket'] as String;
    // ws:// (dev; прод — wss за TLS-терминацией).
    final wsBase = api.baseUrl.replaceFirst('http', 'ws');
    final uri = Uri.parse('$wsBase/v1/ws?ticket=$ticket');
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
