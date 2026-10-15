import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../theme/tokens.dart';
import '../widgets/artwork.dart';
import '../widgets/app_button.dart';
import '../widgets/nav.dart';

/// Guest · 04a · Terms before payment — «Know the rules.»
///
/// Обязательный экран условий (ТЗ, §0.2): три правила, цена сессии и кнопка
/// подтверждения. По макету подтверждение — сама кнопка «I understand —
/// Continue» (чекбокса в макете нет), и в флоу экран стоит между выбором
/// трека и оплатой, то есть пропустить его нельзя.
class TermsScreen extends StatelessWidget {
  const TermsScreen({
    super.key,
    required this.session,
    required this.track,
    required this.onConfirmed,
    this.onReadFullTerms,
  });

  final JoinResponse session;
  final CatalogTrack track;
  final VoidCallback onConfirmed;
  final VoidCallback? onReadFullTerms;

  @override
  Widget build(BuildContext context) {
    final price = session.priceLabel;
    final rules = <String>[
      'You pay $price to send this request. The DJ decides if and when it gets played.',
      'If several people request the same song, each request is paid separately.',
      'A song can only be played once per night.',
    ];

    return Scaffold(
      backgroundColor: DropTokens.bg,
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(20, 24, 20, 20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const NavRow(title: 'Before you pay'),
              const SizedBox(height: 24),
              const Text('Know the rules.', style: DropType.screenTitle),
              const SizedBox(height: 24),
              Row(
                children: [
                  const Artwork(url: null),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(track.title,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: DropType.trackTitle),
                        const SizedBox(height: 5),
                        Text(
                          '${track.artist} · Original mix',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: DropType.meta,
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: 8),
                  Text(price,
                      style: DropType.caption.copyWith(
                        fontWeight: FontWeight.w700,
                        color: DropTokens.accent,
                      )),
                ],
              ),
              const SizedBox(height: 20),
              Container(
                decoration: const BoxDecoration(
                  border: Border(
                    top: BorderSide(color: DropTokens.divider),
                    bottom: BorderSide(color: DropTokens.divider),
                  ),
                ),
                padding: const EdgeInsets.symmetric(vertical: 20),
                child: Column(
                  children: [
                    for (var i = 0; i < rules.length; i++) ...[
                      if (i > 0) const SizedBox(height: 20),
                      Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          const Padding(
                            padding: EdgeInsets.only(top: 2),
                            child: Icon(Icons.check_circle_outline,
                                size: 18, color: DropTokens.accent),
                          ),
                          const SizedBox(width: 14),
                          Expanded(
                            child: Text(rules[i], style: DropType.body),
                          ),
                        ],
                      ),
                    ],
                  ],
                ),
              ),
              const Spacer(),
              AppButton(label: 'I understand — Continue', onPressed: onConfirmed),
              const SizedBox(height: 14),
              Center(
                child: GestureDetector(
                  onTap: onReadFullTerms,
                  behavior: HitTestBehavior.opaque,
                  child: Text(
                    'Read the full terms',
                    style: DropType.caption.copyWith(
                      fontWeight: FontWeight.w600,
                      color: DropTokens.accent,
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
