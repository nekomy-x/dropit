import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../main.dart';

/// Экран QR-скана (вход в сессию).
///
/// B5: реальный поиск-сканер камеры — mobile_scanner подключается при
/// сборке под устройство; в каркасе — ручной ввод токена QR + кнопка
/// «демо-скан» для e2e-проверки контракта.
class ScanScreen extends StatefulWidget {
  const ScanScreen({super.key, required this.api, required this.onJoined});

  final ApiClient api;
  final void Function(JoinResponse) onJoined;

  @override
  State<ScanScreen> createState() => _ScanScreenState();
}

class _ScanScreenState extends State<ScanScreen> {
  final _controller = TextEditingController();
  bool _busy = false;
  String? _error;

  Future<void> _join() async {
    final token = _controller.text.trim();
    if (token.isEmpty) {
      setState(() => _error = 'Enter the code from the club QR');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final session = await widget.api.joinByQr(token);
      widget.onJoined(session);
    } on ApiException catch (e) {
      setState(() {
        _error = switch (e.envelope.code) {
          'not_found' => 'QR not found or expired — ask staff to regenerate',
          'session_closed' => 'This session is closed',
          'unauthorized' => 'Device not recognized — restart the app',
          _ => e.envelope.message,
        };
      });
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Icon(Icons.headphones, size: 72, color: DropItTheme.neon),
              const SizedBox(height: 16),
              Text(
                'DROP IT',
                textAlign: TextAlign.center,
                style: Theme.of(context).textTheme.headlineLarge?.copyWith(
                      fontWeight: FontWeight.w800,
                      letterSpacing: 4,
                    ),
              ),
              const SizedBox(height: 8),
              Text(
                'Your song. Your night.',
                textAlign: TextAlign.center,
                style: Theme.of(context)
                    .textTheme
                    .bodyMedium
                    ?.copyWith(color: DropItTheme.grey),
              ),
              const SizedBox(height: 48),
              // Рамка «камеры» — место будущего сканера (B5).
              Container(
                height: 180,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(16),
                  border: Border.all(color: DropItTheme.neon, width: 2),
                ),
                child: Center(
                  child: Text(
                    'Point camera at\nclub QR code',
                    textAlign: TextAlign.center,
                    style: TextStyle(color: DropItTheme.grey),
                  ),
                ),
              ),
              const SizedBox(height: 24),
              TextField(
                controller: _controller,
                decoration: const InputDecoration(
                  hintText: '…or paste QR code',
                  prefixIcon: Icon(Icons.qr_code_2),
                ),
                onSubmitted: (_) => _join(),
              ),
              const SizedBox(height: 16),
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(bottom: 12),
                  child: Text(
                    _error!,
                    style: TextStyle(color: DropItTheme.red),
                  ),
                ),
              ElevatedButton(
                onPressed: _busy ? null : _join,
                child: _busy
                    ? const SizedBox(
                        height: 20,
                        width: 20,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Text('Join the night'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
