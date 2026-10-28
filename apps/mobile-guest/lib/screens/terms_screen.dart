/// Guest · 04a · Terms before payment.
///
/// Макет pen.dev: шапка «назад + заголовок», правила запроса и оплаты,
/// кнопка принятия условий и ссылка на полные условия.
library;

import 'package:flutter/material.dart';

import '../data/models.dart';
import '../state/app_state.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class TermsScreen extends StatelessWidget {
  const TermsScreen({
    super.key,
    required this.session,
    required this.onBack,
    required this.onAccept,
  });

  final JoinSession session;
  final VoidCallback onBack;
  final VoidCallback onAccept;

  @override
  Widget build(BuildContext context) {
    final state = AppState.of(context);
    final track = state.selectedTrack ?? _fallbackTrack;
    final price =
        session.priceLabel == '—' ? track.priceLabel : session.priceLabel;
    final rules = [
      'You pay $price to send this request. The DJ decides if and when it gets played.',
      'If several people request the same song, each request is paid separately.',
      'A song can only be played once per night.',
    ];
    return DropScaffold(
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
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
                  const Text('Before you pay', style: DropType.navTitle),
                ],
              ),
            ),
            const SizedBox(height: 26),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                children: [
                  const Text('Know the rules.', style: DropType.screenTitle),
                  const SizedBox(height: 24),

                  // ── Заказ: трек + цена ──
                  Container(
                    padding: const EdgeInsets.all(12),
                    decoration: BoxDecoration(
                      color: DropTokens.surface,
                      borderRadius: BorderRadius.circular(DropTokens.rMd),
                      border: Border.all(color: DropTokens.border),
                    ),
                    child: Row(
                      children: [
                        ArtTile(path: track.art, size: 50),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(track.title, style: DropType.trackTitle),
                              const SizedBox(height: 2),
                              Text('${track.artist} · Original mix',
                                  style: DropType.caption),
                            ],
                          ),
                        ),
                        const SizedBox(width: 8),
                        Text(
                          price,
                          style: DropType.labelTight.copyWith(
                            color: DropTokens.accent,
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 28),

                  // ── Праила ─
                  for (final rule in rules) ...[
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const Icon(Icons.check_circle_outline,
                            size: 18, color: DropTokens.accent),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Text(
                            rule,
                            style: DropType.body.copyWith(height: 1.5),
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 18),
                  ],
                ],
              ),
            ),

            // ── Принять и продолжить ──
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
              child: PrimaryButton(
                label: 'I understand — Continue',
                onPressed: onAccept,
              ),
            ),
            Padding(
              padding: const EdgeInsets.only(bottom: 20),
              child: Center(
                child: InkWell(
                  onTap: () => _showFullTerms(context),
                  child: Text(
                    'Read the full terms',
                    style: DropType.caption.copyWith(
                      fontWeight: FontWeight.w600,
                      color: DropTokens.accent,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _showFullTerms(BuildContext context) {
    showModalBottomSheet(
      context: context,
      backgroundColor: DropTokens.sheet,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(28)),
      ),
      builder: (_) => const _FullTermsSheet(),
    );
  }

  static const _fallbackTrack = Track(
    id: 'move',
    title: 'Move',
    artist: 'Adam Port',
    genre: 'Afro house',
    art: 'assets/images/art_move.jpg',
    priceLabel: '3,00 €',
  );
}

class _FullTermsSheet extends StatelessWidget {
  const _FullTermsSheet();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 16, 24, 32),
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
          const Text('Full terms', style: DropType.sheetTitle),
          const SizedBox(height: 12),
          Text(
            'A track request is a paid ask to the DJ. Payment is charged when '
            'the request is sent. The DJ may decline a request at their '
            'discretion; declined requests are refunded to the original '
            'payment method within 5–10 business days. A track can be '
            'played once per night. Prices are set by the venue and shown '
            'before payment.',
            style: DropType.body.copyWith(color: DropTokens.muted),
          ),
          const SizedBox(height: 20),
          PrimaryButton(
            label: 'Close',
            trailingArrow: false,
            onPressed: () => Navigator.of(context).pop(),
          ),
        ],
      ),
    );
  }
}
