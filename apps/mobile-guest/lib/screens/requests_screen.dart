/// Guest · 06 · My requests — табы All / Upcoming / Played / Declined.
///
/// Макет pen.dev: заголовок, фильтры комнат и даты, чипы статусов,
/// список запросов с цветными статусами, карточка причины отказа
/// и пояснение под списком.
library;

import 'package:flutter/material.dart';

import '../data/models.dart';
import '../state/app_state.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class RequestsScreen extends StatefulWidget {
  const RequestsScreen({super.key, required this.onOpenRequest});

  final ValueChanged<GuestRequest> onOpenRequest;

  @override
  State<RequestsScreen> createState() => _RequestsScreenState();
}

class _RequestsScreenState extends State<RequestsScreen> {
  // null = All; «Upcoming» — это queue + awaiting (как в макете: 2).
  List<RequestStatus>? _filter;
  String _room = 'All rooms';
  String _night = 'Tonight';

  static const _upcoming = [RequestStatus.queue, RequestStatus.awaiting];
  static const _tabs = <(List<RequestStatus>?, String)>[
    (null, 'All'),
    (_upcoming, 'Upcoming'),
    ([RequestStatus.played], 'Played'),
    ([RequestStatus.declined], 'Declined'),
  ];

  String get _explanation {
    final f = _filter;
    if (f == null) return '';
    if (f.contains(RequestStatus.queue)) {
      return 'The DJ sets the order. Your position may change.';
    }
    if (f.contains(RequestStatus.played)) {
      return 'Your picks made it to the dance floor.';
    }
    if (f.contains(RequestStatus.declined)) {
      return 'Declined requests are refunded to your original payment method.';
    }
    return '';
  }

  Color _statusColor(RequestStatus status) => switch (status) {
        RequestStatus.queue => DropTokens.statusQueue,
        RequestStatus.awaiting => DropTokens.statusAwait,
        RequestStatus.played => DropTokens.statusPlayed,
        RequestStatus.declined => DropTokens.statusDeclined,
      };

  @override
  Widget build(BuildContext context) {
    final state = AppState.of(context);
    final visible = state.requests
        .where((r) =>
            (_filter == null || _filter!.contains(r.status)) &&
            (_room == 'All rooms' || r.room == _room))
        .toList();

    return DropScaffold(
      body: RefreshIndicator(
        onRefresh: state.refreshRequests,
        child: ListView(
          padding: EdgeInsets.zero,
          children: [
            const SizedBox(height: 24),
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: 20),
              child: BrandCityRow(),
            ),
            const SizedBox(height: 18),
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: 20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('My requests', style: DropType.screenTitle),
                  SizedBox(height: 6),
                  Text(
                    'Your picks. From the queue to the dance floor.',
                    style: DropType.caption,
                  ),
                ],
              ),
            ),
            const SizedBox(height: 20),

            // ── Фильтры комнат и даты ──
            SizedBox(
              height: 36,
              child: ListView(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(horizontal: 20),
                children: [
                  _FilterSelect(
                    icon: Icons.place_outlined,
                    value: _room,
                    options: const ['All rooms', 'Kater Blau'],
                    onSelected: (v) => setState(() => _room = v),
                  ),
                  const SizedBox(width: 8),
                  _FilterSelect(
                    icon: Icons.today_outlined,
                    value: _night,
                    options: const ['Tonight', 'All time'],
                    onSelected: (v) => setState(() => _night = v),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 12),

            // ── Чипы статусов ──
            SizedBox(
              height: 36,
              child: ListView(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(horizontal: 20),
                children: [
                  for (final (statuses, label) in _tabs) ...[
                    FilterChipPill(
                      label: '$label · ${_count(state, statuses)}',
                      active: _filter == statuses ||
                          (_filter != null &&
                              statuses != null &&
                              _filter!.length == statuses.length &&
                              _filter!.every(statuses.contains)),
                      onTap: () => setState(() => _filter = statuses),
                    ),
                    const SizedBox(width: 8),
                  ],
                ],
              ),
            ),
            const SizedBox(height: 20),

            // ── Сводка списка ──
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text('${visible.length} REQUESTS',
                      style: DropType.labelTight),
                  Text('Newest first ↓',
                      style: DropType.caption.copyWith(color: DropTokens.text)),
                ],
              ),
            ),
            const SizedBox(height: 12),

            // ── Список запросов ──
            for (final request in visible) ...[
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                child: _RequestCard(
                  request: request,
                  statusColor: _statusColor(request.status),
                  onTap: () => widget.onOpenRequest(request),
                ),
              ),
              const SizedBox(height: 10),
            ],

            // ── Пояснение статуса ──
            if (_explanation.isNotEmpty)
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 6, 20, 0),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Icon(Icons.info_outline,
                        size: 15, color: DropTokens.muted),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        _explanation,
                        style: DropType.caption.copyWith(height: 1.5),
                      ),
                    ),
                  ],
                ),
              ),
            const SizedBox(height: 24),
          ],
        ),
      ),
    );
  }

  int _count(AppState state, List<RequestStatus>? statuses) => statuses == null
      ? state.requests.length
      : state.requests.where((r) => statuses.contains(r.status)).length;
}

class _FilterSelect extends StatelessWidget {
  const _FilterSelect({
    required this.icon,
    required this.value,
    required this.options,
    required this.onSelected,
  });

  final IconData icon;
  final String value;
  final List<String> options;
  final ValueChanged<String> onSelected;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: DropTokens.surface,
      borderRadius: BorderRadius.circular(999),
      child: InkWell(
        borderRadius: BorderRadius.circular(999),
        onTap: () => _pick(context),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          child: Row(
            children: [
              Icon(icon, size: 14, color: DropTokens.muted),
              const SizedBox(width: 6),
              Text(value,
                  style: DropType.caption.copyWith(color: DropTokens.text)),
              const SizedBox(width: 4),
              const Icon(Icons.keyboard_arrow_down,
                  size: 14, color: DropTokens.muted),
            ],
          ),
        ),
      ),
    );
  }

  void _pick(BuildContext context) {
    showModalBottomSheet(
      context: context,
      backgroundColor: DropTokens.sheet,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(28)),
      ),
      builder: (_) => DropSheet(
          child: Padding(
        padding: const EdgeInsets.fromLTRB(20, 16, 20, 28),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Center(
              child: Container(
                width: 36,
                height: 4,
                decoration: BoxDecoration(
                  color: DropTokens.handle,
                  borderRadius: BorderRadius.circular(2),
                ),
              ),
            ),
            const SizedBox(height: 16),
            for (final option in options)
              ListTile(
                title: Text(option, style: DropType.bodySm),
                trailing: option == value
                    ? const Icon(Icons.check, color: DropTokens.accent)
                    : null,
                onTap: () {
                  onSelected(option);
                  Navigator.of(context).pop();
                },
              ),
          ],
        ),
      )),
    );
  }
}

class _RequestCard extends StatelessWidget {
  const _RequestCard({
    required this.request,
    required this.statusColor,
    required this.onTap,
  });

  final GuestRequest request;
  final Color statusColor;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: DropTokens.surface,
          borderRadius: BorderRadius.circular(DropTokens.rCard),
          border: Border.all(color: DropTokens.border),
        ),
        child: Column(
          children: [
            Row(
              children: [
                ArtTile(path: request.track.art, size: 42),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(request.track.title,
                          style: DropType.trackTitle,
                          overflow: TextOverflow.ellipsis),
                      const SizedBox(height: 2),
                      Text(request.track.artist, style: DropType.caption),
                    ],
                  ),
                ),
                const SizedBox(width: 8),
                Text(
                  request.status.label,
                  style: DropType.caption.copyWith(
                    fontWeight: FontWeight.w600,
                    color: statusColor,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Text('${request.room} · ${request.timeLabel}',
                    style: DropType.caption),
                const Spacer(),
                Text('${request.priceLabel} · ${request.paymentLabel}',
                    style: DropType.caption),
              ],
            ),
            if (request.declineReason != null) ...[
              const SizedBox(height: 10),
              Container(
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                  color: DropTokens.declineCard,
                  borderRadius: BorderRadius.circular(DropTokens.rSm),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'A note from the DJ',
                      style: DropType.caption.copyWith(
                        fontWeight: FontWeight.w600,
                        color: DropTokens.declineTitle,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      request.declineReason!,
                      style: DropType.caption.copyWith(
                        height: 1.5,
                        color: DropTokens.declineBody,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
