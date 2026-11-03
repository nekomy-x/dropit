import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../api/queue_state.dart';
import '../api/ws_client.dart';
import '../main.dart';
import 'skeletons.dart';

/// Очередь запросов — ГЛАВНЫЙ экран (§1.4).
///
/// Карточка: обложка, название, исполнитель, количество запросов этой
/// же песни (×N — социальный сигнал), время поступления, стиль.
/// Сортировка: новые сверху / по количеству (переключатель).
/// Обновление: pull-to-refresh (WS-обновления — F9-инфраструктура).
class QueueScreen extends StatefulWidget {
  const QueueScreen({super.key, required this.api});

  final ApiClient api;

  @override
  State<QueueScreen> createState() => _QueueScreenState();
}

class _QueueScreenState extends State<QueueScreen> {
  String _sort = 'new';
  List<QueueItem>? _items;
  String? _error;
  bool _busy = false;
  bool _paused = false; // live-баннер паузы (session.paused/resumed)

  // WS-live (gateway-ws): события применяются инкрементально.
  WsClient? _ws;
  Stream<WsEvent>? _wsEvents;

  Future<void> _load() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final items = await widget.api.queue(sort: _sort);
      setState(() => _items = items);
    } on ApiException catch (e) {
      setState(() => _error = e.message);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  void initState() {
    super.initState();
    _load();
    _connectWs();
  }

  /// WS-live: подписка на queue-канал текущей смены.
  Future<void> _connectWs() async {
    try {
      final shift = await widget.api.shift();
      if (shift == null) return; // нет смены — нет подписки
      final ws = WsClient(widget.api);
      final ok = await ws.connectQueueLive(shift.id);
      if (ok) {
        setState(() => _wsEvents = ws.events);
      }
    } catch (_) {
      // WS недоступен (dev без сокет-транспорта) — очередь работает
      // через pull-to-refresh.
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(_paused ? 'Requests — PAUSED' : 'Requests'),
        actions: [
          // Переключатель сортировки: новые / популярные (§1.4).
          SegmentedButton<String>(
            segments: const [
              ButtonSegment(
                  value: 'new', icon: Icon(Icons.schedule), label: Text('New')),
              ButtonSegment(
                  value: 'hot',
                  icon: Icon(Icons.local_fire_department),
                  label: Text('Hot')),
            ],
            selected: {_sort},
            onSelectionChanged: (s) {
              setState(() => _sort = s.first);
              _load();
            },
          ),
          const SizedBox(width: 8),
        ],
      ),
      body: RefreshIndicator(
        onRefresh: _load,
        color: DjTheme.neon,
        child: _body(),
      ),
    );
  }

  Widget _body() {
    if (_busy && _items == null) {
      return const SkeletonList(); // скелетоны очереди (§5)
    }
    if (_error != null) {
      return ListView(
        children: [
          const SizedBox(height: 120),
          Icon(Icons.cloud_off, size: 48, color: DjTheme.red),
          const SizedBox(height: 12),
          Text(_error!, textAlign: TextAlign.center),
          const SizedBox(height: 12),
          Center(
            child: OutlinedButton(onPressed: _load, child: const Text('Retry')),
          ),
        ],
      );
    }
    final items = _items ?? const <QueueItem>[];
    if (items.isEmpty) {
      // Пустая очередь (§1.8).
      return ListView(
        children: const [
          SizedBox(height: 160),
          Icon(Icons.music_off, size: 48, color: DjTheme.grey),
          SizedBox(height: 12),
          Text('No requests yet',
              textAlign: TextAlign.center, style: TextStyle(fontSize: 22)),
        ],
      );
    }
    return ListView.builder(
      padding: const EdgeInsets.all(12),
      itemCount: items.length,
      itemBuilder: (_, i) => _QueueCard(
        item: items[i],
        onOpen: () => _openRequest(items[i]),
      ),
    );
  }

  /// Раскрытие запроса (§1.5): крупное превью действий.
  Future<void> _openRequest(QueueItem item) async {
    await showModalBottomSheet<void>(
      context: context,
      backgroundColor: DjTheme.surface,
      isScrollControlled: true,
      builder: (sheetContext) => _RequestSheet(item: item, api: widget.api),
    );
    _load();
  }
}

class _QueueCard extends StatelessWidget {
  const _QueueCard({required this.item, required this.onOpen});

  final QueueItem item;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: InkWell(
        onTap: onOpen,
        borderRadius: BorderRadius.circular(12),
        child: Padding(
          padding: const EdgeInsets.all(14),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Обложка (нет — иконка; кэш обложек — B5).
              Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Container(
                    width: 56,
                    height: 56,
                    decoration: BoxDecoration(
                      color: DjTheme.black,
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child:
                        const Icon(Icons.album, color: DjTheme.grey, size: 32),
                  ),
                  const SizedBox(height: 8),
                  _statusChip(item.status),
                ],
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(item.title,
                        style: const TextStyle(
                            fontSize: 20, fontWeight: FontWeight.w700)),
                    Text(item.artist,
                        style: TextStyle(color: DjTheme.grey, fontSize: 16)),
                    const SizedBox(height: 4),
                    Wrap(
                      spacing: 8,
                      runSpacing: 4,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        if (item.sameTrackCount > 1) ...[
                          // ×N — выделить: социальный сигнал (§1.4).
                          Container(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 8, vertical: 2),
                            decoration: BoxDecoration(
                              color: DjTheme.neon,
                              borderRadius: BorderRadius.circular(6),
                            ),
                            child: Text(
                              '×${item.sameTrackCount}',
                              style: const TextStyle(
                                color: DjTheme.black,
                                fontWeight: FontWeight.w800,
                                fontSize: 14,
                              ),
                            ),
                          ),
                        ],
                        Text(item.timeAgo,
                            style:
                                TextStyle(color: DjTheme.grey, fontSize: 13)),
                        if (item.style != null) ...[
                          Text('· ${item.style}',
                              style:
                                  TextStyle(color: DjTheme.grey, fontSize: 13)),
                        ],
                      ],
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _statusChip(String status) {
    final (color, label) = switch (status) {
      'pending' => (DjTheme.neon, 'NEW'),
      'accepted' => (DjTheme.grey, 'OK'),
      'rejected' => (DjTheme.red, 'NO'),
      'played' => (DjTheme.grey, 'DONE'),
      _ => (DjTheme.grey, status.toUpperCase()),
    };
    return Text(label,
        style:
            TextStyle(color: color, fontWeight: FontWeight.w800, fontSize: 13));
  }
}

/// Карточка/раскрытие (§1.5): Принять · Отклонить (шаблон причины) ·
/// Сыграна. ISRC — мелко, для копирования в Rekordbox/Serato.
class _RequestSheet extends StatelessWidget {
  const _RequestSheet({required this.item, required this.api});

  final QueueItem item;
  final ApiClient api;

  /// Шаблоны причин отклонения (§1.5, опционально).
  static const _rejectReasons = [
    'Not the right style',
    'Already played tonight',
    'Not the vibe tonight',
  ];

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(item.title,
                style:
                    const TextStyle(fontSize: 26, fontWeight: FontWeight.w800)),
            Text(item.artist,
                style: TextStyle(color: DjTheme.grey, fontSize: 18)),
            const SizedBox(height: 8),
            if (item.sameTrackCount > 1)
              Text('${item.sameTrackCount} people requested this song',
                  style: TextStyle(color: DjTheme.neon, fontSize: 16)),
            const Divider(height: 32),
            if (item.isrc != null)
              Padding(
                padding: const EdgeInsets.only(bottom: 16),
                child: Row(
                  children: [
                    Expanded(
                      child: Text('ISRC: ${item.isrc}',
                          style: TextStyle(color: DjTheme.grey, fontSize: 12)),
                    ),
                    const SizedBox(width: 8),
                    IconButton(
                      onPressed: () => _copy(context, item.isrc!),
                      tooltip: 'Copy ISRC',
                      icon:
                          const Icon(Icons.copy, size: 14, color: DjTheme.grey),
                    ),
                  ],
                ),
              ),
            // Крупные превью-действия, одной рукой (§1.5).
            ElevatedButton.icon(
              icon: const Icon(Icons.check),
              label: const Text('Accept'),
              onPressed: item.status == 'pending'
                  ? () => _run(context, api.accept(item.id))
                  : null,
            ),
            const SizedBox(height: 10),
            OutlinedButton.icon(
              icon: const Icon(Icons.close, color: DjTheme.red),
              label: const Text('Reject', style: TextStyle(color: DjTheme.red)),
              onPressed:
                  item.status == 'pending' ? () => _reject(context) : null,
            ),
            const SizedBox(height: 10),
            ElevatedButton.icon(
              icon: const Icon(Icons.play_arrow),
              label: const Text('Played'),
              style: ElevatedButton.styleFrom(backgroundColor: DjTheme.white),
              onPressed: item.status == 'accepted'
                  ? () => _run(context, api.played(item.id))
                  : null,
            ),
          ],
        ),
      ),
    );
  }

  void _copy(BuildContext context, String text) {
    // TODO(B5/платформа): clipboard — Clipboard.setData.
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('ISRC $text')),
    );
  }

  Future<void> _reject(BuildContext context) async {
    final reason = await showModalBottomSheet<String>(
      context: context,
      backgroundColor: DjTheme.surface,
      builder: (_) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Padding(
              padding: EdgeInsets.all(16),
              child: Text('Reason (sent to guest)',
                  style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
            ),
            ..._rejectReasons.map(
              (r) => ListTile(
                title: Text(r, style: const TextStyle(fontSize: 18)),
                onTap: () => Navigator.pop(context, r),
              ),
            ),
            ListTile(
              title: Text('No reason',
                  style: TextStyle(color: DjTheme.grey, fontSize: 18)),
              onTap: () => Navigator.pop(context, ''),
            ),
          ],
        ),
      ),
    );
    if (reason != null) {
      // ignore: use_build_context_synchronously
      await _run(context, api.reject(item.id, reason.isEmpty ? null : reason));
    }
  }

  Future<void> _run(BuildContext context, Future<ActionResult> future) async {
    try {
      final result = await future;
      if (context.mounted) {
        Navigator.pop(context);
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(result.noop
                ? 'Already done'
                : switch (result.status) {
                    'accepted' => 'Accepted',
                    'rejected' => 'Rejected',
                    'played' => 'Marked as played',
                    _ => result.status,
                  }),
          ),
        );
      }
    } on ApiException catch (e) {
      // 409 — недопустимый переход (accept после played и т.п.).
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(e.code == 'conflict'
                ? 'Cannot do that now (already processed)'
                : e.message),
            backgroundColor: DjTheme.red,
          ),
        );
      }
    }
  }
}
