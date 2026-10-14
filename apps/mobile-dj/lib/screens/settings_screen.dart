import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../main.dart';

/// Настройка вечера (§1.3): тумблер приёма запросов (крупный,
/// доступный одной рукой) + индикация смены.
///
/// Стили-чипы: сохранение стилей сессии — эндпоинт бэкенда пока не
/// реализован (см. tehnicheskaya-karta F4-остатки); чипы показывают
/// выбранное локально, помечено TODO в комментарии.
class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key, required this.api});

  final ApiClient api;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  ShiftInfo? _shift;
  bool _accepting = true;
  bool _busy = false;
  String? _error;

  // Стили вечера (§1.3: можно менять в течение ночи).
  // ЗАГЛУШКА (правило 5): персист на бэкенд — эндпоит не реализован.
  static const _allStyles = [
    'Salsa', 'Bachata', 'Merengue', 'Kizomba', 'Rock', 'Jazz', 'Pop', 'Reggaeton',
  ];
  final Set<String> _selected = {};

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final shift = await widget.api.shift();
      setState(() {
        _shift = shift;
      });
    } on ApiException catch (e) {
      setState(() => _error = e.message);
    }
  }

  Future<void> _toggle() async {
    setState(() => _busy = true);
    try {
      if (_accepting) {
        await widget.api.pauseAcceptance();
        setState(() => _accepting = false);
      } else {
        await widget.api.resumeAcceptance();
        setState(() => _accepting = true);
      }
    } on ApiException catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(e.message), backgroundColor: DjTheme.red),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Night setup')),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(20),
          children: [
            // Смена (§1.2): клуб, статы.
            Card(
              child: Padding(
                padding: const EdgeInsets.all(20),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(_shift == null
                        ? 'No active shift'
                        : 'Shift active (${_shift!.status})'),
                    if (_shift != null)
                      Text(_shift!.timezone,
                          style: TextStyle(color: DjTheme.grey)),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 24),
            // Тумблер приёма — КРУПНЫЙ, одной рукой (§1.3).
            Text('Request intake', style: Theme.of(context).textTheme.titleLarge),
            const SizedBox(height: 8),
            FilledButton(
              onPressed: _busy ? null : _toggle,
              style: FilledButton.styleFrom(
                minimumSize: const Size.fromHeight(72),
                backgroundColor: _accepting ? DjTheme.neon : DjTheme.surface,
                foregroundColor: _accepting ? DjTheme.black : DjTheme.white,
                side: const BorderSide(color: DjTheme.grey),
              ),
              child: _busy
                  ? const SizedBox(
                      height: 22,
                      width: 22,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : Text(
                      _accepting ? 'OPEN — accepting requests' : 'PAUSED',
                      style: const TextStyle(
                          fontSize: 22, fontWeight: FontWeight.w800),
                    ),
            ),
            const SizedBox(height: 24),
            // Стили-чипы (§1.3).
            Text('Styles you accept tonight',
                style: Theme.of(context).textTheme.titleLarge),
            const SizedBox(height: 4),
            Text('Chip persistence — backend endpoint pending (stub, rule 5)',
                style: TextStyle(color: DjTheme.grey, fontSize: 12)),
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: _allStyles.map((style) {
                final on = _selected.contains(style);
                return FilterChip(
                  label: Text(style, style: const TextStyle(fontSize: 16)),
                  selected: on,
                  selectedColor: DjTheme.neon,
                  backgroundColor: DjTheme.surface,
                  labelStyle: TextStyle(
                    color: on ? DjTheme.black : DjTheme.white,
                    fontWeight: FontWeight.w700,
                  ),
                  onSelected: (_) {
                    setState(() {
                      on ? _selected.remove(style) : _selected.add(style);
                    });
                  },
                );
              }).toList(),
            ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(top: 16),
                child: Text(_error!, style: TextStyle(color: DjTheme.red)),
              ),
          ],
        ),
      ),
    );
  }
}
