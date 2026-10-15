import 'package:flutter/material.dart';

import 'tokens.dart';

/// Тема приложения гостя, собранная из токенов макета.
///
/// Здесь нет ни одного «магического» цвета: всё приходит из [DropTokens],
/// а шрифты — Anton / Inter / Geist Mono, подключённые в `pubspec.yaml`.
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
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: DropTokens.input,
        hintStyle: DropType.bodySm.copyWith(color: DropTokens.muted),
        contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
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

  static OutlineInputBorder _fieldBorder(Color color) => OutlineInputBorder(
        borderRadius: BorderRadius.circular(DropTokens.rMd),
        borderSide: BorderSide(color: color),
      );
}
