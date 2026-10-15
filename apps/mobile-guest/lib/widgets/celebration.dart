import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// Праздничный блок экрана «запрос отправлен» (guest 05):
/// мягкое изумрудное свечение, две волны-ряби, зелёный бейдж с галочкой,
/// конфетти и искры. Геометрия взята из макета (кадр 350×128).
class Celebration extends StatelessWidget {
  const Celebration({super.key, this.width = 350});

  final double width;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: width,
      height: 128,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          // Soft emerald glow.
          const Positioned(
            left: 61,
            top: 0,
            child: _Glow(),
          ),
          // Success ripples.
          _ripple(113, 2, 124, const Color(0x2000FF87)),
          _ripple(124, 13, 102, const Color(0x4800FF87)),
          // Success badge.
          Positioned(
            left: 135,
            top: 24,
            child: Container(
              width: 80,
              height: 80,
              decoration: const BoxDecoration(
                color: DropTokens.accent,
                shape: BoxShape.circle,
              ),
              child: const Icon(Icons.check,
                  size: 38, color: DropTokens.onAccent),
            ),
          ),
          // Confetti.
          _confetti(48, 35, 4, 10, 25, DropTokens.accent),
          _confetti(85, 12, 4, 8, -25, DropTokens.sparkle),
          _confetti(112, 98, 3, 8, 35, DropTokens.accent),
          _confetti(263, 20, 4, 10, 30, DropTokens.accent),
          _confetti(298, 69, 4, 9, -25, DropTokens.sparkle),
          _confetti(251, 106, 4, 8, 65, DropTokens.accent),
          _confetti(63, 105, 3, 6, -15, DropTokens.sparklePale),
          _confetti(294, 10, 3, 5, 20, DropTokens.sparklePale),
          // Sparkles.
          const Positioned(
            left: 71,
            top: 67,
            child: Icon(Icons.auto_awesome,
                size: 18, color: DropTokens.accent),
          ),
          const Positioned(
            left: 239,
            top: 51,
            child: Icon(Icons.auto_awesome,
                size: 14, color: DropTokens.sparkleSoft),
          ),
          const Positioned(
            left: 312,
            top: 37,
            child: Icon(Icons.auto_awesome,
                size: 14, color: DropTokens.accent),
          ),
        ],
      ),
    );
  }

  Widget _ripple(double left, double top, double size, Color color) =>
      Positioned(
        left: left,
        top: top,
        child: Container(
          width: size,
          height: size,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            border: Border.all(color: color),
          ),
        ),
      );

  Widget _confetti(double left, double top, double w, double h, double angle,
          Color color) =>
      Positioned(
        left: left,
        top: top,
        child: Transform.rotate(
          angle: angle * 3.14159265 / 180,
          child: Container(
            width: w,
            height: h,
            decoration: BoxDecoration(
              color: color,
              borderRadius: BorderRadius.circular(1),
            ),
          ),
        ),
      );
}

class _Glow extends StatelessWidget {
  const _Glow();

  @override
  Widget build(BuildContext context) => Container(
        width: 228,
        height: 128,
        decoration: const BoxDecoration(
          gradient: DropTokens.celebrationGlow,
          shape: BoxShape.rectangle,
        ),
      );
}
