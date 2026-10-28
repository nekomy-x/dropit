/// Guest · 04 · Payment + 04b · Choose payment method + 04d · Add card.
///
/// Макет pen.dev: экран оплаты (заказ, состав, способ оплаты, кнопка
/// оплаты) и две шторки — выбор способа оплаты и форма новой карты.
library;

import 'package:flutter/material.dart';

import '../data/demo_data.dart';
import '../data/models.dart';
import '../state/app_state.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

/// Stripe PaymentSheet owns card and wallet selection. The profile entry
/// explains where payment methods are managed without collecting card data.
void showPaymentMethodsSheet(BuildContext context) {
  showModalBottomSheet(
    context: context,
    backgroundColor: DropTokens.sheet,
    shape: const RoundedRectangleBorder(
      borderRadius: BorderRadius.vertical(top: Radius.circular(28)),
    ),
    builder: (context) => Padding(
      padding: const EdgeInsets.fromLTRB(24, 28, 24, 32),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text('Payment methods', style: DropType.sheetTitle),
          const SizedBox(height: 12),
          const Text(
            'Choose a card or supported wallet in Stripe checkout when you send a request.',
            style: DropType.bodySm,
          ),
          const SizedBox(height: 24),
          PrimaryButton(
            label: 'Done',
            onPressed: () => Navigator.of(context).pop(),
          ),
        ],
      ),
    ),
  );
}

class PaymentScreen extends StatefulWidget {
  const PaymentScreen({
    super.key,
    required this.session,
    required this.onBack,
    required this.onPaid,
  });

  final JoinSession session;
  final VoidCallback onBack;
  final Future<void> Function(Track) onPaid;

  @override
  State<PaymentScreen> createState() => _PaymentScreenState();
}

class _PaymentScreenState extends State<PaymentScreen> {
  bool _paying = false;

  Future<void> _pay() async {
    final state = AppState.of(context);
    final track = state.selectedTrack;
    if (track == null) return;
    setState(() => _paying = true);
    try {
      await widget.onPaid(track);
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Payment failed: $error')),
        );
      }
    } finally {
      if (mounted) setState(() => _paying = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final state = AppState.of(context);
    final track = state.selectedTrack ?? DemoData.trackMove;
    final price = widget.session.priceLabel == '—'
        ? track.priceLabel
        : widget.session.priceLabel;

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
                    onPressed: widget.onBack,
                    padding: EdgeInsets.zero,
                    constraints: const BoxConstraints(),
                    icon: const Icon(Icons.arrow_back,
                        size: 22, color: DropTokens.text),
                  ),
                  const SizedBox(width: 12),
                  const Text('Payment', style: DropType.navTitle),
                ],
              ),
            ),
            const SizedBox(height: 18),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: JoinRoomCard(room: widget.session.room),
            ),
            const SizedBox(height: 22),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                children: [
                  const Text('Make it yours.', style: DropType.screenTitle),
                  const SizedBox(height: 24),

                  // ── Заказ ──
                  Row(
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
                    ],
                  ),
                  const SizedBox(height: 24),

                  // ── Состав заказа ──
                  _SummaryRow(label: 'Track request', value: price),
                  const SizedBox(height: 10),
                  const _SummaryRow(label: 'Service fee', value: '0,00 €'),
                  const SizedBox(height: 12),
                  const Divider(color: DropTokens.border),
                  const SizedBox(height: 12),
                  Row(
                    children: [
                      Text('Total',
                          style: DropType.sectionTitle.copyWith(
                              fontSize: 18, fontWeight: FontWeight.w700)),
                      const Spacer(),
                      Text(price,
                          style: DropType.sectionTitle.copyWith(
                              fontSize: 18, fontWeight: FontWeight.w700)),
                    ],
                  ),
                  const SizedBox(height: 28),

                  // Stripe owns payment-method selection and card details.
                  const SectionLabel('SECURE CHECKOUT'),
                  const SizedBox(height: 12),
                  Container(
                    padding: const EdgeInsets.all(14),
                    decoration: BoxDecoration(
                      color: DropTokens.surface,
                      borderRadius: BorderRadius.circular(DropTokens.rLg),
                      border: Border.all(color: DropTokens.border),
                    ),
                    child: const Row(
                      children: [
                        Icon(Icons.lock_outline,
                            size: 20, color: DropTokens.accent),
                        SizedBox(width: 12),
                        Expanded(
                          child: Text(
                            'Choose a card or supported wallet in the secure Stripe checkout.',
                            style: DropType.bodySm,
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 28),
                  Text(
                    'Payment sends your request to the DJ.\nPlayback and timing are up to the DJ.',
                    style: DropType.caption.copyWith(height: 1.5),
                  ),
                ],
              ),
            ),

            // ── Оплата ──
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 12, 20, 8),
              child: PrimaryButton(
                label: price,
                loading: _paying,
                onPressed: _paying ? null : _pay,
              ),
            ),
            const Padding(
              padding: EdgeInsets.only(bottom: 20),
              child: Center(
                child: Text(
                  'Secure payment · No hidden fees',
                  style: DropType.caption,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _SummaryRow extends StatelessWidget {
  const _SummaryRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Text(label, style: DropType.caption),
        const Spacer(),
        Text(value, style: DropType.caption),
      ],
    );
  }
}
