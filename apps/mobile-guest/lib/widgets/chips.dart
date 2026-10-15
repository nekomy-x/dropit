import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// Чип стиля (настройка вечера, фильтры) — DS · 02 «Style chips»:
/// выбранный: заливка `#00FF8726`, рамка `$drop-accent` 2, текст 13 Bold
/// акцентом; невыбранный: фон `$drop-bg`, рамка `$drop-line`, текст цв. muted.
class StyleChip extends StatelessWidget {
  const StyleChip({
    super.key,
    required this.label,
    this.selected = false,
    this.onTap,
    this.height = 38,
  });

  final String label;
  final bool selected;
  final VoidCallback? onTap;
  final double height;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      behavior: HitTestBehavior.opaque,
      child: Container(
        height: height,
        padding: const EdgeInsets.symmetric(horizontal: 16),
        decoration: BoxDecoration(
          color: selected ? DropTokens.tintStrong : DropTokens.bg,
          borderRadius: BorderRadius.circular(height / 2),
          border: Border.all(
            color: selected ? DropTokens.accent : DropTokens.border,
            width: selected ? 2 : 1,
          ),
        ),
        alignment: Alignment.center,
        child: Text(
          label,
          style: DropType.bodySm.copyWith(
            fontSize: 13,
            fontWeight: selected ? FontWeight.w700 : FontWeight.w400,
            color: selected ? DropTokens.accent : DropTokens.muted,
          ),
        ),
      ),
    );
  }
}

/// Фильтр жанра в поиске (guest 03): выбранный — светлая таблетка,
/// текст цветом фона; невыбранный — поверхность с текстом `#CED4D0`.
class FilterPill extends StatelessWidget {
  const FilterPill({
    super.key,
    required this.label,
    this.selected = false,
    this.onTap,
    this.height = 36,
  });

  final String label;
  final bool selected;
  final VoidCallback? onTap;
  final double height;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      behavior: HitTestBehavior.opaque,
      child: Container(
        height: height,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(
          color: selected ? DropTokens.text : DropTokens.surface,
          borderRadius: BorderRadius.circular(height / 2),
        ),
        alignment: Alignment.center,
        child: Text(
          label,
          style: DropType.meta.copyWith(
            fontSize: 11,
            fontWeight: FontWeight.w600,
            color: selected ? DropTokens.bg : DropTokens.chipIdle,
          ),
        ),
      ),
    );
  }
}

/// Таблетка сортировки (DJ-очередь) — оставлена для общих мест.
class SortTab extends StatelessWidget {
  const SortTab({
    super.key,
    required this.label,
    this.selected = false,
    this.onTap,
  });

  final String label;
  final bool selected;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return Expanded(
      child: GestureDetector(
        onTap: onTap,
        behavior: HitTestBehavior.opaque,
        child: Container(
          height: 40,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: selected ? DropTokens.tintStrong : DropTokens.surface,
            borderRadius: BorderRadius.circular(20),
            border: Border.all(
              color: selected ? DropTokens.accent : DropTokens.border,
              width: selected ? 2 : 1,
            ),
          ),
          child: Text(
            label,
            style: DropType.bodySm.copyWith(
              fontSize: 13,
              fontWeight: selected ? FontWeight.w700 : FontWeight.w400,
              color: selected ? DropTokens.accent : DropTokens.muted,
            ),
          ),
        ),
      ),
    );
  }
}
