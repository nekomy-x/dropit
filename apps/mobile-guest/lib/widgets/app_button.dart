import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// Главная кнопка приложения — по DS · 02 «Buttons — states»:
/// высота 54, радиус 27, заливка `$drop-accent`, текст Inter 16 SemiBold
/// цветом `#101813`, справа стрелка 20.
///
/// Поддерживает все состояния из макета: default, pressed, disabled,
/// loading («Processing…» + спиннер) и вариант outline secondary.
class AppButton extends StatelessWidget {
  const AppButton({
    super.key,
    required this.label,
    this.onPressed,
    this.icon,
    this.loading = false,
    this.variant = AppButtonVariant.primary,
    this.height = DropTokens.buttonHeight,
    this.trailingArrow = true,
  });

  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;
  final bool loading;
  final AppButtonVariant variant;
  final double height;
  final bool trailingArrow;

  bool get _enabled => onPressed != null && !loading;

  @override
  Widget build(BuildContext context) {
    final isPrimary = variant == AppButtonVariant.primary;
    final bg = switch (variant) {
      AppButtonVariant.primary => DropTokens.accent,
      AppButtonVariant.outline => DropTokens.surface,
      AppButtonVariant.destructive => DropTokens.redSoft,
      AppButtonVariant.ghost => Colors.transparent,
    };
    final fg = switch (variant) {
      AppButtonVariant.primary => DropTokens.onAccent,
      AppButtonVariant.outline => DropTokens.text,
      AppButtonVariant.destructive => DropTokens.red,
      AppButtonVariant.ghost => DropTokens.accent,
    };
    final borderColor = switch (variant) {
      AppButtonVariant.outline => DropTokens.border,
      _ => Colors.transparent,
    };

    final child = loading
        ? Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('Processing…', style: DropType.button.copyWith(color: fg)),
              const SizedBox(width: 10),
              SizedBox(
                width: 18,
                height: 18,
                child: CircularProgressIndicator(strokeWidth: 2, color: fg),
              ),
            ],
          )
        : Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (icon != null) ...[
                Icon(icon, size: 20, color: fg),
                const SizedBox(width: 10),
              ],
              Flexible(
                child: Text(
                  label,
                  textAlign: TextAlign.center,
                  style: DropType.button.copyWith(color: fg),
                ),
              ),
              if (trailingArrow && isPrimary) ...[
                const SizedBox(width: 12),
                Icon(Icons.arrow_forward, size: 20, color: fg),
              ],
            ],
          );

    return Semantics(
      button: true,
      enabled: _enabled,
      label: label,
      child: SizedBox(
        height: height,
        width: double.infinity,
        child: Material(
          color: _enabled ? bg : DropTokens.surface2,
          borderRadius: BorderRadius.circular(height / 2),
          child: InkWell(
            borderRadius: BorderRadius.circular(height / 2),
            onTap: _enabled ? onPressed : null,
            splashColor: isPrimary ? DropTokens.accentPressed : null,
            highlightColor: isPrimary
                ? DropTokens.accentPressed.withValues(alpha: 0.6)
                : null,
            child: Container(
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(height / 2),
                border: Border.all(color: borderColor),
              ),
              padding: const EdgeInsets.symmetric(horizontal: 20),
              alignment: Alignment.center,
              child: _enabled
                  ? child
                  : Opacity(
                      opacity: variant == AppButtonVariant.ghost ? 0.5 : 1,
                      child: DefaultTextStyle.merge(
                        style: const TextStyle(color: DropTokens.muted),
                        child: IconTheme(
                          data: const IconThemeData(color: DropTokens.muted),
                          child: child,
                        ),
                      ),
                    ),
            ),
          ),
        ),
      ),
    );
  }
}

enum AppButtonVariant { primary, outline, destructive, ghost }

/// Круглая кнопка-действие (например «+» у рекомендации трека).
class AppIconButton extends StatelessWidget {
  const AppIconButton({
    super.key,
    required this.icon,
    this.onPressed,
    this.size = 34,
    this.filled = true,
    this.tooltip,
  });

  final IconData icon;
  final VoidCallback? onPressed;
  final double size;
  final bool filled;
  final String? tooltip;

  @override
  Widget build(BuildContext context) {
    final button = SizedBox(
      width: size,
      height: size,
      child: Material(
        color: filled ? DropTokens.accent : Colors.transparent,
        shape: CircleBorder(
          side: filled
              ? BorderSide.none
              : const BorderSide(color: DropTokens.border),
        ),
        child: InkWell(
          customBorder: const CircleBorder(),
          onTap: onPressed,
          child: Icon(
            icon,
            size: size * 0.53,
            color: filled ? DropTokens.onAccent : DropTokens.text,
          ),
        ),
      ),
    );
    return tooltip == null ? button : Tooltip(message: tooltip!, child: button);
  }
}

/// Кнопка-иконка в стеклянном круге поверх камеры (guest 02).
class GhostIconButton extends StatelessWidget {
  const GhostIconButton({
    super.key,
    required this.icon,
    this.onPressed,
    this.size = 52,
  });

  final IconData icon;
  final VoidCallback? onPressed;
  final double size;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: size,
      height: size,
      child: Material(
        color: DropTokens.ghostFill,
        shape: const CircleBorder(
          side: BorderSide(color: DropTokens.ghostStroke),
        ),
        child: InkWell(
          customBorder: const CircleBorder(),
          onTap: onPressed,
          child: Icon(icon, size: size * 0.42, color: DropTokens.text),
        ),
      ),
    );
  }
}
