/// Guest · 13 · Request detail.
///
/// Макет pen.dev: герой запроса (трек, бейдж статуса, мета), таймлайн
/// статусов (Sent → Accepted → In queue → Played), чек оплаты и кнопки
/// «Share request» / «Report an issue».
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../data/models.dart';
import '../state/app_state.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class RequestDetailScreen extends StatelessWidget {
  const RequestDetailScreen({
    super.key,
    required this.request,
    required this.onBack,
  });

  final GuestRequest request;
  final VoidCallback onBack;

  @override
  Widget build(BuildContext context) {
    final state = AppState.of(context);
    final steps = _stepsFor(request.status);

    return DropScaffold(
      tabBar: DropTabBar(
        current: 2,
        // Таб-бар на push-экране: переключаем таб и возвращаемся к корню.
        onChanged: (index) {
          state.setTab(index);
          Navigator.of(context).popUntil((route) => route.isFirst);
        },
      ),
      body: ListView(
        padding: EdgeInsets.zero,
        children: [
          const SizedBox(height: 24),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Row(
              children: [
                IconButton(
                  onPressed: onBack,
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(),
                  icon: const Icon(Icons.arrow_back,
                      size: 22, color: DropTokens.text),
                ),
                const SizedBox(width: 12),
                const Text('Request', style: DropType.trackTitle),
                const Spacer(),
                const CityPill(),
              ],
            ),
          ),
          const SizedBox(height: 20),

          // ── Герой запроса ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    ArtTile(
                      path: request.track.art,
                      size: 56,
                      radius: DropTokens.rXs,
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(request.track.title,
                              style: DropType.requestTitle),
                          const SizedBox(height: 3),
                          Text(request.track.artist, style: DropType.caption),
                        ],
                      ),
                    ),
                    const SizedBox(width: 8),
                    Container(
                      padding: const EdgeInsets.symmetric(
                          horizontal: 10, vertical: 6),
                      decoration: BoxDecoration(
                        color: DropTokens.tint1A,
                        borderRadius: BorderRadius.circular(12),
                      ),
                      child: Text(
                        _statusBadge(request.status),
                        style: DropType.labelTight.copyWith(
                          color: DropTokens.accent,
                        ),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 14),
                Row(
                  children: [
                    const Icon(Icons.place_outlined,
                        size: 13, color: DropTokens.muted),
                    const SizedBox(width: 6),
                    Text(request.room,
                        style:
                            DropType.caption.copyWith(color: DropTokens.text)),
                    const SizedBox(width: 8),
                    Container(
                      width: 3,
                      height: 3,
                      decoration: const BoxDecoration(
                        color: DropTokens.muted,
                        shape: BoxShape.circle,
                      ),
                    ),
                    const SizedBox(width: 8),
                    Text(request.id, style: DropType.caption),
                  ],
                ),
              ],
            ),
          ),
          const SizedBox(height: 26),

          // ── Таймлайн ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Column(
              children: [
                for (var i = 0; i < steps.length; i++) ...[
                  _TimelineStep(
                    step: steps[i],
                    last: i == steps.length - 1,
                  ),
                ],
              ],
            ),
          ),
          const SizedBox(height: 24),

          // ── Чек ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Container(
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: DropTokens.surface,
                borderRadius: BorderRadius.circular(DropTokens.rLg),
                border: Border.all(color: DropTokens.border),
              ),
              child: Column(
                children: [
                  Row(
                    children: [
                      const Text('Payment', style: DropType.caption),
                      const Spacer(),
                      Text(
                        request.priceLabel,
                        style: DropType.trackTitle.copyWith(
                          color: DropTokens.accent,
                        ),
                      ),
                      const SizedBox(width: 10),
                      Container(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 8, vertical: 3),
                        decoration: BoxDecoration(
                          color: DropTokens.tint1A,
                          borderRadius: BorderRadius.circular(6),
                        ),
                        child: Text(
                          request.paymentLabel.toUpperCase(),
                          style: DropType.labelTight.copyWith(
                            color: DropTokens.accent,
                          ),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 12),
                  const Divider(color: DropTokens.border),
                  _ReceiptValueRow(
                      label: 'Requested at',
                      value: 'Tonight · ${request.timeLabel}'),
                  _ReceiptValueRow(label: 'Room', value: request.room),
                ],
              ),
            ),
          ),
          const SizedBox(height: 24),

          // ── Действия ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Column(
              children: [
                PrimaryButton(
                  label: 'Share request',
                  icon: Icons.ios_share,
                  onPressed: () => _share(context),
                ),
                const SizedBox(height: 10),
                SecondaryButton(
                  label: 'Report an issue',
                  icon: Icons.flag_outlined,
                  onPressed: () => _report(context),
                ),
              ],
            ),
          ),
          const SizedBox(height: 100),
        ],
      ),
    );
  }

  String _statusBadge(RequestStatus status) => switch (status) {
        RequestStatus.queue => 'IN QUEUE #3',
        RequestStatus.awaiting => 'AWAITING DJ',
        RequestStatus.played => 'PLAYED',
        RequestStatus.declined => 'DECLINED',
      };

  List<_StepData> _stepsFor(RequestStatus status) {
    const sent = _StepData('Sent to DJ', 'Request delivered', _StepState.done);
    return switch (status) {
      RequestStatus.awaiting => [
          sent,
          const _StepData('Awaiting DJ', 'The DJ has not responded yet',
              _StepState.current),
          const _StepData(
              'In queue', 'Waiting for approval', _StepState.pending),
          const _StepData('Played', 'Not played yet', _StepState.pending)
        ],
      RequestStatus.queue => [
          sent,
          const _StepData('Accepted', 'Approved by the DJ', _StepState.done),
          const _StepData(
              'In queue', 'Waiting to be played', _StepState.current),
          const _StepData('Played', 'Not played yet', _StepState.pending)
        ],
      RequestStatus.played => [
          sent,
          const _StepData('Accepted', 'Approved by the DJ', _StepState.done),
          const _StepData('In queue', 'Completed', _StepState.done),
          const _StepData('Played', 'Played by the DJ', _StepState.current)
        ],
      RequestStatus.declined => [
          sent,
          const _StepData(
              'Declined', 'The DJ declined this request', _StepState.current)
        ],
    };
  }

  void _share(BuildContext context) {
    final text = 'DROP IT · ${request.track.title} — ${request.track.artist} '
        '(${request.id}, ${request.room})';
    _copyRequestDetails(context, text);
  }

  void _report(BuildContext context) {
    showModalBottomSheet(
      context: context,
      backgroundColor: DropTokens.sheet,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(28)),
      ),
      builder: (_) => Padding(
        padding: const EdgeInsets.fromLTRB(24, 16, 24, 28),
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
            const SizedBox(height: 20),
            const Text('Report an issue', style: DropType.sheetTitle),
            const SizedBox(height: 12),
            Text(
              'Tell us what went wrong with request ${request.id}. '
              'Support replies in ~5 minutes.',
              style: DropType.body.copyWith(color: DropTokens.muted),
            ),
            const SizedBox(height: 20),
            PrimaryButton(
              label: 'Contact support',
              onPressed: () {
                Navigator.of(context).pop();
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('support@dropit.club copied')),
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}

/// Копирование деталей запроса в буфер + обратная связь.
void _copyRequestDetails(BuildContext context, String text) {
  Clipboard.setData(ClipboardData(text: text));
  ScaffoldMessenger.of(context).showSnackBar(
    const SnackBar(content: Text('Request details copied')),
  );
}

class _ReceiptValueRow extends StatelessWidget {
  const _ReceiptValueRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          Text(label, style: DropType.caption),
          const Spacer(),
          Text(
            value,
            style: DropType.caption.copyWith(
              fontWeight: FontWeight.w600,
              color: DropTokens.text,
            ),
          ),
        ],
      ),
    );
  }
}

enum _StepState { done, current, pending }

class _StepData {
  const _StepData(this.label, this.sub, this.state);

  final String label;
  final String sub;
  final _StepState state;
}

class _TimelineStep extends StatelessWidget {
  const _TimelineStep({required this.step, required this.last});

  final _StepData step;
  final bool last;

  @override
  Widget build(BuildContext context) {
    final color = switch (step.state) {
      _StepState.done => DropTokens.accent,
      _StepState.current => DropTokens.accent,
      _StepState.pending => DropTokens.muted,
    };
    return IntrinsicHeight(
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Рельс: маркер + коннектор.
          SizedBox(
            width: 24,
            child: Column(
              children: [
                Container(
                  width: 24,
                  height: 24,
                  decoration: BoxDecoration(
                    color: step.state == _StepState.pending
                        ? Colors.transparent
                        : DropTokens.accent,
                    shape: BoxShape.circle,
                    border: step.state == _StepState.pending
                        ? Border.all(color: DropTokens.inactive, width: 2)
                        : step.state == _StepState.current
                            ? Border.all(color: DropTokens.accent, width: 3)
                            : null,
                  ),
                  child: switch (step.state) {
                    _StepState.done => const Icon(Icons.check,
                        size: 13, color: DropTokens.onAccent),
                    _StepState.current => Center(
                        child: Container(
                          width: 8,
                          height: 8,
                          decoration: const BoxDecoration(
                            color: DropTokens.bg,
                            shape: BoxShape.circle,
                          ),
                        ),
                      ),
                    _StepState.pending => null,
                  },
                ),
                if (!last)
                  Expanded(
                    child: Container(
                      width: 2,
                      margin: const EdgeInsets.symmetric(vertical: 4),
                      color: step.state == _StepState.pending
                          ? DropTokens.tint1F
                          : DropTokens.accent,
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(width: 12),
          // Контент шага.
          Expanded(
            child: Padding(
              padding: EdgeInsets.only(bottom: last ? 0 : 22),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    step.label,
                    style: DropType.bodySm.copyWith(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: color,
                    ),
                  ),
                  const SizedBox(height: 3),
                  Text(step.sub, style: DropType.caption),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
