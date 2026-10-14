import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../api/ws_client.dart';
import '../main.dart';
import 'skeletons.dart';

/// My requests: история + статусы гостя (обновление вручную;
/// WS-обновления — gateway-ws, F9-инфраструктура).
class RequestsScreen extends StatefulWidget {
  const RequestsScreen({super.key, required this.session, this.api});

  final JoinResponse session;

  /// API для WS-live-подключения (опционально: без него —
  /// ручной refresh).
  final ApiClient? api;

  @override
  State<RequestsScreen> createState() => _RequestsScreenState();
}

class _RequestsScreenState extends State<RequestsScreen> {
  bool _busy = false;
  List<GuestRequestItem>? _items;
  String? _error;
  // WS-live: accepted/rejected (с причиной) прилетают без refresh.
  Stream<WsEvent>? _live;

  // Демо-состояния (ЗАГЛУШКА, правило 5): до подключения живого API —
  // после integration-тестов данные придут из /v1/my/requests.
  // TODO(B5/F9): api.myRequests() при живом каталоге.

  @override
  void initState() {
    super.initState();
    _connectLive();
  }

  /// Live-статусы: guest-канал (транспорт — web_socket_channel в
  /// прод-сборке; в каркасе мок-события в тестах).
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
      appBar: AppBar(title: const Text('My requests')),
      body: SafeArea(
        child: _busy && _items == null
            ? const SkeletonList() // скелетоны вместо спиннера (§5)
            : _error != null
                ? Center(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Text(_error!,
                            style: TextStyle(color: DropItTheme.red)),
                        const SizedBox(height: 16),
                        OutlinedButton(onPressed: () {}, child: const Text('Retry')),
                      ],
                    ),
                  )
                : _demoList(context),
      ),
    );
  }

  Widget _demoList(BuildContext context) {
    final demo = [
      GuestRequestItem(
        id: 'demo-1',
        status: 'pending',
        title: 'Vivir Mi Vida',
        artist: 'Marc Anthony',
        sameTrackCount: 3,
        style: 'salsa',
        createdAt: DateTime.now().subtract(const Duration(minutes: 2)),
      ),
      GuestRequestItem(
        id: 'demo-2',
        status: 'accepted',
        title: 'Bailando',
        artist: 'Enrique Iglesias',
        sameTrackCount: 1,
        style: null,
        createdAt: DateTime.now().subtract(const Duration(minutes: 12)),
      ),
      GuestRequestItem(
        id: 'demo-3',
        status: 'rejected',
        title: 'Gasolina',
        artist: 'Daddy Yankee',
        sameTrackCount: 1,
        style: null,
        createdAt: DateTime.now().subtract(const Duration(minutes: 25)),
      ),
    ];
    return ListView.builder(
      padding: const EdgeInsets.all(16),
      itemCount: demo.length,
      itemBuilder: (_, i) => _RequestCard(item: demo[i]),
    );
  }
}

class _RequestCard extends StatelessWidget {
  const _RequestCard({required this.item});

  final GuestRequestItem item;

  @override
  Widget build(BuildContext context) {
    final (color, label) = switch (item.status) {
      'pending' => (DropItTheme.grey, 'Waiting for DJ…'),
      'accepted' => (DropItTheme.neon, 'Accepted'),
      'rejected' => (DropItTheme.red, 'Rejected'),
      'played' => (DropItTheme.neon, 'Played'),
      'expired' => (DropItTheme.grey, 'Expired'),
      _ => (DropItTheme.grey, item.status),
    };
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Row(
          children: [
            Container(
              width: 48,
              height: 48,
              decoration: BoxDecoration(
                color: DropItTheme.surface,
                borderRadius: BorderRadius.circular(8),
              ),
              child: const Icon(Icons.music_note, color: DropItTheme.grey),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(item.title,
                      style: const TextStyle(fontWeight: FontWeight.w700)),
                  Text(item.artist,
                      style: TextStyle(color: DropItTheme.grey)),
                  const SizedBox(height: 4),
                  if (item.sameTrackCount > 1)
                    Text(
                      '×${item.sameTrackCount} requests for this song',
                      style: TextStyle(color: DropItTheme.neon, fontSize: 12),
                    ),
                ],
              ),
            ),
            const SizedBox(width: 8),
            Text(label, style: TextStyle(color: color, fontSize: 12)),
          ],
        ),
      ),
    );
  }
}
