import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../main.dart';

/// Логин DJ (§1.1): email + пароль, вход с 2FA (код из
/// приложения-аутентификатора), «Забыли пароль?».
class LoginScreen extends StatefulWidget {
  const LoginScreen({super.key, required this.api, required this.onLoggedIn});

  final ApiClient api;
  final VoidCallback onLoggedIn;

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  final _email = TextEditingController();
  final _password = TextEditingController();
  final _totp = TextEditingController();
  bool _busy = false;
  bool _need2fa = false;
  String? _error;

  Future<void> _submit() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      if (_need2fa) {
        await widget.api.login2fa(_email.text.trim(), _totp.text.trim());
        widget.onLoggedIn();
      } else {
        final done = await widget.api.login(
          _email.text.trim(),
          _password.text,
        );
        if (done) {
          widget.onLoggedIn();
        } else {
          // Шаг 2: сервер требует TOTP-код.
          setState(() => _need2fa = true);
        }
      }
    } on ApiException catch (e) {
      setState(() {
        _error = switch (e.code) {
          'invalid_credentials' => 'Wrong email or password',
          'invalid_totp' => 'Wrong code — check your authenticator',
          'totp_required' => 'Enter the code from your authenticator app',
          _ => e.message,
        };
      });
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  void dispose() {
    _email.dispose();
    _password.dispose();
    _totp.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: Center(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(24),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 420),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  const Icon(Icons.headphones, size: 64, color: DjTheme.neon),
                  const SizedBox(height: 12),
                  Text(
                    'DROP IT DJ',
                    textAlign: TextAlign.center,
                    style: Theme.of(context)
                        .textTheme
                        .headlineSmall
                        ?.copyWith(fontWeight: FontWeight.w800, letterSpacing: 3),
                  ),
                  const SizedBox(height: 36),
                  TextField(
                    controller: _email,
                    enabled: !_need2fa,
                    keyboardType: TextInputType.emailAddress,
                    decoration: const InputDecoration(
                      hintText: 'Email',
                      prefixIcon: Icon(Icons.mail_outline),
                    ),
                  ),
                  const SizedBox(height: 12),
                  if (!_need2fa) ...[
                    TextField(
                      controller: _password,
                      obscureText: true,
                      decoration: const InputDecoration(
                        hintText: 'Password',
                        prefixIcon: Icon(Icons.lock_outline),
                      ),
                      onSubmitted: (_) => _submit(),
                    ),
                    Align(
                      alignment: Alignment.centerRight,
                      child: TextButton(
                        onPressed: () {},
                        child: Text('Forgot password?',
                            style: TextStyle(color: DjTheme.grey)),
                      ),
                    ),
                  ] else ...[
                    // Шаг 2FA: крупное поле кода (читается в темноте).
                    TextField(
                      controller: _totp,
                      keyboardType: TextInputType.number,
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                          fontSize: 32, letterSpacing: 12, fontWeight: FontWeight.w800),
                      maxLength: 6,
                      decoration: const InputDecoration(
                        counterText: '',
                        hintText: '••••••',
                        hintStyle: TextStyle(fontSize: 32, letterSpacing: 12),
                      ),
                      autofocus: true,
                      onSubmitted: (_) => _submit(),
                    ),
                    Text(
                      'Enter the 6-digit code from\nyour authenticator app',
                      textAlign: TextAlign.center,
                      style: TextStyle(color: DjTheme.grey),
                    ),
                  ],
                  const SizedBox(height: 20),
                  if (_error != null)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 12),
                      child: Text(_error!, style: TextStyle(color: DjTheme.red)),
                    ),
                  ElevatedButton(
                    onPressed: _busy ? null : _submit,
                    child: _busy
                        ? const SizedBox(
                            height: 22,
                            width: 22,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : Text(_need2fa ? 'Verify' : 'Sign in'),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
