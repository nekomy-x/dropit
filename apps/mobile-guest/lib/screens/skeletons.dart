import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// Скелетон загрузки (dizayn-zadanie §5: «состояния каждого экрана —
/// загрузка (скелетоны)»).
///
/// Серые пульсирующие заглушки вместо пустоты: пользователь видит
/// структуру карточек до прихода данных.

/// Пульсирующий блок-заглушка.
class SkeletonBox extends StatefulWidget {
  const SkeletonBox({
    super.key,
    this.width,
    this.height = 16,
    this.borderRadius = 6,
  });

  final double? width;
  final double height;
  final double borderRadius;

  @override
  State<SkeletonBox> createState() => _SkeletonBoxState();
}

class _SkeletonBoxState extends State<SkeletonBox>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1100),
  )..repeat(reverse: true);

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return FadeTransition(
      opacity: Tween(begin: 0.35, end: 0.75).animate(
        CurvedAnimation(parent: _controller, curve: Curves.easeInOut),
      ),
      child: Container(
        width: widget.width,
        height: widget.height,
        decoration: BoxDecoration(
          color: DropTokens.surface2,
          borderRadius: BorderRadius.circular(widget.borderRadius),
        ),
      ),
    );
  }
}

/// Скелетон карточки запроса (гость/DJ — общий паттерн).
class SkeletonRequestCard extends StatelessWidget {
  const SkeletonRequestCard({super.key});

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Row(
          children: [
            // Обложка.
            const SkeletonBox(width: 56, height: 56, borderRadius: 8),
            const SizedBox(width: 14),
            // Строки текста.
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const SkeletonBox(width: 180, height: 18),
                  const SizedBox(height: 8),
                  const SkeletonBox(width: 120, height: 14),
                  const SizedBox(height: 8),
                  const SkeletonBox(width: 70, height: 12),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Список скелетонов (экран запросов/очереди).
class SkeletonList extends StatelessWidget {
  const SkeletonList({super.key, this.count = 5});

  final int count;

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.all(16),
      children: List.generate(count, (_) => const SkeletonRequestCard()),
    );
  }
}

/// Скелетон больших чисел (статистика/дашборд).
class SkeletonStatTile extends StatelessWidget {
  const SkeletonStatTile({super.key});

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          children: [
            const SkeletonBox(width: 60, height: 12),
            const SizedBox(height: 10),
            const SkeletonBox(width: 90, height: 34),
          ],
        ),
      ),
    );
  }
}

/// Скелетон карточки сессии (экран гостя после скана).
class SkeletonSessionCard extends StatelessWidget {
  const SkeletonSessionCard({super.key});

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const SkeletonBox(width: 200, height: 22),
            const SizedBox(height: 10),
            const SkeletonBox(width: 140, height: 14),
            const SizedBox(height: 20),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                const SkeletonBox(width: 110, height: 14),
                const SkeletonBox(width: 60, height: 26),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
