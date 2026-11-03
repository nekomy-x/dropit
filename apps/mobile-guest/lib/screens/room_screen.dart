/// Guest · 12 · Room · Now playing.
///
/// Макет pen.dev: карточка комнаты с live-бейджем, блок «сейчас играет»
/// с эквалайзером и прогрессом сета, очередь «up next» и кнопка запроса
/// трека.
library;

import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../data/models.dart';
import '../state/app_state.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class RoomScreen extends StatefulWidget {
  const RoomScreen({
    super.key,
    required this.room,
    required this.onBack,
    required this.onRequestTrack,
  });

  final Room room;
  final VoidCallback onBack;
  final VoidCallback onRequestTrack;

  @override
  State<RoomScreen> createState() => _RoomScreenState();
}

class _RoomScreenState extends State<RoomScreen>
    with SingleTickerProviderStateMixin {
  late final AnimationController _eq;

  /// Высоты 42 полос эквалайзера — из макета (Bar 1…42).
  static const _barHeights = <double>[
    10, 18, 28, 14, 34, 22, 12, 30, 18, 36, 24, 14, 28, 16, 32, 20, //
    10, 26, 18, 12, 30, 22, 14, 34, 18, 26, 12, 20, 28, 16, 24, 10, //
    32, 18, 22, 28, 14, 36, 20, 12, 30, 26,
  ];

  @override
  void initState() {
    super.initState();
    _eq = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1200),
    )..repeat();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (MediaQuery.disableAnimationsOf(context)) {
      _eq.stop();
    } else if (!_eq.isAnimating) {
      _eq.repeat();
    }
  }

  @override
  void dispose() {
    _eq.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = AppState.of(context);
    return DropScaffold(
      tabBar: DropTabBar(
        current: 0,
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
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: BrandCityRow(),
          ),
          const SizedBox(height: 18),

          // ── Карточка комнаты ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: DropTokens.surface,
                borderRadius: BorderRadius.circular(DropTokens.rLg),
                border: Border.all(color: DropTokens.border),
              ),
              child: Column(
                children: [
                  Row(
                    children: [
                      ArtTile(
                        path: widget.room.image,
                        size: 48,
                        radius: DropTokens.rXs,
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(widget.room.name,
                                style: DropType.roomName,
                                overflow: TextOverflow.ellipsis),
                            const SizedBox(height: 2),
                            Text(
                              '${widget.room.session} · ${widget.room.dj}',
                              style: DropType.caption,
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(width: 8),
                      const LiveBadge(
                          label: 'LIVE NOW', fill: DropTokens.tint1A),
                    ],
                  ),
                  const SizedBox(height: 10),
                  Row(
                    children: [
                      const Icon(Icons.people_outline,
                          size: 14, color: DropTokens.muted),
                      const SizedBox(width: 6),
                      Expanded(
                          child: Text(
                        'Requests open · set till 06:00',
                        style: DropType.caption.copyWith(
                          fontWeight: FontWeight.w500,
                        ),
                      )),
                    ],
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 24),

          // ── Сейчас играет ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    const Row(
                      children: [
                        Icon(Icons.album, size: 16, color: DropTokens.accent),
                        SizedBox(width: 8),
                        Text('NOW PLAYING', style: DropType.labelTight),
                      ],
                    ),
                    Text('Sunrise set',
                        style: DropType.caption.copyWith(
                          fontWeight: FontWeight.w500,
                        )),
                  ],
                ),
                const SizedBox(height: 14),
                const Text('Move', style: DropType.nowPlayingTitle),
                const SizedBox(height: 6),
                Text('Adam Port, Stryv',
                    style: DropType.bodySm.copyWith(
                      fontSize: 13,
                      fontWeight: FontWeight.w500,
                      color: DropTokens.muted,
                    )),
                const SizedBox(height: 20),

                // Эквалайзер.
                SizedBox(
                  height: 36,
                  child: AnimatedBuilder(
                    animation: _eq,
                    builder: (context, _) {
                      return Row(
                        crossAxisAlignment: CrossAxisAlignment.end,
                        children: [
                          for (var i = 0; i < _barHeights.length; i++)
                            Expanded(
                              child: _EqBar(
                                height: _barHeights[i],
                                active: i % 4 == 0,
                                phase: (_eq.value * 6.28 + i * 0.35) % 6.28,
                              ),
                            ),
                        ],
                      );
                    },
                  ),
                ),
                const SizedBox(height: 16),

                // Прогресс сета.
                ClipRRect(
                  borderRadius: BorderRadius.circular(2),
                  child: const LinearProgressIndicator(
                    value: 205 / 350,
                    minHeight: 4,
                    backgroundColor: DropTokens.tint1F,
                    valueColor: AlwaysStoppedAnimation(DropTokens.accent),
                  ),
                ),
                const SizedBox(height: 8),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Text('1:42',
                        style: DropType.caption
                            .copyWith(fontWeight: FontWeight.w500)),
                    Text('4:10',
                        style: DropType.caption
                            .copyWith(fontWeight: FontWeight.w500)),
                  ],
                ),
              ],
            ),
          ),
          const SizedBox(height: 28),

          // ── Очередь ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Text('UP NEXT', style: DropType.labelTight),
                    Text('3 in queue', style: DropType.caption),
                  ],
                ),
                const SizedBox(height: 10),
                for (final entry in _queue) ...[
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 10),
                    child: Row(
                      children: [
                        Text(entry.$1, style: DropType.stepPosition),
                        const SizedBox(width: 14),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(entry.$2,
                                  style: DropType.trackTitle,
                                  overflow: TextOverflow.ellipsis),
                              const SizedBox(height: 2),
                              Text(entry.$3, style: DropType.caption),
                            ],
                          ),
                        ),
                        const Icon(Icons.chevron_right,
                            size: 16, color: DropTokens.muted),
                      ],
                    ),
                  ),
                  const Divider(color: DropTokens.border),
                ],
              ],
            ),
          ),
          const SizedBox(height: 20),

          // ── Запросить трек ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Column(
              children: [
                PrimaryButton(
                  label: 'Request a track',
                  icon: Icons.add,
                  trailingArrow: false,
                  onPressed: widget.onRequestTrack,
                ),
                const SizedBox(height: 12),
                const Text(
                  'Requests are voted on by the room · DJ has final say',
                  style: DropType.caption,
                  textAlign: TextAlign.center,
                ),
              ],
            ),
          ),
          const SizedBox(height: 24),
        ],
      ),
    );
  }
}

/// Очередь из макета: позиция, трек, автор.
const _queue = [
  ('01', 'Rhythm of the Night', 'Corona · by @lena_k'),
  ('02', 'Sunshine & Whiskey', 'RÜFÜS DU SOL · by @marco'),
  ('03', 'Innerbloom', 'What So Not · by @sasha'),
];

class _EqBar extends StatelessWidget {
  const _EqBar({
    required this.height,
    required this.active,
    required this.phase,
  });

  final double height;
  final bool active;
  final double phase;

  @override
  Widget build(BuildContext context) {
    // Пульсация: каждая полоса дышит со своей фазой.
    final factor = 0.55 + 0.45 * (1 + math.sin(phase)) / 2;
    return Container(
      height: height * factor,
      margin: const EdgeInsets.symmetric(horizontal: 1),
      decoration: BoxDecoration(
        color: active ? DropTokens.accent : DropTokens.eqBarIdle,
        borderRadius: BorderRadius.circular(2),
      ),
    );
  }
}
