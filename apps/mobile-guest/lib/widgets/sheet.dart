import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// Шторка снизу (guest 04b «Choose payment method», guest 04c, guest 04d):
/// радиус верхних углов 24, ручка 36×4, заголовок Anton 28, отступы 20/24.
class AppSheet extends StatelessWidget {
  const AppSheet({
    super.key,
    required this.title,
    required this.children,
    this.leading,
    this.trailing,
    this.titleGap = 22,
  });

  final String title;
  final List<Widget> children;
  final Widget? leading;
  final Widget? trailing;
  final double titleGap;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(
        bottom: MediaQuery.of(context).viewInsets.bottom,
      ),
      child: Container(
        decoration: const BoxDecoration(
          color: DropTokens.sheet,
          borderRadius: BorderRadius.vertical(
            top: Radius.circular(DropTokens.rSheet),
          ),
        ),
        padding: const EdgeInsets.fromLTRB(20, 12, 20, 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Center(
              child: Container(
                width: 36,
                height: 4,
                decoration: BoxDecoration(
                  color: DropTokens.handle,
                  borderRadius: BorderRadius.circular(2),
                ),
              ),
            ),
            SizedBox(height: titleGap),
            Row(
              children: [
                if (leading != null) ...[leading!, const SizedBox(width: 12)],
                Expanded(
                  child: Text(title, style: DropType.sheetTitle),
                ),
                if (trailing != null) trailing!,
              ],
            ),
            const SizedBox(height: 20),
            ...children,
          ],
        ),
      ),
    );
  }
}

/// Строка выбора способа оплаты в шторке (guest 04b).
class PaymentMethodRow extends StatelessWidget {
  const PaymentMethodRow({
    super.key,
    required this.title,
    required this.subtitle,
    required this.brandLabel,
    this.brandColor = const Color(0xFF1434CB),
    this.selected = false,
    this.onTap,
  });

  final String title;
  final String subtitle;
  final String brandLabel;
  final Color brandColor;
  final bool selected;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      behavior: HitTestBehavior.opaque,
      child: Container(
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          color: selected ? DropTokens.tintFaint : DropTokens.sheetRow,
          borderRadius: BorderRadius.circular(DropTokens.rMd),
          border: Border.all(
            color: selected ? DropTokens.accent : DropTokens.borderSoft,
          ),
        ),
        child: Row(
          children: [
            _BrandBadge(label: brandLabel, color: brandColor),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    title,
                    style: DropType.bodySm.copyWith(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const SizedBox(height: 5),
                  Text(subtitle, style: DropType.meta),
                ],
              ),
            ),
            Icon(
              selected ? Icons.check_circle : Icons.circle_outlined,
              size: 20,
              color: selected ? DropTokens.accent : DropTokens.inactive,
            ),
          ],
        ),
      ),
    );
  }
}

/// Бейдж платёжной системы 40×28 (в макете — векторные логотипы;
/// здесь — их текстовый эквивалент, чтобы не тянуть чужие ассеты).
class _BrandBadge extends StatelessWidget {
  const _BrandBadge({required this.label, required this.color});

  final String label;
  final Color color;

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
        style: TextStyle(
          fontFamily: 'Inter',
          fontSize: 9,
          fontWeight: FontWeight.w800,
          letterSpacing: -0.2,
          color: color,
        ),
      ),
    );
  }
}

/// Поле формы карты (guest 04d): подпись 11 muted + поле 48 r10.
class SheetField extends StatelessWidget {
  const SheetField({
    super.key,
    required this.label,
    required this.placeholder,
    this.icon,
    this.controller,
    this.keyboardType,
    this.maxLength,
  });

  final String label;
  final String placeholder;
  final IconData? icon;
  final TextEditingController? controller;
  final TextInputType? keyboardType;
  final int? maxLength;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label,
            style: DropType.meta.copyWith(
              fontSize: 11,
              fontWeight: FontWeight.w500,
            )),
        const SizedBox(height: 8),
        SizedBox(
          height: DropTokens.fieldHeight,
          child: TextField(
            controller: controller,
            keyboardType: keyboardType,
            maxLength: maxLength,
            style: DropType.bodySm.copyWith(fontSize: 13),
            cursorColor: DropTokens.accent,
            decoration: InputDecoration(
              counterText: '',
              hintText: placeholder,
              hintStyle: DropType.bodySm.copyWith(
                fontSize: 13,
                color: DropTokens.muted,
              ),
              filled: true,
              fillColor: DropTokens.sheetRow,
              suffixIcon: icon == null
                  ? null
                  : Icon(icon, size: 20, color: DropTokens.muted),
              contentPadding: const EdgeInsets.symmetric(horizontal: 14),
              border: _border(),
              enabledBorder: _border(),
              focusedBorder: _border(color: DropTokens.accent),
            ),
          ),
        ),
      ],
    );
  }

  static OutlineInputBorder _border({Color color = DropTokens.border}) =>
      OutlineInputBorder(
        borderRadius: BorderRadius.circular(DropTokens.rInput),
        borderSide: BorderSide(color: color),
      );
}
