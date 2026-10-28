/// Guest · 08 · Sign in · 09 · Create account · 10 · Reset password.
///
/// Макет pen.dev: единый каркас форм авторизации — быстрый вход через
/// Google/Apple, разделитель «or with email», поля email/пароля,
/// основная кнопка и ссылки перехода.
library;

import 'package:flutter/material.dart';

import '../state/app_state.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

/// Общий каркас экрана авторизации: «назад + бренд», заголовок, описание.
class _AuthScaffold extends StatelessWidget {
  const _AuthScaffold({
    required this.title,
    required this.description,
    required this.onBack,
    required this.children,
  });

  final String title;
  final String description;
  final VoidCallback onBack;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return DropScaffold(
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const SizedBox(height: 24),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: Row(
                children: [
                  IconButton(
                    onPressed: onBack,
                    padding: EdgeInsets.zero,
                    constraints: const BoxConstraints(),
                    icon: const Icon(Icons.arrow_back,
                        size: 22, color: DropTokens.text),
                  ),
                  const SizedBox(width: 16),
                  const BrandLockup(),
                ],
              ),
            ),
            const SizedBox(height: 26),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                children: [
                  Text(title, style: DropType.screenTitle),
                  const SizedBox(height: 10),
                  Text(
                    description,
                    style: DropType.body.copyWith(color: DropTokens.muted),
                  ),
                  const SizedBox(height: 26),
                  ...children,
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Кнопка быстрого входа (Google / Apple): h52, фон «ночь», pill.
class _ProviderButton extends StatelessWidget {
  const _ProviderButton({
    required this.label,
    required this.glyph,
    required this.onTap,
  });

  final String label;
  final Widget glyph;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 52,
      width: double.infinity,
      child: Material(
        color: DropTokens.bg,
        borderRadius: BorderRadius.circular(999),
        child: InkWell(
          borderRadius: BorderRadius.circular(999),
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                glyph,
                const SizedBox(width: 12),
                Text(
                  label,
                  style: DropType.bodySm.copyWith(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: DropTokens.text,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Разделитель «or with email».
class _EmailSeparator extends StatelessWidget {
  const _EmailSeparator();

  @override
  Widget build(BuildContext context) {
    return const Row(
      children: [
        Expanded(child: Divider(color: DropTokens.divider26)),
        Padding(
          padding: EdgeInsets.symmetric(horizontal: 12),
          child: Text('or with email', style: DropType.caption),
        ),
        Expanded(child: Divider(color: DropTokens.divider26)),
      ],
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// Guest · 08 · Sign in.
class SignInScreen extends StatefulWidget {
  const SignInScreen({
    super.key,
    required this.onBack,
    required this.onDone,
    required this.onCreateAccount,
    required this.onForgotPassword,
  });

  final VoidCallback onBack;
  final VoidCallback onDone;
  final VoidCallback onCreateAccount;
  final VoidCallback onForgotPassword;

  @override
  State<SignInScreen> createState() => _SignInScreenState();
}

class _SignInScreenState extends State<SignInScreen> {
  final _email = TextEditingController();
  final _password = TextEditingController();
  final bool _obscure = true;
  String? _error;

  @override
  void dispose() {
    _email.dispose();
    _password.dispose();
    super.dispose();
  }

  void _submit() {
    final email = _email.text.trim();
    if (!email.contains('@') || email.isEmpty) {
      setState(() => _error = 'Enter a valid email');
      return;
    }
    if (_password.text.length < 8) {
      setState(() => _error = 'Password must be at least 8 characters');
      return;
    }
    setState(() => _error = null);
    AppState.of(context).signIn();
    widget.onDone();
  }

  @override
  Widget build(BuildContext context) {
    return _AuthScaffold(
      title: 'Sign in',
      description: 'Welcome back. Pick up where you left off.',
      onBack: widget.onBack,
      children: [
        _ProviderButton(
          label: 'Google',
          glyph: const CustomPaint(size: Size(20, 20), painter: GoogleGlyph()),
          onTap: () {
            AppState.of(context).signIn();
            widget.onDone();
          },
        ),
        const SizedBox(height: 10),
        _ProviderButton(
          label: 'Apple',
          glyph: const CustomPaint(size: Size(18, 20), painter: AppleGlyph()),
          onTap: () {
            AppState.of(context).signIn();
            widget.onDone();
          },
        ),
        const SizedBox(height: 24),
        const _EmailSeparator(),
        const SizedBox(height: 24),
        const FieldLabel('Email'),
        DropField(
          controller: _email,
          hint: 'you@example.com',
          prefixIcon: Icons.mail_outline,
        ),
        const SizedBox(height: 16),
        const FieldLabel('Password'),
        DropField(
          controller: _password,
          hint: 'Enter your password',
          prefixIcon: Icons.lock_outline,
          obscure: _obscure,
          suffixIcon: _obscure ? Icons.visibility_off : Icons.visibility,
        ),
        Align(
          alignment: Alignment.centerRight,
          child: TextButton(
            onPressed: widget.onForgotPassword,
            child: Text(
              'Forgot password?',
              style: DropType.bodySm.copyWith(
                fontSize: 13,
                color: DropTokens.accent,
              ),
            ),
          ),
        ),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: Text(
              _error!,
              style: DropType.caption.copyWith(color: DropTokens.red),
            ),
          ),
        PrimaryButton(label: 'Sign in', onPressed: _submit),
        const SizedBox(height: 16),
        Center(
          child: InkWell(
            onTap: widget.onCreateAccount,
            child: Text(
              'New here?  Create an account',
              style: DropType.bodySm.copyWith(
                fontSize: 13,
                color: DropTokens.accent,
              ),
            ),
          ),
        ),
        const SizedBox(height: 24),
        const Center(
          child: Text(
            'Your details stay private. Your music gets heard.',
            style: DropType.caption,
            textAlign: TextAlign.center,
          ),
        ),
        const SizedBox(height: 40),
      ],
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// Guest · 09 · Create account.
class CreateAccountScreen extends StatefulWidget {
  const CreateAccountScreen({
    super.key,
    required this.onBack,
    required this.onDone,
    required this.onSignIn,
  });

  final VoidCallback onBack;
  final VoidCallback onDone;
  final VoidCallback onSignIn;

  @override
  State<CreateAccountScreen> createState() => _CreateAccountScreenState();
}

class _CreateAccountScreenState extends State<CreateAccountScreen> {
  final _email = TextEditingController();
  final _password = TextEditingController();
  final bool _obscure = true;
  String? _error;

  @override
  void dispose() {
    _email.dispose();
    _password.dispose();
    super.dispose();
  }

  void _submit() {
    final email = _email.text.trim();
    if (!email.contains('@') || email.isEmpty) {
      setState(() => _error = 'Enter a valid email');
      return;
    }
    if (_password.text.length < 8) {
      setState(() => _error = 'Use at least 8 characters');
      return;
    }
    setState(() => _error = null);
    AppState.of(context).signIn();
    widget.onDone();
  }

  @override
  Widget build(BuildContext context) {
    return _AuthScaffold(
      title: 'Create account',
      description: 'Save your details. Track your requests.',
      onBack: widget.onBack,
      children: [
        _ProviderButton(
          label: 'Google',
          glyph: const CustomPaint(size: Size(20, 20), painter: GoogleGlyph()),
          onTap: () {
            AppState.of(context).signIn();
            widget.onDone();
          },
        ),
        const SizedBox(height: 10),
        _ProviderButton(
          label: 'Apple',
          glyph: const CustomPaint(size: Size(18, 20), painter: AppleGlyph()),
          onTap: () {
            AppState.of(context).signIn();
            widget.onDone();
          },
        ),
        const SizedBox(height: 24),
        const _EmailSeparator(),
        const SizedBox(height: 24),
        const FieldLabel('Email'),
        DropField(
          controller: _email,
          hint: 'you@example.com',
          prefixIcon: Icons.mail_outline,
        ),
        const SizedBox(height: 16),
        const FieldLabel('Password'),
        DropField(
          controller: _password,
          hint: 'Create a password',
          prefixIcon: Icons.lock_outline,
          obscure: _obscure,
          suffixIcon: _obscure ? Icons.visibility_off : Icons.visibility,
        ),
        const Padding(
          padding: EdgeInsets.only(top: 8, left: 2),
          child: Text('Use at least 8 characters.', style: DropType.caption),
        ),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.only(top: 8, left: 2),
            child: Text(
              _error!,
              style: DropType.caption.copyWith(color: DropTokens.red),
            ),
          ),
        const SizedBox(height: 16),
        PrimaryButton(label: 'Create account', onPressed: _submit),
        const SizedBox(height: 16),
        Center(
          child: InkWell(
            onTap: widget.onSignIn,
            child: Text(
              'Already a member?  Sign in',
              style: DropType.bodySm.copyWith(
                fontSize: 13,
                color: DropTokens.accent,
              ),
            ),
          ),
        ),
        const SizedBox(height: 24),
        Center(
          child: Text(
            'By continuing, you agree to our Terms of Service\nand Privacy Policy.',
            style: DropType.caption.copyWith(height: 1.5),
            textAlign: TextAlign.center,
          ),
        ),
        const SizedBox(height: 40),
      ],
    );
  }
}

// ────────────────────────────────────────────────────────────────────────
/// Guest · 10 · Reset password.
class ResetPasswordScreen extends StatefulWidget {
  const ResetPasswordScreen({
    super.key,
    required this.onBack,
    required this.onBackToSignIn,
  });

  final VoidCallback onBack;
  final VoidCallback onBackToSignIn;

  @override
  State<ResetPasswordScreen> createState() => _ResetPasswordScreenState();
}

class _ResetPasswordScreenState extends State<ResetPasswordScreen> {
  final _email = TextEditingController();

  @override
  void dispose() {
    _email.dispose();
    super.dispose();
  }

  void _submit() {
    final email = _email.text.trim();
    if (!email.contains('@') || email.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Enter a valid email')),
      );
      return;
    }
    showDialog(
      context: context,
      builder: (_) => AlertDialog(
        backgroundColor: DropTokens.sheet,
        title: const Text('Reset link sent', style: DropType.title),
        content: Text(
          'We sent a reset link to $email. Remember to check your spam folder.',
          style: DropType.bodySm,
        ),
        actions: [
          TextButton(
            onPressed: () {
              Navigator.of(context).pop();
              widget.onBackToSignIn();
            },
            child: const Text('OK', style: TextStyle(color: DropTokens.accent)),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return _AuthScaffold(
      title: 'Forgot your password?',
      description:
          'Enter your email and we’ll send you a link to reset your password.',
      onBack: widget.onBack,
      children: [
        const FieldLabel('Email'),
        DropField(
          controller: _email,
          hint: 'you@example.com',
          keyboardType: TextInputType.emailAddress,
        ),
        const SizedBox(height: 20),
        PrimaryButton(label: 'Send reset link', onPressed: _submit),
        const SizedBox(height: 16),
        Center(
          child: InkWell(
            onTap: widget.onBackToSignIn,
            child: Text(
              'Back to sign in',
              style: DropType.caption.copyWith(
                fontWeight: FontWeight.w600,
                color: DropTokens.accent,
              ),
            ),
          ),
        ),
        const SizedBox(height: 24),
        const Center(
          child: Text(
            'Remember to check your spam folder.',
            style: DropType.caption,
          ),
        ),
        const SizedBox(height: 40),
      ],
    );
  }
}
