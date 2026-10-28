/// Guest · 15 · Identity — Verified-free identity.
///
/// Макет pen.dev: строка «назад + бренд», поля деталей гостя с
/// редактированием, чипы любимых жанров, кнопка сохранения и
/// приватное примечание.
library;

import 'package:flutter/material.dart';

import '../data/demo_data.dart';
import '../state/app_state.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class IdentityScreen extends StatefulWidget {
  const IdentityScreen({super.key, required this.onBack});

  final VoidCallback onBack;

  @override
  State<IdentityScreen> createState() => _IdentityScreenState();
}

class _IdentityScreenState extends State<IdentityScreen> {
  String _email = DemoData.memberEmail;
  String _code = DemoData.memberCode;
  String _city = DemoData.homeCity;
  String _genre = 'House';

  void _editField({
    required String title,
    required String value,
    required ValueChanged<String> onSaved,
  }) {
    final controller = TextEditingController(text: value);
    showDialog(
      context: context,
      builder: (_) => AlertDialog(
        backgroundColor: DropTokens.sheet,
        title: Text(title, style: DropType.title),
        content: DropField(
          controller: controller,
          hint: value,
          height: 48,
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child:
                const Text('Cancel', style: TextStyle(color: DropTokens.muted)),
          ),
          TextButton(
            onPressed: () {
              onSaved(controller.text.trim());
              Navigator.of(context).pop();
            },
            child:
                const Text('Save', style: TextStyle(color: DropTokens.accent)),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final state = AppState.of(context);
    return DropScaffold(
      tabBar: DropTabBar(
        current: 3,
        // Таб-бар на push-экране: переключаем таб и возвращаемся к корню.
        onChanged: (index) {
          state.setTab(index);
          Navigator.of(context).popUntil((route) => route.isFirst);
        },
      ),
      body: ListView(
        padding: EdgeInsets.zero,
        children: [
          const SizedBox(height: 24),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Row(
              children: [
                IconButton(
                  onPressed: widget.onBack,
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
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: Text('Personal details', style: DropType.screenTitle),
          ),
          const SizedBox(height: 24),

          // ── Детали ──
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: SectionLabel('YOUR DETAILS'),
          ),
          const SizedBox(height: 8),
          _DetailRow(
            icon: Icons.mail_outline,
            label: 'Email',
            value: _email,
            onEdit: () => _editField(
              title: 'Email',
              value: _email,
              onSaved: (v) => setState(() => _email = v),
            ),
          ),
          _DetailRow(
            icon: Icons.badge_outlined,
            label: 'Member code',
            value: _code,
            onEdit: () => _editField(
              title: 'Member code',
              value: _code,
              onSaved: (v) => setState(() => _code = v),
            ),
          ),
          _DetailRow(
            icon: Icons.home_outlined,
            label: 'Home city',
            value: _city,
            onEdit: () => _editField(
              title: 'Home city',
              value: _city,
              onSaved: (v) => setState(() => _city = v),
            ),
          ),
          const SizedBox(height: 24),

          // ── Любимые жанры ──
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: SectionLabel('FAVOURITE GENRES'),
          ),
          const SizedBox(height: 10),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                for (final genre in DemoData.favoriteGenres)
                  FilterChipPill(
                    label: genre,
                    active: _genre == genre,
                    onTap: () => setState(() => _genre = genre),
                  ),
              ],
            ),
          ),
          const SizedBox(height: 28),

          // ── Сохранить ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: PrimaryButton(
              label: 'Save changes',
              icon: Icons.check,
              trailingArrow: false,
              onPressed: () {
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('Details saved')),
                );
              },
            ),
          ),
          const SizedBox(height: 16),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Text(
              'No ID, no name check. Rooms you join only ever see your handle.',
              style: DropType.caption.copyWith(height: 1.5),
            ),
          ),
          const SizedBox(height: 100),
        ],
      ),
    );
  }
}

class _DetailRow extends StatelessWidget {
  const _DetailRow({
    required this.icon,
    required this.label,
    required this.value,
    required this.onEdit,
  });

  final IconData icon;
  final String label;
  final String value;
  final VoidCallback onEdit;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 52,
      margin: const EdgeInsets.symmetric(horizontal: 20, vertical: 4),
      padding: const EdgeInsets.symmetric(horizontal: 4),
      child: Row(
        children: [
          Icon(icon, size: 18, color: DropTokens.muted),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(label, style: DropType.caption),
                const SizedBox(height: 2),
                Text(value, style: DropType.bodySm.copyWith(fontSize: 13)),
              ],
            ),
          ),
          IconButton(
            onPressed: onEdit,
            icon: const Icon(Icons.edit_outlined,
                size: 14, color: DropTokens.tabInactive),
          ),
        ],
      ),
    );
  }
}
