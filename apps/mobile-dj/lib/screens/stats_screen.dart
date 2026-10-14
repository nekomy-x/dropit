import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../main.dart';
import 'skeletons.dart';

/// Статистика вечера (§1.7): принято/отклонено/сыграно, заработано
/// (доля DJ), пик по времени.
class StatsScreen extends StatefulWidget {
  const StatsScreen({super.key, required this.api});

  final ApiClient api;

  @override
  State<StatsScreen> createState() => _StatsScreenState();
}

class _StatsScreenState extends State<StatsScreen> {
  DjStats? _stats;
  String? _error;
  bool _busy = false;

  Future<void> _load() async {
    setState(() => _busy = true);
    try {
      final stats = await widget.api.statsToday();
      setState(() => _stats = stats);
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
  }

  @override
  Widget build(BuildContext context) {
    if (_busy && _stats == null) {
      // Скелетоны больших чисел (§5).
      return Scaffold(
        appBar: AppBar(title: const Text('Tonight')),
        body: ListView(
          padding: const EdgeInsets.all(20),
          children: [
            Card(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: const Column(children: [SkeletonBox(height: 44, width: 120)]),
              ),
            ),
            const SizedBox(height: 16),
            const Row(children: [
              Expanded(child: SkeletonStatTile()),
              SizedBox(width: 8),
              Expanded(child: SkeletonStatTile()),
            ]),
            const SizedBox(height: 8),
            const Row(children: [
              Expanded(child: SkeletonStatTile()),
              SizedBox(width: 8),
              Expanded(child: SkeletonStatTile()),
            ]),
          ],
        ),
      );
    }
    return Scaffold(
      appBar: AppBar(title: const Text('Tonight')),
      body: RefreshIndicator(
        onRefresh: _load,
        color: DjTheme.neon,
        child: ListView(
          padding: const EdgeInsets.all(20),
          children: [
            // Заработок за вечер — крупно (§1.7).
            Card(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Column(
                  children: [
                    Text('Earned tonight',
                        style: TextStyle(color: DjTheme.grey, fontSize: 16)),
                    const SizedBox(height: 4),
                    Text(
                      _stats?.earnedLabel ?? '—',
                      style: const TextStyle(
                        fontSize: 48,
                        fontWeight: FontWeight.w800,
                        color: DjTheme.neon,
                      ),
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 16),
            Row(
              children: [
                Expanded(
                    child: _statTile('Requests', _stats?.requestsToday, DjTheme.white)),
                const SizedBox(width: 8),
                Expanded(
                    child: _statTile('Accepted', _stats?.accepted, DjTheme.neon)),
              ],
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                    child: _statTile('Played', _stats?.played, DjTheme.neon)),
                const SizedBox(width: 8),
                Expanded(
                    child: _statTile('Rejected', _stats?.rejected, DjTheme.red)),
              ],
            ),
            const SizedBox(height: 16),
            if (_stats != null)
              Text(
                'Acceptance rate: ${_stats!.requestsToday == 0 ? 0 : ((_stats!.accepted / _stats!.requestsToday) * 100).round()}%',
                style: const TextStyle(fontSize: 18),
              ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(top: 16),
                child: Text(_error!, style: TextStyle(color: DjTheme.red)),
              ),
            // Пик по времени (§1.7) — эндпоинт не реализован: заглушка.
            const SizedBox(height: 16),
            Text(
              'Peak hours chart — pending backend endpoint (stub, rule 5)',
              style: TextStyle(color: DjTheme.grey, fontSize: 12),
            ),
          ],
        ),
      ),
    );
  }

  Widget _statTile(String label, int? value, Color color) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          children: [
            Text(label, style: TextStyle(color: DjTheme.grey, fontSize: 14)),
            const SizedBox(height: 4),
            Text(
              '${value ?? '—'}',
              style: TextStyle(
                  fontSize: 34, fontWeight: FontWeight.w800, color: color),
            ),
          ],
        ),
      ),
    );
  }
}
