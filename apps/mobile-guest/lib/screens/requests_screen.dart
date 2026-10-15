import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../api/ws_client.dart';
import '../theme/tokens.dart';
import '../widgets/app_button.dart';
import '../widgets/artwork.dart';
import '../widgets/nav.dart';
import 'skeletons.dart';

/// My requests: история запросов гостя и их статусы.
///
/// Оформление — по токенам макета (кадры «Guest · 06 · My requests»):
/// карточка-строка с обложкой 48, названием, метаданными и статусом
/// акцентом/красным. Живые данные — `GET /v1/my/requests`; если API нет
/// (каркас/тесты) показываем демо-список.
class RequestsScreen extends StatefulWidget {
  const RequestsScreen({super.key, required this.session, this.api});

  final JoinResponse session;

  /// API для загрузки и WS-live (без него — демо-список).
  final ApiClient? api;

  @override
  State<RequestsScreen> createState() => _RequestsScreenState();
}

class _RequestsScreenState extends State<RequestsScreen> {
  bool _busy = false;
  List<GuestRequestItem>? _items;
  String? _error;
  Stream<WsEvent>? _live;

  @override
  void initState() {
    super.initState();
    _load();
    _connectLive();
  }

  Future<void> _load() async {
    final api = widget.api;
    if (api == null) {
      setState(() => _items = _demoItems());
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final items = await api.myRequests();
      if (mounted) setState(() => _items = items);
    } on ApiException catch (e) {
      if (mounted) setState(() => _error = e.message);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _connectLive() async {
    final api = widget.api;
    if (api == null) return;
    final ws = WsClient(api);
    final device = api.deviceToken;
    if (device == null) return;
    final ok = await ws.connectGuestLive(device);
    if (ok && mounted) setState(() => _live = ws.events);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: DropTokens.bg,
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(20, 24, 20, 0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const NavRow(title: 'My requests'),
              const SizedBox(height: 24),
              const Text('Your requests.', style: DropType.screenTitle),
              const SizedBox(height: 12),
              Text(
                'Every request you sent tonight — with its status.',
                style: DropType.bodySm.copyWith(color: DropTokens.muted),
              ),
              const SizedBox(height: 20),
              Expanded(child: _body()),
            ],
          ),
        ),
      ),
    );
  }

  Widget _body() {
    if (_busy && _items == null) return const SkeletonList();
    if (_error != null) {
      return ListView(
        children: [
          const SizedBox(height: 60),
          _ErrorNote(message: _error!, onRetry: _load),
        ],
      );
    }
    final items = _items ?? const <GuestRequestItem>[];
    if (items.isEmpty) {
      return ListView(
        children: [
          const SizedBox(height: 80),
          Center(
            child: Text('No requests yet.',
                style: DropType.bodySm.copyWith(color: DropTokens.muted)),
          ),
        ],
      );
    }
    return RefreshIndicator(
      onRefresh: _load,
      color: DropTokens.accent,
      backgroundColor: DropTokens.surface,
      child: ListView.builder(
        padding: const EdgeInsets.only(bottom: 20),
        itemCount: items.length,
        itemBuilder: (_, i) => _RequestCard(
          item: items[i],
          isLast: i == items.length - 1,
        ),
      ),
    );
  }

  static List<GuestRequestItem> _demoItems() => [
        GuestRequestItem(
          id: 'demo-1',
          status: 'pending',
          title: 'Move',
          artist: 'Adam Port, Stryv',
          sameTrackCount: 3,
          style: 'Afro house',
          createdAt: DateTime.now().subtract(const Duration(minutes: 2)),
        ),
        GuestRequestItem(
          id: 'demo-2',
          status: 'accepted',
          title: 'Move Your Body',
          artist: 'Marshall Jefferson',
          sameTrackCount: 1,
          style: null,
          createdAt: DateTime.now().subtract(const Duration(minutes: 12)),
        ),
        GuestRequestItem(
          id: 'demo-3',
          status: 'rejected',
          title: 'One More Time',
          artist: 'Daft Punk',
          sameTrackCount: 1,
          style: null,
          createdAt: DateTime.now().subtract(const Duration(minutes: 25)),
        ),
      ];
}

class _RequestCard extends StatelessWidget {
  const _RequestCard({required this.item, this.isLast = false});

  final GuestRequestItem item;
  final bool isLast;

  @override
  Widget build(BuildContext context) {
    final (color, label) = switch (item.status) {
      'pending' => (DropTokens.muted, 'Waiting for DJ…'),
      'accepted' => (DropTokens.accent, 'Accepted'),
      'rejected' => (DropTokens.red, 'Rejected'),
      'played' => (DropTokens.accent, 'Played'),
      'expired' => (DropTokens.muted, 'Expired'),
      _ => (DropTokens.muted, item.status),
    };
    return Container(
      padding: const EdgeInsets.symmetric(vertical: 14),
      decoration: BoxDecoration(
        border: isLast
            ? null
            : const Border(
                bottom: BorderSide(color: DropTokens.borderSoft),
              ),
      ),
      child: Row(
        children: [
          const Artwork(url: null, size: 48),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(item.title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: DropType.trackTitle),
                const SizedBox(height: 5),
                Text(item.artist,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: DropType.metaSm),
                if (item.sameTrackCount > 1) ...[
                  const SizedBox(height: 6),
                  Text(
                    '×${item.sameTrackCount} requests for this song',
                    style: DropType.metaSm.copyWith(color: DropTokens.accent),
                  ),
                ],
              ],
            ),
          ),
          const SizedBox(width: 8),
          Text(
            label,
            style: DropType.caption.copyWith(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: color,
            ),
          ),
        ],
      ),
    );
  }
}

class _ErrorNote extends StatelessWidget {
  const _ErrorNote({required this.message, this.onRetry});

  final String message;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: DropTokens.redSoft,
        borderRadius: BorderRadius.circular(DropTokens.rMd),
        border: Border.all(color: DropTokens.red),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Icon(Icons.error_outline, size: 24, color: DropTokens.red),
          const SizedBox(height: 8),
          Text(
            'Can’t reach the booth',
            style: DropType.bodySm.copyWith(
              fontWeight: FontWeight.w700,
              color: DropTokens.red,
            ),
          ),
          const SizedBox(height: 8),
          Text(message, style: DropType.caption),
          const SizedBox(height: 12),
          AppButton(
            label: 'Retry',
            height: 44,
            trailingArrow: false,
            variant: AppButtonVariant.outline,
            onPressed: onRetry,
          ),
        ],
      ),
    );
  }
}
