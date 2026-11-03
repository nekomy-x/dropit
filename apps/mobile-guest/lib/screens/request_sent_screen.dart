/// Guest · 05 · Request sent.
///
/// Макет pen.dev: подтверждение отправки запроса — свечение, ripples,
/// бейдж с галочкой, конфетти, чек (трек/статус/оплата/ID) и две кнопки:
/// «View my request» и «Find another track».
library;

import 'package:flutter/material.dart';

import '../data/models.dart';
import '../theme/tokens.dart';
import '../widgets/receipt_ticket.dart';
import '../widgets/ui.dart';

class RequestSentScreen extends StatelessWidget {
  const RequestSentScreen({
    super.key,
    required this.session,
    required this.request,
    required this.onViewRequest,
    required this.onFindAnother,
  });

  final JoinSession session;
  final GuestRequest request;
  final VoidCallback onViewRequest;
  final VoidCallback onFindAnother;

  @override
  Widget build(BuildContext context) {
    return DropScaffold(
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const SizedBox(height: 24),
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: 20),
              child: BrandCityRow(),
            ),
            const SizedBox(height: 18),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: JoinRoomCard(room: session.room),
            ),
            const SizedBox(height: 24),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                children: [
                  // ── Празднование ──
                  const SizedBox(
                    height: 160,
                    child: Stack(
                      alignment: Alignment.center,
                      children: [
                        _Celebration(),
                      ],
                    ),
                  ),
                  const SizedBox(height: 8),
                  const Text('Your pick is in.', style: DropType.screenTitle),
                  const SizedBox(height: 8),
                  Text(
                    'Your request is with ${session.dj}.\nFollow its status in My requests.',
                    style: DropType.bodySm.copyWith(
                      fontSize: 13,
                      height: 1.5,
                      color: DropTokens.muted,
                    ),
                  ),
                  const SizedBox(height: 24),

                  // ── Чек (тап → neon-квитанция) ──
                  GestureDetector(
                    onTap: () => _showTicket(context),
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
                              ArtTile(path: request.track.art, size: 50),
                              const SizedBox(width: 12),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(request.track.title,
                                        style: DropType.trackTitle),
                                    const SizedBox(height: 2),
                                    Text(request.track.artist,
                                        style: DropType.caption),
                                  ],
                                ),
                              ),
                              const Icon(Icons.chevron_right,
                                  size: 18, color: DropTokens.muted),
                            ],
                          ),
                          const SizedBox(height: 14),
                          const Divider(color: DropTokens.border),
                          const _ReceiptRow(
                            label: 'Status',
                            value: 'Sent to DJ',
                            valueColor: DropTokens.accent,
                            bold: true,
                          ),
                          _ReceiptRow(
                            label: 'Payment',
                            value:
                                '${request.priceLabel} · ${request.paymentLabel}',
                          ),
                          _ReceiptRow(
                            label: 'Request ID',
                            value: request.id,
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),

            // ── Действия ──
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 12, 20, 8),
              child: PrimaryButton(
                label: 'View my request',
                onPressed: onViewRequest,
              ),
            ),
            Padding(
              padding: const EdgeInsets.only(bottom: 20),
              child: SecondaryButton(
                label: 'Find another track',
                onPressed: onFindAnother,
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _showTicket(BuildContext context) {
    showModalBottomSheet(
      context: context,
      backgroundColor: DropTokens.bg,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(28)),
      ),
      builder: (_) => DropSheet(
          child: Padding(
        padding: const EdgeInsets.fromLTRB(20, 16, 20, 32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
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
            ReceiptTicket(request: request),
            const SizedBox(height: 20),
            PrimaryButton(
              label: 'Done',
              trailingArrow: false,
              onPressed: () => Navigator.of(context).pop(),
            ),
          ],
        ),
      )),
    );
  }
}

class _ReceiptRow extends StatelessWidget {
  const _ReceiptRow({
    required this.label,
    required this.value,
    this.valueColor,
    this.bold = false,
  });

  final String label;
  final String value;
  final Color? valueColor;
  final bool bold;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          Text(label, style: DropType.caption),
          const SizedBox(width: 12),
          Expanded(
              child: Text(
            value,
            textAlign: TextAlign.end,
            style: DropType.caption.copyWith(
              fontWeight: bold ? FontWeight.w600 : FontWeight.normal,
              color: valueColor ?? DropTokens.muted,
            ),
          )),
        ],
      ),
    );
  }
}

/// Анимация празднования: свечение, круги, бейдж, конфетти и искры.
class _Celebration extends StatefulWidget {
  const _Celebration();

  @override
  State<_Celebration> createState() => _CelebrationState();
}

class _CelebrationState extends State<_Celebration>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c;

  @override
  void initState() {
    super.initState();
    _c = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1600),
    )..repeat();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (MediaQuery.disableAnimationsOf(context)) {
      _c.stop();
    } else if (!_c.isAnimating) {
      _c.repeat();
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _c,
      builder: (context, _) {
        final t = _c.value;
        return SizedBox(
          width: 350,
          height: 128,
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              // Мягкое изумрудное свечение.
              Positioned.fill(
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    gradient: RadialGradient(
                      colors: [
                        DropTokens.accent
                            .withValues(alpha: 0.22 * (1 - t * 0.3)),
                        DropTokens.accent.withValues(alpha: 0),
                      ],
                    ),
                  ),
                ),
              ),
              // Два расходящихся крга.
              for (final spec in const [(124.0, 0.0), (102.0, 0.35)])
                Center(
                  child: Container(
                    width: spec.$1 * (1 + t * 0.35),
                    height: spec.$1 * (1 + t * 0.35),
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      border: Border.all(
                        color: DropTokens.accent
                            .withValues(alpha: (0.5 - t * 0.4).clamp(0, 1)),
                        width: 1.5,
                      ),
                    ),
                  ),
                ),
              // Центральный бейдж.
              const Center(
                child: _SuccessBadge(),
              ),
              // Конфетти.
              for (final c in _confetti)
                Positioned(
                  left: c.$1,
                  top: c.$2 + (t * 24) % 140 - 20,
                  child: Opacity(
                    opacity: (1 - t).clamp(0.2, 1),
                    child: Container(
                      width: c.$3,
                      height: c.$4,
                      decoration: BoxDecoration(
                        color: c.$5,
                        borderRadius: BorderRadius.circular(1),
                      ),
                    ),
                  ),
                ),
              // Искры.
              for (final s in _sparkles)
                Positioned(
                  left: s.$1,
                  top: s.$2,
                  child: Icon(_sparkleIcon, size: s.$3, color: s.$4),
                ),
            ],
          ),
        );
      },
    );
  }
}

const _sparkleIcon = Icons.auto_awesome;

const _confetti = <(double, double, double, double, Color)>[
  (48, 35, 4, 10, DropTokens.accent),
  (85, 12, 4, 8, DropTokens.sparkle),
  (112, 98, 3, 8, DropTokens.accent),
  (263, 20, 4, 10, DropTokens.accent),
  (298, 69, 4, 9, DropTokens.sparkle),
  (251, 106, 4, 8, DropTokens.accent),
  (63, 105, 3, 6, DropTokens.sparklePale),
  (294, 10, 3, 5, DropTokens.sparklePale),
];

const _sparkles = <(double, double, double, Color)>[
  (71, 67, 18, DropTokens.accent),
  (239, 51, 14, DropTokens.sparkleSoft),
  (312, 37, 14, DropTokens.accent),
];

class _SuccessBadge extends StatelessWidget {
  const _SuccessBadge();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 80,
      height: 80,
      decoration: const BoxDecoration(
        color: DropTokens.accent,
        shape: BoxShape.circle,
      ),
      child: const Icon(Icons.check, size: 38, color: DropTokens.onAccent),
    );
  }
}
