/// Общие UI-компоненты гостевого приложения — сборка строго по макету
/// pen.dev (Guest 01–15): таб-бар, бренд-локуп, карточка комнаты, кнопки,
/// чипсы фильтров, экранный каркас.
library;

import 'package:flutter/material.dart';

import '../data/models.dart';
import '../theme/tokens.dart';
import 'brand_wave_logo.dart';

// ────────────────────────────────────────────────────────────────────────
/// Каркас экрана: фон «ночь», безопасные отступы, опциональный таб-бар.
class DropScaffold extends StatelessWidget {
  const DropScaffold({
    super.key,
    required this.body,
    this.tabBar,
    this.bottomBar,
    this.floatingAction,
  });

  final Widget body;
  final Widget? tabBar;
  final Widget? bottomBar;
  final Widget? floatingAction;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: DropTokens.bg,
      body: SafeArea(child: body),
      bottomNavigationBar: bottomBar ??
          (tabBar == null || MediaQuery.viewInsetsOf(context).bottom > 0
              ? null
              : SafeArea(
                  top: false,
                  minimum: const EdgeInsets.fromLTRB(12, 0, 12, 12),
                  child: tabBar!,
                )),
      floatingActionButton: floatingAction,
    );
  }
}

/// Scrollable sheet content above the keyboard and system navigation area.
class DropSheet extends StatelessWidget {
  const DropSheet({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
      child: SafeArea(
        top: false,
        child: SingleChildScrollView(child: child),
      ),
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// Нижняя навигация: 4 таба (Listen · Explore · Requests · Profile),
/// высота 64, радиус 16/пилюля, активный таб — акцент + #F0F4F1.
class DropTabBar extends StatelessWidget {
  const DropTabBar({
    super.key,
    required this.current,
    required this.onChanged,
  });

  final int current;
  final ValueChanged<int> onChanged;

  static const _items = [
    (Icons.headphones, 'Listen'),
    (Icons.explore_outlined, 'Explore'),
    (Icons.queue_music, 'Requests'),
    (Icons.person_outline, 'Profile'),
  ];

  @override
  Widget build(BuildContext context) {
    return Container(
      constraints: BoxConstraints(
          minHeight: DropTokens.tabBarHeight +
              (MediaQuery.textScalerOf(context).scale(12) - 12) * 2),
      decoration: BoxDecoration(
        color: DropTokens.bg,
        borderRadius: BorderRadius.circular(DropTokens.rTile),
        border: Border.all(color: DropTokens.border),
      ),
      child: Row(
        children: [
          for (var i = 0; i < _items.length; i++)
            Expanded(
              child: _TabItem(
                icon: _items[i].$1,
                label: _items[i].$2,
                active: i == current,
                onTap: () => onChanged(i),
              ),
            ),
        ],
      ),
    );
  }
}

class _TabItem extends StatelessWidget {
  const _TabItem({
    required this.icon,
    required this.label,
    required this.active,
    required this.onTap,
  });

  final IconData icon;
  final String label;
  final bool active;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return InkWell(
      borderRadius: BorderRadius.circular(DropTokens.rTile),
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 10),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              icon,
              size: 21,
              color: active ? DropTokens.accent : DropTokens.tabInactive,
            ),
            const SizedBox(height: 4),
            Text(
              label,
              style: TextStyle(
                fontFamily: 'Inter',
                fontSize: 12,
                fontWeight: active ? FontWeight.w600 : FontWeight.w500,
                color: active ? DropTokens.textTabActive : DropTokens.muted,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// Фирменный локуп: шейдерный вордмарк DROP IT + значок наушников.
class BrandLockup extends StatelessWidget {
  const BrandLockup({super.key, this.fontSize = 18, this.markColor});

  final double fontSize;
  final Color? markColor;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        BrandWaveLogo(fontSize: fontSize, markSize: fontSize * 0.85),
      ],
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// «Текущий город»: пин + город + шеврон.
class CityPill extends StatelessWidget {
  const CityPill({super.key, this.color, this.labelColor, this.onTap});

  final Color? color;
  final Color? labelColor;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(999),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.location_on,
                size: 15, color: color ?? DropTokens.accent),
            const SizedBox(width: 4),
            Text(
              'Berlin',
              style: TextStyle(
                fontFamily: 'Inter',
                fontSize: 13,
                color: labelColor ?? DropTokens.text,
              ),
            ),
            const SizedBox(width: 2),
            const Icon(Icons.keyboard_arrow_down,
                size: 14, color: DropTokens.tabInactive),
          ],
        ),
      ),
    );
  }
}

/// Шапка «бренд + город» (гостевые экраны с таб-баром).
class BrandCityRow extends StatelessWidget {
  const BrandCityRow({super.key, this.onCity});

  final VoidCallback? onCity;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 36,
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          const BrandLockup(),
          CityPill(onTap: onCity),
        ],
      ),
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// Карточка присоединённой комнаты (Search/Payment/Request sent/Room).
class JoinRoomCard extends StatelessWidget {
  const JoinRoomCard({super.key, required this.room, this.photo = 40});

  final Room room;
  final double photo;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        ClipRRect(
          borderRadius: BorderRadius.circular(DropTokens.rXs),
          child: Image.asset(
            room.image,
            width: photo,
            height: photo,
            fit: BoxFit.cover,
            errorBuilder: (_, __, ___) => Container(
              width: photo,
              height: photo,
              color: DropTokens.surface2,
            ),
          ),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(room.name,
                  style: DropType.title, overflow: TextOverflow.ellipsis),
              const SizedBox(height: 2),
              Text(
                '${room.session} · ${room.dj} · Live now',
                style: DropType.caption,
                overflow: TextOverflow.ellipsis,
              ),
            ],
          ),
        ),
        const SizedBox(width: 8),
        const Icon(Icons.verified, size: 20, color: DropTokens.accent),
      ],
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// Обложка трека / фото (локальный ассет с заглушкой).
class ArtTile extends StatelessWidget {
  const ArtTile({
    super.key,
    required this.path,
    this.size = 50,
    this.radius = DropTokens.rSm,
  });

  final String path;
  final double size;
  final double radius;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: path.startsWith('http://') || path.startsWith('https://')
          ? Image.network(
              path,
              width: size,
              height: size,
              fit: BoxFit.cover,
              errorBuilder: (_, __, ___) => _fallbackArt(size),
            )
          : Image.asset(
              path,
              width: size,
              height: size,
              fit: BoxFit.cover,
              errorBuilder: (_, __, ___) => _fallbackArt(size),
            ),
    );
  }

  Widget _fallbackArt(double size) => Container(
        width: size,
        height: size,
        decoration: const BoxDecoration(
          gradient: LinearGradient(
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
            colors: [DropTokens.surface2, DropTokens.tint],
          ),
        ),
        child:
            Icon(Icons.music_note, size: size * 0.44, color: DropTokens.accent),
      );
}

// ────────────────────────────────────────────────────────────────────────
/// Главная кнопка (h54, r10, DROP IT green, Inter 16/600 + стрелка).
class PrimaryButton extends StatelessWidget {
  const PrimaryButton({
    super.key,
    required this.label,
    this.onPressed,
    this.icon,
    this.trailingArrow = true,
    this.height = DropTokens.buttonHeight,
    this.loading = false,
  });

  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;
  final bool trailingArrow;
  final double height;
  final bool loading;

  @override
  Widget build(BuildContext context) {
    final enabled = onPressed != null && !loading;
    return ConstrainedBox(
      constraints: BoxConstraints(minHeight: height),
      child: SizedBox(
        width: double.infinity,
        child: Material(
          color: enabled ? DropTokens.accent : DropTokens.surface2,
          borderRadius: BorderRadius.circular(DropTokens.rInput),
          child: InkWell(
            borderRadius: BorderRadius.circular(DropTokens.rInput),
            onTap: enabled ? onPressed : null,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 14),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  if (loading) ...[
                    const SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(
                        strokeWidth: 2,
                        color: DropTokens.onAccent,
                      ),
                    ),
                    const SizedBox(width: 10),
                    Flexible(
                        child: Text('Processing…',
                            style: DropType.button
                                .copyWith(color: DropTokens.muted))),
                  ] else ...[
                    if (icon != null) ...[
                      Icon(icon, size: 20, color: DropTokens.onAccent),
                      const SizedBox(width: 10),
                    ],
                    Flexible(
                      child: Text(
                        label,
                        textAlign: TextAlign.center,
                        style: DropType.button.copyWith(
                          color:
                              enabled ? DropTokens.onAccent : DropTokens.muted,
                        ),
                      ),
                    ),
                    if (trailingArrow && enabled) ...[
                      const SizedBox(width: 12),
                      const Icon(Icons.arrow_forward,
                          size: 20, color: DropTokens.onAccent),
                    ],
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Вторичная кнопка 44px (Find another track · Report an issue).
class SecondaryButton extends StatelessWidget {
  const SecondaryButton({
    super.key,
    required this.label,
    this.onPressed,
    this.icon,
    this.height = 44,
    this.filled = false,
  });

  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;
  final double height;
  final bool filled;

  @override
  Widget build(BuildContext context) {
    return ConstrainedBox(
      constraints: BoxConstraints(minHeight: height),
      child: SizedBox(
        width: double.infinity,
        child: Material(
          color: filled ? DropTokens.surface : Colors.transparent,
          borderRadius: BorderRadius.circular(height / 2),
          child: InkWell(
            borderRadius: BorderRadius.circular(height / 2),
            onTap: onPressed,
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                if (icon != null) ...[
                  Icon(icon, size: 15, color: DropTokens.muted),
                  const SizedBox(width: 8),
                ],
                Flexible(
                    child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 12),
                  child: Text(label,
                      textAlign: TextAlign.center,
                      style:
                          DropType.trackTitle.copyWith(color: DropTokens.text)),
                )),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// Чип фильтра (All · House · Afro house …) — активный акцентный.
///
/// [activeFill]/[activeText] позволяют повторить вариант из макета Search,
/// где активный чип «All» — светлый (`$drop-text`) с тёмной подписью.
class FilterChipPill extends StatelessWidget {
  const FilterChipPill({
    super.key,
    required this.label,
    this.active = false,
    this.onTap,
    this.radius = 999,
    this.activeFill,
    this.activeText,
  });

  final String label;
  final bool active;
  final VoidCallback? onTap;
  final double radius;
  final Color? activeFill;
  final Color? activeText;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: active ? (activeFill ?? DropTokens.accent) : DropTokens.chipIdleBg,
      borderRadius: BorderRadius.circular(radius),
      child: InkWell(
        borderRadius: BorderRadius.circular(radius),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
          child: Text(
            label,
            style: TextStyle(
              fontFamily: 'Inter',
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: active
                  ? (activeText ?? DropTokens.onAccent)
                  : DropTokens.chipIdleText,
            ),
          ),
        ),
      ),
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// Секционная подпись («YOUR NIGHT STARTS HERE», «ACCOUNT», …).
class SectionLabel extends StatelessWidget {
  const SectionLabel(this.text, {super.key, this.color});

  final String text;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      style: TextStyle(
        fontFamily: 'Inter',
        fontSize: 12,
        fontWeight: FontWeight.w600,
        letterSpacing: 1,
        color: color ?? DropTokens.muted,
      ),
    );
  }
}

/// Круглая стеклянная кнопка поверх камеры (Flash / Gallery / Close).
class GhostCircleButton extends StatelessWidget {
  const GhostCircleButton({
    super.key,
    required this.icon,
    this.onPressed,
    this.size = 52,
    this.fill = DropTokens.ghostFill12,
    this.bordered = true,
    this.iconColor,
  });

  final IconData icon;
  final VoidCallback? onPressed;
  final double size;
  final Color fill;
  final bool bordered;
  final Color? iconColor;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: size,
      height: size,
      child: Material(
        color: fill,
        shape: CircleBorder(
          side: bordered
              ? const BorderSide(color: DropTokens.ghostStroke)
              : BorderSide.none,
        ),
        child: InkWell(
          customBorder: const CircleBorder(),
          onTap: onPressed,
          child: Icon(icon,
              size: size * 0.42, color: iconColor ?? DropTokens.text),
        ),
      ),
    );
  }
}

/// Строка «подпись + значение + опциональный шеврон» (Profile, Identity).
class SettingsRow extends StatelessWidget {
  const SettingsRow({
    super.key,
    required this.label,
    this.value,
    this.icon,
    this.onTap,
    this.height = 46,
    this.labelColor,
    this.valueColor,
    this.showChevron = true,
  });

  final String label;
  final String? value;
  final IconData? icon;
  final VoidCallback? onTap;
  final double height;
  final Color? labelColor;
  final Color? valueColor;
  final bool showChevron;

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onTap,
      child: Container(
        constraints: BoxConstraints(minHeight: height),
        padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 8),
        child: Row(
          children: [
            if (icon != null) ...[
              Icon(icon, size: 18, color: DropTokens.muted),
              const SizedBox(width: 14),
            ],
            Expanded(
              child: Text(
                label,
                style: DropType.bodySm.copyWith(
                  fontSize: 13,
                  color: labelColor ?? DropTokens.text,
                ),
              ),
            ),
            if (value != null)
              Flexible(
                  child: Text(
                value!,
                textAlign: TextAlign.end,
                style: DropType.caption
                    .copyWith(color: valueColor ?? DropTokens.muted),
              )),
            if (showChevron) ...[
              const SizedBox(width: 6),
              const Icon(Icons.chevron_right,
                  size: 15, color: DropTokens.tabInactive),
            ],
          ],
        ),
      ),
    );
  }
}

/// Бейдж «LIVE NOW» / «LIVE» с пульсирующей точкой.
class LiveBadge extends StatelessWidget {
  const LiveBadge({
    super.key,
    required this.label,
    this.fill = DropTokens.liveTint,
    this.color = DropTokens.live,
  });

  final String label;
  final Color fill;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: fill,
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 6,
            height: 6,
            decoration: BoxDecoration(color: color, shape: BoxShape.circle),
          ),
          const SizedBox(width: 6),
          Text(
            label,
            style: TextStyle(
              fontFamily: 'Inter',
              fontSize: 12,
              fontWeight: FontWeight.w700,
              color: color,
            ),
          ),
        ],
      ),
    );
  }
}

/// Поле ввода в стиле макета: линия под значением, а не рамка.
///
/// То же правило, что и у веб-панелей (`.field-input` в
/// `packages/ui/panel.css`): ни заливки, ни радиуса, ни боковых границ —
/// только 1px линия снизу, которая становится акцентной в фокусе.
class DropField extends StatefulWidget {
  const DropField({
    super.key,
    required this.hint,
    this.prefixIcon,
    this.suffixIcon,
    this.onSuffixPressed,
    this.suffixTooltip,
    this.controller,
    this.obscure = false,
    this.height = 52,
    this.keyboardType,
    this.onSubmitted,
  });

  final String hint;
  final IconData? prefixIcon;
  final IconData? suffixIcon;
  final VoidCallback? onSuffixPressed;
  final String? suffixTooltip;
  final TextEditingController? controller;
  final bool obscure;
  final double height;
  final TextInputType? keyboardType;
  final ValueChanged<String>? onSubmitted;

  @override
  State<DropField> createState() => _DropFieldState();
}

class _DropFieldState extends State<DropField> {
  final FocusNode _focusNode = FocusNode();
  bool _focused = false;

  @override
  void initState() {
    super.initState();
    _focusNode.addListener(_onFocusChanged);
  }

  void _onFocusChanged() {
    if (_focused != _focusNode.hasFocus) {
      setState(() => _focused = _focusNode.hasFocus);
    }
  }

  @override
  void dispose() {
    _focusNode.removeListener(_onFocusChanged);
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      height: widget.height,
      decoration: BoxDecoration(
        color: Colors.transparent,
        border: Border(
          bottom: BorderSide(
            color: _focused ? DropTokens.accent : DropTokens.border,
            width: 1,
          ),
        ),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 0),
      child: Row(
        children: [
          if (widget.prefixIcon != null) ...[
            Icon(widget.prefixIcon, size: 18, color: DropTokens.muted),
            const SizedBox(width: 10),
          ],
          Expanded(
            child: TextField(
              controller: widget.controller,
              focusNode: _focusNode,
              obscureText: widget.obscure,
              autocorrect: widget.onSuffixPressed == null && !widget.obscure,
              enableSuggestions:
                  widget.onSuffixPressed == null && !widget.obscure,
              keyboardType: widget.keyboardType,
              onSubmitted: widget.onSubmitted,
              style: DropType.bodySm.copyWith(fontSize: 13),
              cursorColor: DropTokens.accent,
              decoration: InputDecoration(
                border: InputBorder.none,
                enabledBorder: InputBorder.none,
                focusedBorder: InputBorder.none,
                errorBorder: InputBorder.none,
                disabledBorder: InputBorder.none,
                isCollapsed: true,
                filled: false,
                hintText: widget.hint,
                hintStyle: DropType.bodySm
                    .copyWith(fontSize: 13, color: DropTokens.muted),
              ),
            ),
          ),
          if (widget.suffixIcon != null)
            IconButton(
              tooltip: widget.suffixTooltip,
              onPressed: widget.onSuffixPressed,
              icon: Icon(widget.suffixIcon, size: 18, color: DropTokens.muted),
            ),
        ],
      ),
    );
  }
}

/// Подпись поля формы (Inter 12/500, muted).
class FieldLabel extends StatelessWidget {
  const FieldLabel(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 8, left: 2),
      child: Text(
        text,
        style: const TextStyle(
          fontFamily: 'Inter',
          fontSize: 12,
          fontWeight: FontWeight.w500,
          color: DropTokens.muted,
        ),
      ),
    );
  }
}

/// Логотип Apple (макет Guest 08/09 — белый).
class AppleGlyph extends CustomPainter {
  const AppleGlyph({this.color = Colors.white});

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = color;
    final body = Path()
      ..moveTo(size.width * 0.5, size.height * 0.28)
      ..cubicTo(size.width * 0.62, size.height * 0.16, size.width * 0.86,
          size.height * 0.2, size.width * 0.86, size.height * 0.52)
      ..cubicTo(size.width * 0.86, size.height * 0.78, size.width * 0.68,
          size.height, size.width * 0.52, size.height)
      ..cubicTo(size.width * 0.47, size.height, size.width * 0.44,
          size.height * 0.94, size.width * 0.38, size.height * 0.94)
      ..cubicTo(size.width * 0.3, size.height * 0.94, size.width * 0.26,
          size.height, size.width * 0.18, size.height * 0.76)
      ..cubicTo(size.width * 0.04, size.height * 0.42, size.width * 0.1,
          size.height * 0.18, size.width * 0.3, size.height * 0.16)
      ..cubicTo(size.width * 0.38, size.height * 0.15, size.width * 0.44,
          size.height * 0.2, size.width * 0.5, size.height * 0.2)
      ..cubicTo(size.width * 0.56, size.height * 0.2, size.width * 0.62,
          size.height * 0.15, size.width * 0.5, size.height * 0.28)
      ..close();
    canvas.drawPath(body, paint);
    final leaf = Path()
      ..moveTo(size.width * 0.5, size.height * 0.26)
      ..cubicTo(size.width * 0.58, size.height * 0.08, size.width * 0.72,
          size.height * 0.02, size.width * 0.82, size.height * 0.04)
      ..cubicTo(size.width * 0.84, size.height * 0.16, size.width * 0.72,
          size.height * 0.24, size.width * 0.62, size.height * 0.26)
      ..close();
    canvas.drawPath(leaf, paint);
  }

  @override
  bool shouldRepaint(covariant AppleGlyph oldDelegate) =>
      oldDelegate.color != color;
}

/// Логотип Google (макет Guest 08/09 — белый).
class GoogleGlyph extends CustomPainter {
  const GoogleGlyph({this.color = Colors.white});

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = size.width * 0.17
      ..strokeCap = StrokeCap.round;
    final r = size.width * 0.36;
    final center = Offset(size.width * 0.5, size.height * 0.5);
    // Дуга «G» с разрывом справа (от ~30° против часовой до ~300°).
    canvas.drawArc(
      Rect.fromCircle(center: center, radius: r),
      0.35,
      4.6,
      false,
      paint,
    );
    // Перекладина «G» вправо от центра.
    canvas.drawLine(
      Offset(size.width * 0.52, size.height * 0.5),
      Offset(size.width * 0.9, size.height * 0.5),
      paint,
    );
  }

  @override
  bool shouldRepaint(covariant GoogleGlyph oldDelegate) =>
      oldDelegate.color != color;
}
