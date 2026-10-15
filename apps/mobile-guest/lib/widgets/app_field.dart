import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// Поле ввода по DS · 02 «Inputs»: высота 52 (поиск — 54), радиус 12,
/// фон `$drop-bg`, рамка `$drop-line`, в фокусе — рамка акцента,
/// ошибка — рамка и текст `$dj-red`.
class AppTextField extends StatelessWidget {
  const AppTextField({
    super.key,
    this.controller,
    this.hint,
    this.prefixIcon,
    this.suffix,
    this.onChanged,
    this.onSubmitted,
    this.errorText,
    this.height = DropTokens.buttonHeightCompact,
    this.enabled = true,
    this.keyboardType,
    this.autofocus = false,
  });

  final TextEditingController? controller;
  final String? hint;
  final IconData? prefixIcon;
  final Widget? suffix;
  final ValueChanged<String>? onChanged;
  final ValueChanged<String>? onSubmitted;
  final String? errorText;
  final double height;
  final bool enabled;
  final TextInputType? keyboardType;
  final bool autofocus;

  @override
  Widget build(BuildContext context) {
    final hasError = errorText != null && errorText!.isNotEmpty;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          height: height,
          child: TextField(
            controller: controller,
            enabled: enabled,
            autofocus: autofocus,
            keyboardType: keyboardType,
            onChanged: onChanged,
            onSubmitted: onSubmitted,
            style: DropType.bodySm.copyWith(color: DropTokens.text),
            cursorColor: DropTokens.accent,
            decoration: InputDecoration(
              hintText: hint,
              filled: true,
              fillColor: DropTokens.bg,
              prefixIcon: prefixIcon == null
                  ? null
                  : Icon(prefixIcon, size: 20, color: DropTokens.muted),
              prefixIconConstraints:
                  const BoxConstraints(minWidth: 48, minHeight: 20),
              suffixIcon: suffix,
              contentPadding: const EdgeInsets.symmetric(horizontal: 16),
              border: _border(hasError ? DropTokens.red : DropTokens.border),
              enabledBorder:
                  _border(hasError ? DropTokens.red : DropTokens.border),
              focusedBorder:
                  _border(hasError ? DropTokens.red : DropTokens.accent),
              disabledBorder: _border(DropTokens.border),
            ),
          ),
        ),
        if (hasError)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text(
              errorText!,
              style: DropType.bodySm.copyWith(color: DropTokens.red),
            ),
          ),
      ],
    );
  }

  static OutlineInputBorder _border(Color color) => OutlineInputBorder(
        borderRadius: BorderRadius.circular(DropTokens.rMd),
        borderSide: BorderSide(color: color),
      );
}
