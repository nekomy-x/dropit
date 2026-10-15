import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../theme/tokens.dart';
import '../widgets/app_button.dart';
import '../widgets/celebration.dart';
import '../widgets/nav.dart';
import '../widgets/track_row.dart';

/// Guest · 05 · Request sent — «Your pick is in.»
///
/// Макет: кадр «Guest · 05 · Request sent». Праздничный блок (свечение,
/// рябь, бейдж, конфетти), затем чек: трек, статус акцентом, оплата и
/// номер запроса. Внизу — «View my request» и «Find another track».
class RequestSentScreen extends StatelessWidget {
  const RequestSentScreen({
    super.key,
    required this.session,
    required this.track,
    required this.created,
    required this.onViewRequest,
    required this.onFindAnother,
  });

  final JoinResponse session;
  final CatalogTrack track;
  final RequestCreated created;
  final VoidCallback onViewRequest;
  final VoidCallback onFindAnother;

  @override
  Widget build(BuildContext context) {
    final dj = session.djDisplayName ?? 'the DJ';
    final sessionLine = session.djDisplayName == null
        ? 'Main floor · Live now'
        : 'Main floor · ${session.djDisplayName} · Live now';

    return Scaffold(
      backgroundColor: DropTokens.bg,
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(20, 24, 20, 20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const BrandBar(),
              const SizedBox(height: 14),
              JoinedRoomBar(
                roomName: session.clubName,
                sessionLine: sessionLine,
                padding: const EdgeInsets.fromLTRB(0, 16, 0, 16),
              ),
              const SizedBox(height: 20),
              Expanded(
                child: ListView(
                  padding: EdgeInsets.zero,
                  children: [
                    const Center(child: Celebration()),
                    const SizedBox(height: 20),
                    const Center(
                      child: Text('Your pick is in.',
                          textAlign: TextAlign.center,
                          style: DropType.screenTitle),
                    ),
                    const SizedBox(height: 12),
                    Center(
                      child: Text(
                        'Your request is with $dj.\n'
                        'Follow its status in My requests.',
                        textAlign: TextAlign.center,
                        style: DropType.bodySm.copyWith(
                          color: DropTokens.muted,
                          height: 1.5,
                        ),
                      ),
                    ),
                    const SizedBox(height: 24),
                    _Receipt(
                      title: track.title,
                      subtitle: track.artist,
                      price: session.priceLabel,
                      requestId: _shortId(created.requestId),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 16),
              AppButton(label: 'View my request', onPressed: onViewRequest),
              const SizedBox(height: 8),
              SizedBox(
                height: 44,
                width: double.infinity,
                child: TextButton(
                  onPressed: onFindAnother,
                  child: Text(
                    'Find another track',
                    style: DropType.bodySm.copyWith(
                      fontWeight: FontWeight.w600,
                      color: DropTokens.text,
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

  /// «#DI-0284» — короткий читаемый номер из UUID запроса.
  static String _shortId(String raw) {
    final clean = raw.replaceAll('-', '');
    final tail = clean.length >= 4 ? clean.substring(clean.length - 4) : clean;
    return '#DI-${tail.toUpperCase()}';
  }
}

class _Receipt extends StatelessWidget {
  const _Receipt({
    required this.title,
    required this.subtitle,
    required this.price,
    required this.requestId,
  });

  final String title;
  final String subtitle;
  final String price;
  final String requestId;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(vertical: 16),
      decoration: BoxDecoration(
        color: DropTokens.surface,
        borderRadius: BorderRadius.circular(DropTokens.rLg),
        border: Border.all(color: DropTokens.border),
      ),
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: DropType.trackTitle),
                      const SizedBox(height: 5),
                      Text(subtitle,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: DropType.metaSm),
                    ],
                  ),
                ),
                AppIconButton(
                  icon: Icons.add,
                  tooltip: 'Request again',
                  onPressed: () {},
                ),
              ],
            ),
          ),
          const SizedBox(height: 18),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: SummaryRow(
              label: 'Status',
              value: 'Sent to DJ',
              valueColor: DropTokens.accent,
              valueStyle: DropType.caption.copyWith(
                fontWeight: FontWeight.w600,
                color: DropTokens.accent,
              ),
              labelStyle: DropType.caption,
            ),
          ),
          const SizedBox(height: 18),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: SummaryRow(
              label: 'Payment',
              value: price,
              labelStyle: DropType.caption,
              valueStyle: DropType.bodySm.copyWith(
                fontSize: 13,
                color: DropTokens.muted,
              ),
            ),
          ),
          const SizedBox(height: 18),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: SummaryRow(
              label: 'Request ID',
              value: requestId,
              labelStyle: DropType.caption,
              valueStyle: DropType.data.copyWith(
                fontWeight: FontWeight.w500,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
