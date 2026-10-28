/// Guest · 14 · Help & support.
///
/// Макет pen.dev: поиск по помощи, список FAQ с раскрытием, карточка
/// живой поддержки с чатом и почтой, версия приложения.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../data/demo_data.dart';
import '../state/app_state.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class HelpScreen extends StatefulWidget {
  const HelpScreen({super.key, required this.onBack});

  final VoidCallback onBack;

  @override
  State<HelpScreen> createState() => _HelpScreenState();
}

class _HelpScreenState extends State<HelpScreen> {
  String _query = '';
  int? _openIndex;

  List<(String, String)> get _visible {
    final q = _query.trim().toLowerCase();
    return DemoData.faq
        .where((f) => q.isEmpty || f.question.toLowerCase().contains(q))
        .map((f) => (f.question, f.answer))
        .toList();
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
                const SizedBox(width: 12),
                const Text('Help & support', style: DropType.trackTitle),
                const Spacer(),
                const CityPill(),
              ],
            ),
          ),
          const SizedBox(height: 20),
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: Text('How can we help?', style: DropType.screenTitle),
          ),
          const SizedBox(height: 20),

          // ── Поиск по помощи ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Container(
              height: 48,
              decoration: const BoxDecoration(
                border: Border(
                  bottom: BorderSide(color: DropTokens.border),
                ),
              ),
              child: Row(
                children: [
                  const Icon(Icons.search, size: 16, color: DropTokens.muted),
                  const SizedBox(width: 10),
                  Expanded(
                    child: TextField(
                      onChanged: (v) => setState(() => _query = v),
                      style: DropType.bodySm.copyWith(fontSize: 13),
                      cursorColor: DropTokens.accent,
                      decoration: InputDecoration(
                        border: InputBorder.none,
                        enabledBorder: InputBorder.none,
                        focusedBorder: InputBorder.none,
                        isCollapsed: true,
                        filled: false,
                        hintText: 'Search help topics',
                        hintStyle: DropType.bodySm
                            .copyWith(fontSize: 13, color: DropTokens.muted),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 20),

          // ── FAQ ──
          for (final (i, entry) in _visible.indexed) ...[
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: _FaqItem(
                question: entry.$1,
                answer: entry.$2,
                open: _openIndex == i,
                onTap: () =>
                    setState(() => _openIndex = _openIndex == i ? null : i),
              ),
            ),
            const Divider(color: DropTokens.border, indent: 20, endIndent: 20),
          ],
          const SizedBox(height: 20),

          // ── Карточка поддержки ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Container(
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: DropTokens.surface,
                borderRadius: BorderRadius.circular(DropTokens.rLg),
                border: Border.all(color: DropTokens.border),
              ),
              child: Column(
                children: [
                  Row(
                    children: [
                      Container(
                        width: 34,
                        height: 34,
                        decoration: BoxDecoration(
                          color: DropTokens.tint14,
                          borderRadius: BorderRadius.circular(DropTokens.rXs),
                        ),
                        child: const Icon(Icons.support_agent,
                            size: 20, color: DropTokens.accent),
                      ),
                      const SizedBox(width: 12),
                      const Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text('Need a human?', style: DropType.trackTitle),
                            SizedBox(height: 2),
                            Text('Real people, replies in ~5 min',
                                style: DropType.caption),
                          ],
                        ),
                      ),
                      const LiveBadge(label: 'LIVE'),
                    ],
                  ),
                  const SizedBox(height: 14),
                  PrimaryButton(
                    label: 'Chat with us',
                    icon: Icons.chat_bubble_outline,
                    trailingArrow: false,
                    onPressed: () => _openChat(context),
                  ),
                  const SizedBox(height: 12),
                  const Divider(color: DropTokens.border),
                  const SizedBox(height: 4),
                  Row(
                    children: [
                      const Icon(Icons.mail_outline,
                          size: 15, color: DropTokens.muted),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Text('support@dropit.club',
                            style: DropType.bodySm.copyWith(fontSize: 13)),
                      ),
                      GestureDetector(
                        onTap: () {
                          Clipboard.setData(
                            const ClipboardData(text: 'support@dropit.club'),
                          );
                          ScaffoldMessenger.of(context).showSnackBar(
                            const SnackBar(content: Text('Email copied')),
                          );
                        },
                        child: Container(
                          width: 28,
                          height: 28,
                          decoration: BoxDecoration(
                            color: const Color(0x0AFFFFFF),
                            borderRadius: BorderRadius.circular(999),
                          ),
                          child: const Icon(Icons.content_copy,
                              size: 13, color: DropTokens.muted),
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),
          const Center(
            child: Text('DROP IT v1.4.2', style: DropType.caption),
          ),
          const SizedBox(height: 100),
        ],
      ),
    );
  }

  void _openChat(BuildContext context) {
    showModalBottomSheet(
      context: context,
      backgroundColor: DropTokens.sheet,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(28)),
      ),
      builder: (_) => const _ChatSheet(),
    );
  }
}

class _FaqItem extends StatelessWidget {
  const _FaqItem({
    required this.question,
    required this.answer,
    required this.open,
    required this.onTap,
  });

  final String question;
  final String answer;
  final bool open;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    question,
                    style: DropType.body.copyWith(
                      fontSize: 14,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                Icon(
                  open ? Icons.keyboard_arrow_up : Icons.keyboard_arrow_down,
                  size: 16,
                  color: open ? DropTokens.accent : DropTokens.muted,
                ),
              ],
            ),
            if (open && answer.isNotEmpty) ...[
              const SizedBox(height: 10),
              Text(
                answer,
                style: DropType.caption.copyWith(height: 1.5),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// Простой чат поддержки (демо-ответ).
class _ChatSheet extends StatefulWidget {
  const _ChatSheet();

  @override
  State<_ChatSheet> createState() => _ChatSheetState();
}

class _ChatSheetState extends State<_ChatSheet> {
  final _controller = TextEditingController();
  final _messages = <(String, bool)>[
    ('Hi! How can we help tonight?', false),
  ];

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _send() {
    final text = _controller.text.trim();
    if (text.isEmpty) return;
    setState(() {
      _messages.add((text, true));
      _messages.add(('Got it — support will reply in ~5 min.', false));
    });
    _controller.clear();
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(
        left: 20,
        right: 20,
        bottom: MediaQuery.of(context).viewInsets.bottom + 20,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const SizedBox(height: 12),
          Center(
            child: Container(
              width: 36,
              height: 4,
              decoration: BoxDecoration(
                color: DropTokens.handle,
                borderRadius: BorderRadius.circular(2),
              ),
            ),
          ),
          const SizedBox(height: 16),
          const Text('Support chat', style: DropType.sheetTitle),
          const SizedBox(height: 12),
          ConstrainedBox(
            constraints: BoxConstraints(
              maxHeight: MediaQuery.of(context).size.height * 0.35,
            ),
            child: ListView(
              shrinkWrap: true,
              children: [
                for (final (text, mine) in _messages)
                  Align(
                    alignment:
                        mine ? Alignment.centerRight : Alignment.centerLeft,
                    child: Container(
                      margin: const EdgeInsets.symmetric(vertical: 4),
                      padding: const EdgeInsets.symmetric(
                          horizontal: 12, vertical: 8),
                      decoration: BoxDecoration(
                        color: mine ? DropTokens.accent : DropTokens.sheetRow,
                        borderRadius: BorderRadius.circular(12),
                      ),
                      child: Text(
                        text,
                        style: DropType.bodySm.copyWith(
                          color: mine ? DropTokens.onAccent : DropTokens.text,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(height: 12),
          Row(
            children: [
              Expanded(
                child: DropField(
                  controller: _controller,
                  hint: 'Type a message…',
                  height: 48,
                ),
              ),
              const SizedBox(width: 10),
              SizedBox(
                width: 48,
                height: 48,
                child: Material(
                  color: DropTokens.accent,
                  borderRadius: BorderRadius.circular(999),
                  child: InkWell(
                    borderRadius: BorderRadius.circular(999),
                    onTap: _send,
                    child: const Icon(Icons.send,
                        size: 18, color: DropTokens.onAccent),
                  ),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
