import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import '../theme/tokens.dart';

/// Фирменный знак DROP IT — «пиксельная волна».
///
/// Соответствует брендбуку, стр. 10: ширина надписи делится на 22 полосы,
/// у каждой своя высота зелёной заливки; контур букв неподвижен, эффект
/// живёт ВНУТРИ надписи. Параметры волны (уровень 0,49, амплитуды 0,20/0,12,
/// множители времени 3,4/−2,1, смягчение ±0,025, градиент #8AB4F8 → #AECBFA)
/// зашиты в `shaders/brand_wave_mint.frag`.
///
/// Знак адаптивен: при включённом «уменьшить движение» и при недоступном
/// шейдере (например в widget-тестах) отдаёт статичный кадр — одноцветный
/// градиент вместо анимации. Размеры при этом не меняются никогда.
class BrandWaveLogo extends StatefulWidget {
  const BrandWaveLogo({
    super.key,
    this.fontSize = 18,
    this.showMark = true,
    this.markSize = 15,
    this.gap = 7,
  });

  /// Кегль надписи: 18 в шапках экранов (как в макете).
  final double fontSize;
  final bool showMark;
  final double markSize;
  final double gap;

  @override
  State<BrandWaveLogo> createState() => _BrandWaveLogoState();
}

class _BrandWaveLogoState extends State<BrandWaveLogo>
    with SingleTickerProviderStateMixin {
  static const _shaderAsset = 'shaders/brand_wave_mint.frag';

  late final AnimationController _clock;
  ui.FragmentShader? _shader;
  bool _shaderFailed = false;

  @override
  void initState() {
    super.initState();
    _clock = AnimationController(
      vsync: this,
      duration: const Duration(seconds: 1),
    );
    _loadShader();
  }

  Future<void> _loadShader() async {
    try {
      final program = await ui.FragmentProgram.fromAsset(_shaderAsset);
      if (!mounted) return;
      setState(() => _shader = program.fragmentShader());
      _clock.repeat();
    } catch (_) {
      // Шейдеры недоступны (тестовый раннер) — статичный кадр.
      if (mounted) setState(() => _shaderFailed = true);
    }
  }

  @override
  void dispose() {
    _clock.dispose();
    _shader?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final reduceMotion = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    final wordmark = Text(
      'DROP IT',
      style: TextStyle(
        fontFamily: 'Inter',
        fontSize: widget.fontSize,
        fontWeight: FontWeight.w700,
        letterSpacing: 0.2,
        color: Colors.white,
        height: 1.0,
      ),
    );

    return Semantics(
      label: 'DROP IT',
      container: true,
      excludeSemantics: true,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          _BrandWordmark(
            shader: _shaderFailed ? null : _shader,
            clock: _clock,
            reduceMotion: reduceMotion,
            child: wordmark,
          ),
          if (widget.showMark) ...[
            SizedBox(width: widget.gap),
            Icon(
              Icons.headphones,
              size: widget.markSize,
              color: DropTokens.text,
            ),
          ],
        ],
      ),
    );
  }
}

class _BrandWordmark extends StatelessWidget {
  const _BrandWordmark({
    required this.child,
    required this.clock,
    required this.reduceMotion,
    this.shader,
  });

  final Widget child;
  final AnimationController clock;
  final bool reduceMotion;
  final ui.FragmentShader? shader;

  @override
  Widget build(BuildContext context) {
    final active = shader != null && !reduceMotion;
    return AnimatedBuilder(
      animation: clock,
      builder: (context, _) {
        return ShaderMask(
          blendMode: BlendMode.srcIn,
          shaderCallback: (bounds) {
            if (active) {
              final s = shader!;
              s.setFloat(0, bounds.width);
              s.setFloat(1, bounds.height);
              s.setFloat(2, clock.value * 6.2831853);
              return s;
            }
            // Статичный кадр: тот же фирменный градиент, без движения.
            return DropTokens.brandGradient.createShader(
              Rect.fromLTWH(0, 0, bounds.width, bounds.height),
            );
          },
          child: child,
        );
      },
    );
  }
}
