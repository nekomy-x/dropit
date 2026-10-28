/// Состояние гостевого приложения.
///
/// Единый источник правды для флоу: выбранный таб, сессия после join,
/// выбранный трек, список запросов, авторизация, способы оплаты.
/// Реализация — [InheritedNotifier], поэтому любые изменения состояния
/// перестраивают оба нужных экрана без внешних зависимостей.
library;

import 'package:flutter/material.dart';

import '../data/demo_data.dart';
import '../data/models.dart';
import '../api/api_client.dart';

class AppState extends ChangeNotifier {
  /// Доступ к состоянию из любого экрана: `AppState.of(context)`.
  static AppState of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<AppStateScope>();
    assert(scope != null, 'AppStateScope не найден в дереве');
    return scope!.notifier!;
  }

  AppState({ApiClient? api}) : _api = api {
    _requests = List.of(DemoData.myRequests);
  }

  final ApiClient? _api;
  _PendingCheckout? _pendingCheckout;

  /// Доступен ли бэкенд (проверяется device-token при старте).
  bool backendAvailable = false;

  // ─── Навигация ────────────────────────────────────────────────────────
  int tab = 0; // 0 Listen · 1 Explore · 2 Requests · 3 Profile

  void setTab(int index) {
    if (tab == index) return;
    tab = index;
    notifyListeners();
  }

  // ─── Сессия (join по QR / кнопке Join) ────────────────────────────────
  JoinSession? _session;

  JoinSession? get session => _session;

  bool get inRoom => _session != null;

  Future<List<Track>> searchTracks(String query) async {
    final api = _api;
    if (api == null || !backendAvailable || query.trim().length < 2) {
      return DemoData.tracks;
    }
    final results = await api.searchCatalog(query);
    final price = _session?.priceLabel ?? DemoData.trackMove.priceLabel;
    return results
        .where((item) => item.id.isNotEmpty)
        .map(
          (item) => Track(
            id: item.id,
            title: item.title,
            artist: item.artist,
            genre: 'Catalog',
            art: item.coverUrl ?? '',
            priceLabel: price,
          ),
        )
        .toList();
  }

  Future<void> refreshRequests() async {
    final api = _api;
    if (api == null || !backendAvailable) return;
    final items = await api.myRequests();
    final localById = {
      for (final request in _requests)
        request.id.replaceFirst('#', '').toLowerCase(): request,
    };
    _requests = items.map((item) {
      final shortId =
          item.id.substring(0, item.id.length < 8 ? item.id.length : 8);
      final local = localById[shortId.toLowerCase()];
      final status = switch (item.status) {
        'pending' || 'awaiting' => RequestStatus.awaiting,
        'accepted' => RequestStatus.queue,
        'played' => RequestStatus.played,
        'rejected' || 'declined' || 'expired' => RequestStatus.declined,
        _ => local?.status ?? RequestStatus.awaiting,
      };
      return GuestRequest(
        id: '#${shortId.toUpperCase()}',
        track: local?.track ??
            Track(
              id: item.id,
              title: item.title,
              artist: item.artist,
              genre: item.style ?? 'Catalog',
              art: '',
              priceLabel: DemoData.trackMove.priceLabel,
            ),
        room: local?.room ?? _session?.room.name ?? 'Club',
        status: status,
        timeLabel:
            '${item.createdAt.hour.toString().padLeft(2, '0')}:${item.createdAt.minute.toString().padLeft(2, '0')}',
        priceLabel: local?.priceLabel ?? DemoData.trackMove.priceLabel,
        paymentLabel: local?.paymentLabel ?? 'Status unavailable',
      );
    }).toList();
    notifyListeners();
  }

  void joinRoom(Room room) {
    _session = JoinSession(
      room: room,
      dj: room.dj,
      priceLabel: DemoData.trackMove.priceLabel,
    );
    notifyListeners();
  }

  /// Join the real room represented by a QR token. The local room model keeps
  /// the server session id, so subsequent request calls never rely on demo
  /// identifiers.
  Future<Room> joinByQr(String qrToken) async {
    final api = _api;
    if (api == null) {
      throw ApiException.of('api_unavailable', 'API is unavailable');
    }
    await api.ensureDeviceToken();
    final joined = await api.joinByQr(qrToken);
    final room = Room(
      id: joined.sessionId,
      name: joined.clubName,
      session: 'Live floor',
      dj: joined.djDisplayName ?? 'DJ on shift',
      genre: 'Live set',
      distance: 'Now',
      image: DemoData.katerBlau.image,
      openNow: joined.status == 'active',
    );
    _session = JoinSession(
      room: room,
      dj: room.dj,
      priceLabel: joined.priceLabel,
    );
    notifyListeners();
    return room;
  }

  void leaveRoom() {
    _session = null;
    _selectedTrack = null;
    notifyListeners();
  }

  // ─── Выбранный трек ───────────────────────────────────────────────────
  Track? _selectedTrack;

  Track? get selectedTrack => _selectedTrack;

  void selectTrack(Track track) {
    _selectedTrack = track;
    notifyListeners();
  }

  // ─── Запросы ──────────────────────────────────────────────────────────
  late List<GuestRequest> _requests;

  List<GuestRequest> get requests => _requests;

  int countByStatus(RequestStatus status) =>
      _requests.where((r) => r.status == status).length;

  List<GuestRequest> filteredRequests(RequestStatus? filter) => filter == null
      ? _requests
      : _requests.where((r) => r.status == filter).toList();

  /// Creates a backend request and payment, or a local demo request offline.
  Future<GuestRequest> submitRequest(
    Track track,
    Room room, {
    Future<void> Function(PaymentIntent intent)? onPaymentIntent,
  }) async {
    if (backendAvailable && _api != null) {
      if (!_looksLikeUuid(track.id)) {
        throw ApiException.of(
          'track_not_synced',
          'Search the live catalog before sending a request.',
        );
      }
      var pending = _pendingCheckout;
      if (pending != null &&
          (pending.trackId != track.id || pending.sessionId != room.id)) {
        throw ApiException.of(
          'checkout_in_progress',
          'Finish or retry the pending checkout before starting another request.',
        );
      }
      if (pending == null) {
        final key = 'guest-request-${DateTime.now().microsecondsSinceEpoch}';
        final request = await _api.createRequest(
          trackId: track.id,
          idempotencyKey: key,
        );
        final intent = await _api.createPaymentIntent(
          requestId: request.requestId,
          idempotencyKey: 'payment-$key',
        );
        pending = _PendingCheckout(
          trackId: track.id,
          sessionId: room.id,
          requestId: request.requestId,
          paymentIntent: intent,
          idempotencyKey: key,
        );
        _pendingCheckout = pending;
      }
      if (onPaymentIntent == null) {
        throw ApiException.of(
          'payment_method_required',
          'A secure payment method is required to send this request.',
        );
      }
      if (!pending.paymentAuthorized) {
        await onPaymentIntent(pending.paymentIntent);
        pending.paymentAuthorized = true;
      }
      final confirmation = await _api.confirmPayment(
        paymentId: pending.paymentIntent.paymentId,
        idempotencyKey: 'confirm-${pending.idempotencyKey}',
      );
      final created = GuestRequest(
        id: '#${pending.requestId.substring(0, 8).toUpperCase()}',
        track: track,
        room: room.name,
        status: confirmation.status == 'processing'
            ? RequestStatus.awaiting
            : RequestStatus.queue,
        timeLabel: _nowLabel(),
        priceLabel: _formatMinor(pending.paymentIntent.amountMinor),
        paymentLabel: switch (confirmation.status) {
          'processing' => 'Processing',
          'captured' || 'succeeded' => 'Paid',
          _ => confirmation.status,
        },
      );
      _pendingCheckout = null;
      _requests = [created, ..._requests];
      notifyListeners();
      return created;
    }

    final created = GuestRequest(
      id: '#DI-${(285 + _requests.length).toString().padLeft(4, '0')}',
      track: track,
      room: room.name,
      status: RequestStatus.queue,
      timeLabel: _nowLabel(),
      priceLabel: track.priceLabel,
      paymentLabel: 'Not charged · demo',
    );
    _requests = [created, ..._requests];
    notifyListeners();
    return created;
  }

  static String _formatMinor(int minor) =>
      '${(minor ~/ 100)},${(minor % 100).toString().padLeft(2, '0')} €';

  static bool _looksLikeUuid(String value) => RegExp(
        r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$',
      ).hasMatch(value);

  static String _nowLabel() {
    final now = DateTime.now();
    return '${now.hour.toString().padLeft(2, '0')}:'
        '${now.minute.toString().padLeft(2, '0')}';
  }

  // ─── Авторизация ──────────────────────────────────────────────────────
  bool _signedIn = false;

  bool get signedIn => _signedIn;

  void signIn() {
    _signedIn = true;
    notifyListeners();
  }

  void signOut() {
    _signedIn = false;
    notifyListeners();
  }

  // ─── Стартовая проверка бэкенда ────────────────────────────────────────

  /// Пробуем получить device-token. Если сервер недоступен — приложение
  /// остаётся полностью функциональным на део-данных.
  Future<void> probeBackend() async {
    final api = _api;
    if (api == null) return;
    try {
      await api.ensureDeviceToken().timeout(const Duration(seconds: 3));
      backendAvailable = true;
    } catch (_) {
      backendAvailable = false;
    }
    notifyListeners();
  }
}

class _PendingCheckout {
  _PendingCheckout({
    required this.trackId,
    required this.sessionId,
    required this.requestId,
    required this.paymentIntent,
    required this.idempotencyKey,
  });

  final String trackId;
  final String sessionId;
  final String requestId;
  final PaymentIntent paymentIntent;
  final String idempotencyKey;
  bool paymentAuthorized = false;
}

/// Доступ к состоянию из любого экрана: `AppState.of(context)`.
class AppStateScope extends InheritedNotifier<AppState> {
  const AppStateScope({
    super.key,
    required AppState state,
    required super.child,
  }) : super(notifier: state);
}
