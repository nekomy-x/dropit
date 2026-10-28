/// Guest · 07 · Profile + 07b · Profile · Guest.
///
/// Макет pen.dev: экран профиля — статистика, разделы ACCOUNT и HERE TO
/// HELP, выход; и экран неавторизованного гостя с кнопками входа и
/// регистрации и списком преимуществ.
library;

import 'package:flutter/material.dart';

import '../data/demo_data.dart';
import '../state/app_state.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class ProfileScreen extends StatefulWidget {
  const ProfileScreen({
    super.key,
    required this.onOpenIdentity,
    required this.onOpenHelp,
    required this.onOpenPrivacy,
    required this.onOpenPaymentMethods,
  });

  final VoidCallback onOpenIdentity;
  final VoidCallback onOpenHelp;
  final VoidCallback onOpenPrivacy;
  final VoidCallback onOpenPaymentMethods;

  @override
  State<ProfileScreen> createState() => _ProfileScreenState();
}

class _ProfileScreenState extends State<ProfileScreen> {
  bool _notificationsEnabled = true;
  String _language = 'English';

  void _chooseLanguage() {
    showModalBottomSheet<void>(
      context: context,
      backgroundColor: DropTokens.sheet,
      builder: (sheetContext) => SafeArea(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          for (final language in ['English', 'Deutsch', 'Español'])
            ListTile(
              title: Text(language),
              trailing: language == _language
                  ? const Icon(Icons.check, color: DropTokens.accent)
                  : null,
              onTap: () {
                setState(() => _language = language);
                Navigator.of(sheetContext).pop();
              },
            ),
        ]),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return DropScaffold(
      body: ListView(
        padding: EdgeInsets.zero,
        children: [
          const SizedBox(height: 24),
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: BrandCityRow(),
          ),
          const SizedBox(height: 18),
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: Text('Your profile', style: DropType.screenTitle),
          ),
          const SizedBox(height: 20),

          // ── Статистика ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Container(
              height: 132,
              decoration: BoxDecoration(
                color: DropTokens.surface,
                borderRadius: BorderRadius.circular(DropTokens.rLg),
                border: Border.all(color: DropTokens.border),
              ),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                children: [
                  const _Stat(
                    value: '${DemoData.statPlayed}',
                    label: 'Played',
                  ),
                  _StatDivider(),
                  const _Stat(
                    value: '${DemoData.statRooms}',
                    label: 'Rooms visited',
                  ),
                  _StatDivider(),
                  const _Stat(
                    value: '${DemoData.statRequests}',
                    label: 'Requests',
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 24),

          // ── ACCOUNT ──
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: SectionLabel('ACCOUNT'),
          ),
          const SizedBox(height: 4),
          SettingsRow(
            label: 'Personal details',
            icon: Icons.person_outline,
            height: 46,
            onTap: widget.onOpenIdentity,
          ),
          SettingsRow(
            label: 'Payment methods',
            icon: Icons.credit_card_outlined,
            value: 'Stripe checkout',
            height: 46,
            onTap: widget.onOpenPaymentMethods,
          ),
          SettingsRow(
            label: 'Notifications',
            icon: Icons.notifications_none,
            value: _notificationsEnabled ? 'On' : 'Off',
            height: 46,
            onTap: () =>
                setState(() => _notificationsEnabled = !_notificationsEnabled),
          ),
          SettingsRow(
            label: 'Language',
            icon: Icons.language,
            value: _language,
            height: 46,
            onTap: _chooseLanguage,
          ),
          const SizedBox(height: 24),

          // ── HERE TO HELP ──
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: SectionLabel('HERE TO HELP'),
          ),
          const SizedBox(height: 4),
          SettingsRow(
            label: 'Help & support',
            icon: Icons.help_outline,
            height: 40,
            onTap: widget.onOpenHelp,
          ),
          SettingsRow(
            label: 'Privacy & terms',
            icon: Icons.lock_outline,
            height: 40,
            onTap: widget.onOpenPrivacy,
          ),
          const SizedBox(height: 24),

          // ── Выход ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: SettingsRow(
              label: 'Sign out',
              icon: Icons.logout,
              height: 44,
              showChevron: false,
              labelColor: DropTokens.muted,
              onTap: () {
                AppState.of(context).signOut();
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('Signed out')),
                );
              },
            ),
          ),
          const SizedBox(height: 100),
        ],
      ),
    );
  }
}

class _Stat extends StatelessWidget {
  const _Stat({required this.value, required this.label});

  final String value;
  final String label;

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        Text(value, style: DropType.statValue),
        const SizedBox(height: 4),
        Text(
          label,
          style: DropType.caption.copyWith(color: DropTokens.statLabel),
        ),
      ],
    );
  }
}

class _StatDivider extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Container(width: 1, height: 48, color: DropTokens.border);
  }
}

/// Guest · 07b · Profile · Guest — неавторизованный гость.
class ProfileGuestScreen extends StatelessWidget {
  const ProfileGuestScreen({
    super.key,
    required this.onSignIn,
    required this.onCreateAccount,
  });

  final VoidCallback onSignIn;
  final VoidCallback onCreateAccount;

  static const _benefits = [
    'All your requests, in one place',
    'Faster checkout next time',
    'Your rooms. Your music.',
  ];

  @override
  Widget build(BuildContext context) {
    return DropScaffold(
      body: ListView(
        padding: EdgeInsets.zero,
        children: [
          const SizedBox(height: 24),
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: BrandCityRow(),
          ),
          const SizedBox(height: 18),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('Your night.\nYour profile.',
                    style: DropType.screenTitle),
                const SizedBox(height: 10),
                Text(
                  'Keep your requests, save your details and make every night yours.',
                  style: DropType.caption.copyWith(
                    height: 1.5,
                    color: DropTokens.caption,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 24),

          // ── Действия аккаунта ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Column(
              children: [
                PrimaryButton(
                  label: 'Sign in',
                  onPressed: onSignIn,
                ),
                const SizedBox(height: 10),
                _OutlineAccountButton(
                  label: 'Create an account',
                  onTap: onCreateAccount,
                ),
                const SizedBox(height: 14),
                const Text(
                  'Quick sign-in with Google or Apple available',
                  style: DropType.caption,
                ),
              ],
            ),
          ),
          const SizedBox(height: 28),

          // ── Преимущества ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Column(
              children: [
                for (final benefit in _benefits) ...[
                  Container(
                    height: 48,
                    padding: const EdgeInsets.symmetric(horizontal: 4),
                    child: Row(
                      children: [
                        const Icon(Icons.check_circle_outline,
                            size: 18, color: DropTokens.accent),
                        const SizedBox(width: 12),
                        Text(
                          benefit,
                          style: DropType.caption
                              .copyWith(fontSize: 13, color: DropTokens.muted),
                        ),
                      ],
                    ),
                  ),
                ],
              ],
            ),
          ),
          const SizedBox(height: 100),
        ],
      ),
    );
  }
}

class _OutlineAccountButton extends StatelessWidget {
  const _OutlineAccountButton({required this.label, required this.onTap});

  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 52,
      width: double.infinity,
      child: Material(
        color: DropTokens.bg,
        borderRadius: BorderRadius.circular(DropTokens.rLg),
        child: InkWell(
          borderRadius: BorderRadius.circular(DropTokens.rLg),
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(label,
                    style: DropType.button.copyWith(color: DropTokens.text)),
                const SizedBox(width: 12),
                const Icon(Icons.arrow_forward,
                    size: 20, color: DropTokens.accent),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
