import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../main.dart';

/// Экран сессии после скана QR (dizayn-zadanie §0.1: цена из сессии,
/// НЕ захардкожена).
class SessionScreen extends StatelessWidget {
  const SessionScreen({
    super.key,
    required this.session,
    required this.onSearch,
    required this.onMyRequests,
  });

  final JoinResponse session;
  final VoidCallback onSearch;
  final VoidCallback onMyRequests;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(session.clubName),
        actions: [
          IconButton(
            icon: const Icon(Icons.receipt_long),
            onPressed: onMyRequests,
            tooltip: 'My requests',
          ),
        ],
      ),
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(20),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        session.djDisplayName ?? 'DJ to be announced',
                        style: Theme.of(context)
                            .textTheme
                            .titleLarge
                            ?.copyWith(fontWeight: FontWeight.w700),
                      ),
                      const SizedBox(height: 4),
                      Text(
                        'Accepting requests: ${session.status == 'active' ? 'open' : session.status.replaceAll('_', ' ')}',
                        style: TextStyle(color: DropItTheme.grey),
                      ),
                      const Divider(height: 32),
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          Text('Request price',
                              style: TextStyle(color: DropItTheme.grey)),
                          Text(
                            session.priceLabel,
                            style: Theme.of(context)
                                .textTheme
                                .headlineSmall
                                ?.copyWith(
                                  color: DropItTheme.neon,
                                  fontWeight: FontWeight.w800,
                                ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 24),
              Text(
                'You pay ${session.priceLabel} to send a request. '
                'The decision to play it stays with the DJ.',
                style: TextStyle(color: DropItTheme.grey),
              ),
              const Spacer(),
              ElevatedButton.icon(
                icon: const Icon(Icons.search),
                label: const Text('Find your song'),
                onPressed: onSearch,
              ),
            ],
          ),
        ),
      ),
    );
  }
}
