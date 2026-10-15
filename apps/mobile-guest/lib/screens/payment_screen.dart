import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../theme/tokens.dart';
import '../widgets/artwork.dart';
import '../widgets/app_button.dart';
import '../widgets/nav.dart';
import '../widgets/sheet.dart';
import '../widgets/track_row.dart';

/// Guest · 04 · Payment (+ 04b Choose payment method, 04d Add payment card).
///
/// Макет: кадры «Guest · 04», «Guest · 04b», «Guest · 04d». Сводка заказа с
/// линиями сверху и снизу, выбранная карта с рамкой акцента, шторки выбора
/// способа оплаты (240 px сверху) и добавления карты (поля 48, радиус 10).
///
/// Оплата по-настоящему создаёт запрос: `POST /v1/requests` с обязательным
/// Idempotency-Key (мок-провайдер Test Mode, ADR-004).
class PaymentScreen extends StatefulWidget {
  const PaymentScreen({
    super.key,
    required this.api,
    required this.session,
    required this.track,
    required this.onSent,
    this.onBack,
  });

  final ApiClient api;
  final JoinResponse session;
  final CatalogTrack track;
  final void Function(RequestCreated) onSent;
  final VoidCallback? onBack;

  @override
  State<PaymentScreen> createState() => _PaymentScreenState();
}

class _PaymentScreenState extends State<PaymentScreen> {
  String _method = 'Visa ending in 4242';
  bool _busy = false;
  String? _error;

  Future<void> _pay() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final created = await widget.api.createRequest(
        trackId: widget.track.id,
        idempotencyKey: 'guest-${DateTime.now().microsecondsSinceEpoch}',
      );
      widget.onSent(created);
    } on ApiException catch (e) {
      setState(() {
        _error = switch (e.envelope.code) {
          'session_paused' =>
            'The DJ paused requests. Try again in a moment.',
          'session_closed' => 'This session is closed.',
          _ => e.envelope.message,
        };
      });
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _chooseMethod() async {
    final picked = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      backgroundColor: Colors.transparent,
      builder: (_) => _ChooseMethodSheet(selected: _method),
    );
    if (picked == null) return;
    if (picked == '__add_card') {
      if (!mounted) return;
      final added = await showModalBottomSheet<String>(
        context: context,
        isScrollControlled: true,
        backgroundColor: Colors.transparent,
        builder: (_) => const _AddCardSheet(),
      );
      if (added != null && mounted) setState(() => _method = added);
      return;
    }
    setState(() => _method = picked);
  }

  @override
  Widget build(BuildContext context) {
    final price = widget.session.priceLabel;
    final sessionLine = widget.session.djDisplayName == null
        ? 'Main floor · Live now'
        : 'Main floor · ${widget.session.djDisplayName} · Live now';

    return Scaffold(
      backgroundColor: DropTokens.bg,
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(20, 24, 20, 20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              NavRow(title: 'Payment', onBack: widget.onBack),
              const SizedBox(height: 14),
              JoinedRoomBar(
                roomName: widget.session.clubName,
                sessionLine: sessionLine,
                padding: const EdgeInsets.fromLTRB(0, 16, 0, 16),
              ),
              const SizedBox(height: 20),
              const Text('Make it yours.', style: DropType.screenTitle),
              const SizedBox(height: 24),
              Expanded(
                child: ListView(
                  padding: EdgeInsets.zero,
                  children: [
                    Row(
                      children: [
                        const Artwork(url: null),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(widget.track.title,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: DropType.trackTitle),
                              const SizedBox(height: 5),
                              Text('${widget.track.artist} · Original mix',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: DropType.metaSm),
                            ],
                          ),
                        ),
                        AppIconButton(
                          icon: Icons.add,
                          tooltip: 'Add another request',
                          onPressed: () => Navigator.of(context).maybePop(),
                        ),
                      ],
                    ),
                    const SizedBox(height: 20),
                    // Order summary: линии сверху и снизу, вертикальные
                    // отступы 20 (кадр Guest · 04).
                    Container(
                      decoration: const BoxDecoration(
                        border: Border(
                          top: BorderSide(color: DropTokens.divider),
                          bottom: BorderSide(color: DropTokens.divider),
                        ),
                      ),
                      padding: const EdgeInsets.symmetric(vertical: 20),
                      child: Column(
                        children: [
                          SummaryRow(label: 'Track request', value: price),
                          const SizedBox(height: 18),
                          const SummaryRow(
                              label: 'Service fee', value: '0,00 €'),
                          const SizedBox(height: 18),
                          SummaryRow(
                              label: 'Total', value: price, emphasis: true),
                        ],
                      ),
                    ),
                    const SizedBox(height: 20),
                    const SectionLabel('PAY WITH',
                        style: DropType.labelWide),
                    const SizedBox(height: 12),
                    _SelectedCard(method: _method, onTap: _chooseMethod),
                    const SizedBox(height: 12),
                    GestureDetector(
                      onTap: _chooseMethod,
                      behavior: HitTestBehavior.opaque,
                      child: Text(
                        'Change payment method',
                        style: DropType.caption.copyWith(
                          fontWeight: FontWeight.w600,
                          color: DropTokens.accent,
                        ),
                      ),
                    ),
                    if (_error != null) ...[
                      const SizedBox(height: 16),
                      _PaymentError(message: _error!),
                    ],
                  ],
                ),
              ),
              const SizedBox(height: 12),
              Text(
                'Payment sends your request to the DJ.\n'
                'Playback and timing are up to the DJ.',
                style: DropType.caption.copyWith(height: 1.5),
              ),
              const SizedBox(height: 16),
              AppButton(
                label: price,
                loading: _busy,
                onPressed: _busy ? null : _pay,
              ),
              const SizedBox(height: 12),
              Center(
                child: Text(
                  'Secure payment · No hidden fees',
                  style: DropType.meta.copyWith(color: DropTokens.muted),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _SelectedCard extends StatelessWidget {
  const _SelectedCard({required this.method, required this.onTap});

  final String method;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      behavior: HitTestBehavior.opaque,
      child: Container(
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(DropTokens.rLg),
          border: Border.all(color: DropTokens.accent),
        ),
        child: Row(
          children: [
            const _CardBrandBadge(label: 'VISA'),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                method,
                style: DropType.bodySm.copyWith(fontSize: 13),
              ),
            ),
            const Icon(Icons.check_circle, size: 20, color: DropTokens.accent),
          ],
        ),
      ),
    );
  }
}

/// Guest · 04b: шторка «Pay your way.»
class _ChooseMethodSheet extends StatelessWidget {
  const _ChooseMethodSheet({required this.selected});

  final String selected;

  @override
  Widget build(BuildContext context) {
    const methods = <({String title, String subtitle, String brand})>[
      (title: 'Visa ending in 4242', subtitle: 'Saved card', brand: 'VISA'),
      (title: 'Apple Pay', subtitle: 'Fast, secure checkout', brand: 'PAY'),
      (title: 'Google Pay', subtitle: 'Pay with your Google account', brand: 'GPay'),
    ];

    return AppSheet(
      title: 'Pay your way.',
      trailing: IconButton(
        icon: const Icon(Icons.close, size: 22, color: DropTokens.muted),
        onPressed: () => Navigator.of(context).pop(),
      ),
      children: [
        for (final m in methods) ...[
          PaymentMethodRow(
            title: m.title,
            subtitle: m.subtitle,
            brandLabel: m.brand,
            brandColor: m.brand == 'PAY'
                ? Colors.black
                : (m.brand == 'GPay' ? const Color(0xFF202124) : const Color(0xFF1434CB)),
            selected: selected == m.title,
            onTap: () => Navigator.of(context).pop(m.title),
          ),
          const SizedBox(height: 8),
        ],
        const SizedBox(height: 4),
        GestureDetector(
          onTap: () => Navigator.of(context).pop('__add_card'),
          behavior: HitTestBehavior.opaque,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 14),
            child: Row(
              children: [
                const Icon(Icons.add_circle_outline,
                    size: 22, color: DropTokens.accent),
                const SizedBox(width: 12),
                Text(
                  'Add a new card',
                  style: DropType.bodySm.copyWith(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: DropTokens.accent,
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

/// Guest · 04d: шторка «Add a card.»
class _AddCardSheet extends StatefulWidget {
  const _AddCardSheet();

  @override
  State<_AddCardSheet> createState() => _AddCardSheetState();
}

class _AddCardSheetState extends State<_AddCardSheet> {
  final _number = TextEditingController();
  final _expiry = TextEditingController();
  final _cvc = TextEditingController();
  final _name = TextEditingController();
  bool _save = true;

  @override
  void dispose() {
    _number.dispose();
    _expiry.dispose();
    _cvc.dispose();
    _name.dispose();
    super.dispose();
  }

  /// ЗАГЛУШКА (правило 5): карта не уходит в платёжный провайдер —
  /// реальный ввод карты отдаётся hosted-форме Stripe (ADR-004).
  void _saveAndUse() {
    final digits = _number.text.replaceAll(RegExp(r'\D'), '');
    final last4 = digits.length >= 4 ? digits.substring(digits.length - 4) : '0000';
    Navigator.of(context).pop('Card ending in $last4');
  }

  @override
  Widget build(BuildContext context) {
    return AppSheet(
      title: 'Add a card.',
      leading: IconButton(
        icon: const Icon(Icons.arrow_back, size: 22, color: DropTokens.text),
        onPressed: () => Navigator.of(context).pop(),
      ),
      trailing: IconButton(
        icon: const Icon(Icons.close, size: 22, color: DropTokens.muted),
        onPressed: () => Navigator.of(context).pop(),
      ),
      children: [
        SheetField(
          label: 'Card number',
          placeholder: '1234  5678  9012  3456',
          icon: Icons.credit_card,
          controller: _number,
          keyboardType: TextInputType.number,
        ),
        const SizedBox(height: 16),
        Row(
          children: [
            Expanded(
              child: SheetField(
                label: 'Expiry date',
                placeholder: 'MM / YY',
                controller: _expiry,
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: SheetField(
                label: 'Security code',
                placeholder: 'CVC',
                icon: Icons.help_outline,
                controller: _cvc,
                keyboardType: TextInputType.number,
              ),
            ),
          ],
        ),
        const SizedBox(height: 16),
        SheetField(
          label: 'Name on card',
          placeholder: 'Full name',
          controller: _name,
        ),
        const SizedBox(height: 20),
        GestureDetector(
          onTap: () => setState(() => _save = !_save),
          behavior: HitTestBehavior.opaque,
          child: Row(
            children: [
              Icon(
                _save ? Icons.check_circle : Icons.circle_outlined,
                size: 20,
                color: _save ? DropTokens.accent : DropTokens.inactive,
              ),
              const SizedBox(width: 10),
              Text('Save card for next time',
                  style: DropType.caption.copyWith(color: DropTokens.text)),
            ],
          ),
        ),
        const SizedBox(height: 20),
        AppButton(label: 'Save & use card', onPressed: _saveAndUse),
        const SizedBox(height: 16),
        const Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(Icons.lock_outline, size: 14, color: DropTokens.muted),
            SizedBox(width: 6),
            Text(
              'Your card details are encrypted and secure',
              style: DropType.metaSm,
            ),
          ],
        ),
      ],
    );
  }
}

class _CardBrandBadge extends StatelessWidget {
  const _CardBrandBadge({required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 40,
      height: 28,
      decoration: BoxDecoration(
        color: Colors.white,
        borderRadius: BorderRadius.circular(4),
      ),
      alignment: Alignment.center,
      child: Text(
        label,
        style: const TextStyle(
          fontFamily: 'Inter',
          fontSize: 9,
          fontWeight: FontWeight.w800,
          color: Color(0xFF1434CB),
        ),
      ),
    );
  }
}

class _PaymentError extends StatelessWidget {
  const _PaymentError({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: DropTokens.redSoft,
        borderRadius: BorderRadius.circular(DropTokens.rMd),
        border: Border.all(color: DropTokens.red),
      ),
      child: Row(
        children: [
          const Icon(Icons.error_outline, size: 24, color: DropTokens.red),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Can’t reach the booth',
                  style: DropType.bodySm.copyWith(
                    fontWeight: FontWeight.w700,
                    color: DropTokens.red,
                  ),
                ),
                const SizedBox(height: 8),
                Text(message, style: DropType.caption),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
