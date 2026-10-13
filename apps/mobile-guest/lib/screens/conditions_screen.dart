import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../main.dart';

/// ОБЯЗАТЕЛЬНЫЙ экран условий оплаты (dizayn-zadanie §0.2 —
/// по ТЗ требуется до оплаты):
///   * «Вы платите {цена} за отправку запроса. Решение о
///     воспроизведении остаётся за диджеем»;
///   * «Если несколько человек запрашивают одну песню, каждый запрос
///     оплачивается отдельно»;
///   * «Песня может быть сыграна только один раз»;
///   * подтверждение + ссылка на условия.
class ConditionsScreen extends StatefulWidget {
  const ConditionsScreen({
    super.key,
    required this.session,
    required this.track,
    required this.onConfirmed,
  });

  final JoinResponse session;
  final CatalogTrack track;
  final VoidCallback onConfirmed;

  @override
  State<ConditionsScreen> createState() => _ConditionsScreenState();
}

class _ConditionsScreenState extends State<ConditionsScreen> {
  bool _accepted = false;
  bool _busy = false;
  String? _error;

  Future<void> _confirm() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      // F5 Test Mode: платёж-интент создаётся сервером; confirm здесь
      // не вызывается до прод-Stripe (ADR-004). Запрос создан —
      // статус виден в My requests.
      widget.onConfirmed();
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.session;
    final t = widget.track;
    return Scaffold(
      appBar: AppBar(title: const Text('Before you pay')),
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(20),
                  child: Column(
                    children: [
                      Text(
                        t.title,
                        style: Theme.of(context)
                            .textTheme
                            .titleLarge
                            ?.copyWith(fontWeight: FontWeight.w700),
                        textAlign: TextAlign.center,
                      ),
                      Text(t.artist,
                          style: TextStyle(color: DropItTheme.grey)),
                      const SizedBox(height: 16),
                      Text(
                        s.priceLabel,
                        style: Theme.of(context)
                            .textTheme
                            .headlineMedium
                            ?.copyWith(
                              color: DropItTheme.neon,
                              fontWeight: FontWeight.w800,
                            ),
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 24),
              _condition(
                context,
                'You pay ${s.priceLabel} to send this request. The decision '
                'to play it stays with the DJ.',
              ),
              _condition(
                context,
                'If several people request the same song, each request '
                'is paid separately.',
              ),
              _condition(
                context,
                'A song can only be played once per night.',
              ),
              const Spacer(),
              CheckboxListTile(
                value: _accepted,
                onChanged: (v) => setState(() => _accepted = v ?? false),
                title: const Text('I understand and accept'),
                activeColor: DropItTheme.neon,
              ),
              TextButton(
                onPressed: () {},
                child: Text(
                  'Terms & conditions',
                  style: TextStyle(color: DropItTheme.grey),
                ),
              ),
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child:
                      Text(_error!, style: TextStyle(color: DropItTheme.red)),
                ),
              ElevatedButton(
                onPressed: _accepted && !_busy ? _confirm : null,
                child: _busy
                    ? const SizedBox(
                        height: 20,
                        width: 20,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : Text('Pay ${s.priceLabel} & send request'),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _condition(BuildContext context, String text) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Icon(Icons.check_circle_outline, color: DropItTheme.neon, size: 20),
          const SizedBox(width: 12),
          Expanded(child: Text(text)),
        ],
      ),
    );
  }
}
