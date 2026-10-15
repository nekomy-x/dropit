import 'package:flutter/material.dart';

import '../theme/tokens.dart';
import 'artwork.dart';
import 'brand_wave_logo.dart';

/// Шапка внутреннего экрана: стрелка назад + название (Inter 15 SemiBold).
/// Встречается на экранах оплаты и условий (guest 04, 04a, 04b, 04d).
class NavRow extends StatelessWidget {
  const NavRow({super.key, required this.title, this.onBack});

  final String title;
  final VoidCallback? onBack;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        _BackTarget(onTap: onBack),
        const SizedBox(width: 12),
        Text(title, style: DropType.navTitle),
      ],
    );
  }
}

class _BackTarget extends StatelessWidget {
  const _BackTarget({this.onTap});
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      label: 'Back',
      child: GestureDetector(
        onTap: onTap ?? () => Navigator.of(context).maybePop(),
        behavior: HitTestBehavior.opaque,
        child: const SizedBox(
          width: 40,
          height: 40,
          child: Icon(Icons.arrow_back, size: 22, color: DropTokens.text),
        ),
      ),
    );
  }
}

/// Шапка главных экранов (guest 01/03/05): фирменный знак + город.
class BrandBar extends StatelessWidget {
  const BrandBar({
    super.key,
    this.city = 'Berlin',
    this.showCity = true,
    this.trailing,
  });

  final String city;
  final bool showCity;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        const BrandWaveLogo(),
        if (showCity) ...[
          const Spacer(),
          const Icon(Icons.place, size: 15, color: DropTokens.accent),
          const SizedBox(width: 6),
          Text(
            city,
            style: DropType.bodySm.copyWith(
              fontSize: 13,
              color: DropTokens.text,
            ),
          ),
          const SizedBox(width: 6),
          const Icon(Icons.expand_more, size: 14, color: DropTokens.muted2),
        ] else
          const Spacer(),
        if (trailing != null) trailing!,
      ],
    );
  }
}

/// Заголовок экрана (Anton 34) + пояснение под ним (Inter 11–13).
class ScreenHeading extends StatelessWidget {
  const ScreenHeading({
    super.key,
    required this.title,
    this.subtitle,
    this.subtitleStyle,
  });

  final String title;
  final String? subtitle;
  final TextStyle? subtitleStyle;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: DropType.screenTitle),
        if (subtitle != null) ...[
          const SizedBox(height: 10),
          Text(subtitle!, style: subtitleStyle ?? DropType.bodySm),
        ],
      ],
    );
  }
}

/// Микро-лейбл с разрядкой и линиями сверху/снизу, как «ORDER SUMMARY».
class SectionLabel extends StatelessWidget {
  const SectionLabel(this.label, {super.key, this.style});

  final String label;
  final TextStyle? style;

  @override
  Widget build(BuildContext context) {
    return Text(label.toUpperCase(), style: style ?? DropType.label);
  }
}

/// Тонкая линия-разделитель (1px) на всю ширину контейнера.
class Hairline extends StatelessWidget {
  const Hairline({super.key, this.color = DropTokens.divider});

  final Color color;

  @override
  Widget build(BuildContext context) =>
      Container(height: 1, color: color);
}

/// Строка «комната / сессия» под шапкой экрана (guest 03–05):
/// фото 40×40 r6, название Inter 16 SemiBold, подпись 11 muted,
/// справа — зелёная галочка, снизу разделитель.
class JoinedRoomBar extends StatelessWidget {
  const JoinedRoomBar({
    super.key,
    required this.roomName,
    required this.sessionLine,
    this.photoUrl,
    this.padding = const EdgeInsets.fromLTRB(0, 16, 0, 0),
  });

  final String roomName;
  final String sessionLine;
  final String? photoUrl;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: padding,
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: DropTokens.divider)),
      ),
      child: Row(
        children: [
          Artwork(url: photoUrl, size: 40, radius: DropTokens.rXs),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(roomName,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: DropType.title),
                const SizedBox(height: 5),
                Text(sessionLine,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: DropType.meta),
              ],
            ),
          ),
          const SizedBox(width: 8),
          const Icon(Icons.verified, size: 20, color: DropTokens.accent),
        ],
      ),
    );
  }
}

/// Заголовок экрана/секции не нужен — см. `nav.dart`.
