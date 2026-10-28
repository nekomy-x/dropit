import 'package:flutter/material.dart';

import 'tokens.dart';

/// Guest app theme using the shared graphite surfaces and blue action accent.
///
/// Здесь нет ни одного «магического» цвета: всё приходит из [DropTokens],
/// а шрифты — Inter / Geist Mono, подключённые в `pubspec.yaml`.
class DropTheme {
  DropTheme._();

  static ThemeData theme() {
    final base = ThemeData.dark(useMaterial3: true);
    const scheme = ColorScheme.dark(
      primary: DropTokens.accent,
      onPrimary: DropTokens.onAccent,
      secondary: DropTokens.accent,
      onSecondary: DropTokens.onAccent,
      surface: DropTokens.surface,
      onSurface: DropTokens.text,
      error: DropTokens.red,
      onError: DropTokens.text,
      outline: DropTokens.border,
    );

    return base.copyWith(
      colorScheme: scheme,
      scaffoldBackgroundColor: DropTokens.bg,
      canvasColor: DropTokens.bg,
      splashFactory: InkSparkle.splashFactory,
      appBarTheme: const AppBarTheme(
        backgroundColor: DropTokens.bg,
        surfaceTintColor: Colors.transparent,
        foregroundColor: DropTokens.text,
        elevation: 0,
        centerTitle: false,
        titleTextStyle: DropType.navTitle,
      ),
      textTheme: base.textTheme
          .apply(
            fontFamily: 'Inter',
            bodyColor: DropTokens.text,
            displayColor: DropTokens.text,
          )
          .copyWith(
            titleLarge: DropType.sectionTitle,
            titleMedium: DropType.title,
            bodyLarge: DropType.body,
            bodyMedium: DropType.bodySm,
            bodySmall: DropType.caption,
            labelLarge: DropType.bodySm,
          ),
      dividerTheme: const DividerThemeData(
        color: DropTokens.divider,
        thickness: 1,
        space: 1,
      ),
      // A field is a line under the value, not a box — the same treatment as
      // the web panels (`.field-input` in packages/ui/panel.css). No fill, no
      // radius, no side or top border; the 1px rule alone says "type here".
      inputDecorationTheme: InputDecorationTheme(
        filled: false,
        hintStyle: DropType.bodySm.copyWith(color: DropTokens.muted),
        contentPadding: const EdgeInsets.symmetric(horizontal: 0, vertical: 12),
        border: _fieldBorder(DropTokens.border),
        enabledBorder: _fieldBorder(DropTokens.border),
        focusedBorder: _fieldBorder(DropTokens.accent),
        errorBorder: _fieldBorder(DropTokens.red),
        focusedErrorBorder: _fieldBorder(DropTokens.red),
      ),
      bottomSheetTheme: const BottomSheetThemeData(
        backgroundColor: DropTokens.sheet,
        surfaceTintColor: Colors.transparent,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.vertical(
            top: Radius.circular(DropTokens.rSheet),
          ),
        ),
      ),
      snackBarTheme: const SnackBarThemeData(
        backgroundColor: DropTokens.surface2,
        contentTextStyle: TextStyle(
          fontFamily: 'Inter',
          fontSize: 13,
          color: DropTokens.text,
        ),
        behavior: SnackBarBehavior.floating,
      ),
      progressIndicatorTheme:
          const ProgressIndicatorThemeData(color: DropTokens.accent),
    );
  }

  /// The one field border of this product: a 1px rule under the value.
  /// `UnderlineInputBorder` is a straight rule at the bottom edge — the
  /// Material equivalent of the web panels' `border-bottom`.
  static UnderlineInputBorder _fieldBorder(Color color) => UnderlineInputBorder(
        borderRadius: BorderRadius.zero,
        borderSide: BorderSide(color: color),
      );
}
