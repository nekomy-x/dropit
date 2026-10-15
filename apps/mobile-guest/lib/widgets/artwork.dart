import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// Обложка трека / фото заведения.
///
/// Если ссылки нет (или она не загрузилась) — фирменная заглушка
/// (градиент акцента + нота), чтобы список не «прыгал» и тесты не ходили
/// в сеть.
class Artwork extends StatelessWidget {
  const Artwork({
    super.key,
    this.url,
    this.size = 50,
    this.radius = DropTokens.rSm,
    this.glyph = Icons.music_note,
  });

  final String? url;
  final double size;
  final double radius;
  final IconData glyph;

  @override
  Widget build(BuildContext context) {
    final Widget image = (url != null && url!.isNotEmpty)
        ? Image.network(
            url!,
            width: size,
            height: size,
            fit: BoxFit.cover,
            errorBuilder: (_, __, ___) => _fallback(),
          )
        : _fallback();
    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: SizedBox(width: size, height: size, child: image),
    );
  }

  Widget _fallback() => Container(
        decoration: const BoxDecoration(
          gradient: LinearGradient(
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
            colors: [DropTokens.surface2, DropTokens.tint],
          ),
        ),
        alignment: Alignment.center,
        child: Icon(glyph, size: size * 0.44, color: DropTokens.accent),
      );
}
