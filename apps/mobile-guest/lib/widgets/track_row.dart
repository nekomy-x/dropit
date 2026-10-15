import 'package:flutter/material.dart';

import '../theme/tokens.dart';
import 'artwork.dart';

/// Строка трека: обложка 50 r8 + название (Inter 14 SemiBold) + метаданные
/// (Inter 10 muted) + управляющий элемент справа. Используется в поиске,
/// оплате и чеке (guest 03, 04, 05).
class TrackRow extends StatelessWidget {
  const TrackRow({
    super.key,
    required this.title,
    required this.subtitle,
    this.artworkUrl,
    this.trailing,
    this.below,
    this.divider = false,
    this.onTap,
  });

  final String title;
  final String subtitle;
  final String? artworkUrl;
  final Widget? trailing;
  final Widget? below;
  final bool divider;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final row = Row(
      children: [
        Artwork(url: artworkUrl),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: DropType.trackTitle,
              ),
              const SizedBox(height: 5),
              Text(
                subtitle,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: DropType.metaSm,
              ),
            ],
          ),
        ),
        if (trailing != null) ...[const SizedBox(width: 12), trailing!],
      ],
    );

    final content = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        row,
        if (below != null) ...[const SizedBox(height: 6), below!],
      ],
    );

    final padded = Padding(
      padding: EdgeInsets.only(bottom: divider ? 6 : 0),
      child: content,
    );

    if (onTap == null) {
      return divider
          ? Container(
              decoration: const BoxDecoration(
                border: Border(
                  bottom: BorderSide(color: DropTokens.borderSoft),
                ),
              ),
              child: padded,
            )
          : padded;
    }
    return InkWell(
      onTap: onTap,
      child: divider
          ? Container(
              decoration: const BoxDecoration(
                border: Border(
                  bottom: BorderSide(color: DropTokens.borderSoft),
                ),
              ),
              child: padded,
            )
          : padded,
    );
  }
}

/// Строка «лейбл — значение» для итогов и чека (guest 04, 05).
class SummaryRow extends StatelessWidget {
  const SummaryRow({
    super.key,
    required this.label,
    required this.value,
    this.emphasis = false,
    this.valueColor,
    this.labelStyle,
    this.valueStyle,
  });

  final String label;
  final String value;
  final bool emphasis;
  final Color? valueColor;
  final TextStyle? labelStyle;
  final TextStyle? valueStyle;

  @override
  Widget build(BuildContext context) {
    final base = emphasis
        ? DropType.sectionTitle
        : DropType.bodySm.copyWith(
            fontSize: 13,
            color: DropTokens.muted,
          );
    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Text(label, style: labelStyle ?? base),
        Text(
          value,
          style: valueStyle ??
              base.copyWith(color: valueColor ?? (emphasis ? DropTokens.text : null)),
        ),
      ],
    );
  }
}
