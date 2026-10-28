/// Guest · 01 · Home — экран «Listen».
///
/// Макет pen.dev, фрейм `Guest · 01 · Home`:
/// шапка «бренд + город», hero-фотография с контрастным слоем и живым
/// заголовком, парный блок действий (карта + QR), строка ближайшего клуба,
/// рекомендации треков и таб-бар.
library;

import 'package:flutter/material.dart';

import '../data/demo_data.dart';
import '../data/models.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class HomeScreen extends StatelessWidget {
  const HomeScreen({
    super.key,
    required this.onScan,
    required this.onExplore,
    required this.onOpenRoom,
    required this.onRequestTrack,
  });

  final VoidCallback onScan;
  final VoidCallback onExplore;
  final ValueChanged<Room> onOpenRoom;
  final ValueChanged<Track> onRequestTrack;

  @override
  Widget build(BuildContext context) {
    return DropScaffold(
      body: ListView(
        padding: EdgeInsets.zero,
        physics: const BouncingScrollPhysics(),
        children: [
          const SizedBox(height: 24),
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 20),
            child: BrandCityRow(),
          ),
          const SizedBox(height: 18),

          // ── Hero: фотография + контрастный слой + живой заголовок ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(DropTokens.rSm),
              child: SizedBox(
                height: 184,
                child: Stack(
                  fit: StackFit.expand,
                  children: [
                    Image.asset('assets/images/hero_night.jpg',
                        fit: BoxFit.cover),
                    const DecoratedBox(
                      decoration: BoxDecoration(
                        gradient: LinearGradient(
                          begin: Alignment.bottomCenter,
                          end: Alignment.topCenter,
                          colors: [Color(0xD9090909), Color(0x05090909)],
                        ),
                      ),
                    ),
                    Padding(
                      padding: const EdgeInsets.only(left: 16, top: 45),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          const Text('Your track.\nTheir next move.',
                              style: DropType.screenTitle),
                          const SizedBox(height: 8),
                          Text(
                            'Join a club. Request a track. Own the night.',
                            style: DropType.caption.copyWith(
                                fontSize: 12, color: DropTokens.caption),
                          ),
                        ],
                      ),
                    ),
                    // Живая подпись поверх hero (слой 04 брендбука 11).
                    Positioned(
                      left: 16,
                      bottom: 14,
                      child: Container(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 8, vertical: 4),
                        decoration: BoxDecoration(
                          color: const Color(0x33000000),
                          borderRadius: BorderRadius.circular(999),
                        ),
                        child: const Text(
                          '12 ROOMS LIVE TONIGHT',
                          style: TextStyle(
                            fontFamily: 'Inter',
                            fontSize: 12,
                            fontWeight: FontWeight.w700,
                            color: Colors.white,
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
          const SizedBox(height: 16),

          // ── Парный блок действий: карта + QR ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: Row(
              children: [
                // Nearby map action [12,278 183x136]
                Expanded(
                  child: _MapActionTile(
                    onTap: onExplore,
                  ),
                ),
                const SizedBox(width: 0),
                // QR quick action [195,278 183x136]
                Expanded(
                  child: _QrActionTile(onTap: onScan),
                ),
              ],
            ),
          ),
          const SizedBox(height: 26),

          // ── Ближайший клуб: подтверждение ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: _NearbyVenueRow(
              room: DemoData.katerBlau,
              onJoin: () => onOpenRoom(DemoData.katerBlau),
            ),
          ),
          const SizedBox(height: 26),

          // ── Рекомендации ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const SectionLabel('YOUR NIGHT STARTS HERE'),
                const SizedBox(height: 10),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Text('Set the mood',
                        style: DropType.sectionTitle.copyWith(
                            fontSize: 18, fontWeight: FontWeight.w700)),
                    InkWell(
                      onTap: onScan,
                      child: Row(
                        children: [
                          Text('Browse all',
                              style: DropType.caption.copyWith(
                                  fontSize: 12,
                                  fontWeight: FontWeight.w600,
                                  color: DropTokens.accent)),
                          const SizedBox(width: 4),
                          const Icon(Icons.arrow_forward,
                              size: 14, color: DropTokens.accent),
                        ],
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                for (final track in [
                  DemoData.trackMove,
                  DemoData.trackOneMoreTime,
                ]) ...[
                  _RecommendationRow(
                    track: track,
                    onAdd: () => onRequestTrack(track),
                  ),
                  const SizedBox(height: 10),
                ],
                const SizedBox(height: 2),
                const Row(
                  children: [
                    Icon(Icons.info_outline, size: 12, color: DropTokens.muted),
                    SizedBox(width: 6),
                    Text(
                      'Join a room to send your pick to the DJ.',
                      style: DropType.caption,
                    ),
                  ],
                ),
              ],
            ),
          ),
          const SizedBox(height: 100),
        ],
      ),
    );
  }
}

/// Плитка «Nearby rooms» — карта с затемнением и подписями.
class _MapActionTile extends StatelessWidget {
  const _MapActionTile({required this.onTap});

  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      child: ClipRRect(
        borderRadius: const BorderRadius.horizontal(left: Radius.circular(28)),
        child: SizedBox(
          height: 136,
          child: Stack(
            fit: StackFit.expand,
            children: [
              Image.asset('assets/images/map_home.jpg', fit: BoxFit.cover),
              const DecoratedBox(
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                    colors: [Color(0x00101016), Color(0xE6101016)],
                  ),
                ),
              ),
              const Padding(
                padding: EdgeInsets.all(14),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Spacer(),
                    Text('Nearby rooms',
                        style: TextStyle(
                          fontFamily: 'Inter',
                          fontSize: 16,
                          fontWeight: FontWeight.w600,
                          color: Colors.white,
                        )),
                    SizedBox(height: 2),
                    Text('12 live  ·  0.2 km',
                        style: TextStyle(
                          fontFamily: 'Inter',
                          fontSize: 12,
                          color: Colors.white,
                        )),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Плитка «Scan to enter» — мятный акцент с QR-бейджем.
class _QrActionTile extends StatelessWidget {
  const _QrActionTile({required this.onTap});

  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      child: Container(
        height: 136,
        decoration: const BoxDecoration(
          color: DropTokens.accent,
          borderRadius: BorderRadius.horizontal(right: Radius.circular(28)),
        ),
        padding: const EdgeInsets.all(14),
        child: const Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Align(
              alignment: Alignment.topRight,
              child: _QrBadge(),
            ),
            Spacer(),
            Text('Scan to enter',
                style: TextStyle(
                  fontFamily: 'Inter',
                  fontSize: 16,
                  fontWeight: FontWeight.w600,
                  color: DropTokens.onAccent,
                )),
            SizedBox(height: 2),
            Text('Club QR · instant access',
                style: TextStyle(
                  fontFamily: 'Inter',
                  fontSize: 12,
                  color: DropTokens.qrHint,
                )),
          ],
        ),
      ),
    );
  }
}

class _QrBadge extends StatelessWidget {
  const _QrBadge();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 48,
      height: 48,
      decoration: BoxDecoration(
        color: DropTokens.onAccent,
        borderRadius: BorderRadius.circular(DropTokens.rXs),
      ),
      child: const Icon(Icons.qr_code_2, size: 28, color: DropTokens.accent),
    );
  }
}

/// Строка ближайшего клуба с кнопкой Join.
class _NearbyVenueRow extends StatelessWidget {
  const _NearbyVenueRow({required this.room, required this.onJoin});

  final Room room;
  final VoidCallback onJoin;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 72,
      padding: const EdgeInsets.symmetric(horizontal: 12),
      decoration: BoxDecoration(
        color: DropTokens.surface,
        borderRadius: BorderRadius.circular(DropTokens.rMd),
        border: Border.all(color: DropTokens.border),
      ),
      child: Row(
        children: [
          ArtTile(path: room.image, size: 40, radius: DropTokens.rXs),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(room.name,
                    style: const TextStyle(
                      fontFamily: 'Inter',
                      fontSize: 14,
                      fontWeight: FontWeight.w500,
                      color: DropTokens.textSoft,
                    )),
                const SizedBox(height: 2),
                Text('${room.genre} · ${room.distance} away',
                    style: DropType.caption),
              ],
            ),
          ),
          InkWell(
            onTap: onJoin,
            borderRadius: BorderRadius.circular(999),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
              child: Text('Join',
                  style: DropType.caption.copyWith(
                    fontSize: 14,
                    fontWeight: FontWeight.w600,
                    color: DropTokens.accent,
                  )),
            ),
          ),
          const Icon(Icons.chevron_right, size: 18, color: DropTokens.muted2),
        ],
      ),
    );
  }
}

/// Строка рекомендации трека с круглой кнопкой запроса.
class _RecommendationRow extends StatelessWidget {
  const _RecommendationRow({required this.track, required this.onAdd});

  final Track track;
  final VoidCallback onAdd;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        ArtTile(path: track.art, size: 50, radius: DropTokens.rXs),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(track.title,
                  style: DropType.trackTitle.copyWith(color: DropTokens.text)),
              const SizedBox(height: 2),
              Text('${track.artist} · ${track.genre}',
                  style: DropType.caption, overflow: TextOverflow.ellipsis),
            ],
          ),
        ),
        const SizedBox(width: 10),
        _AddTrackButton(onTap: onAdd),
      ],
    );
  }
}

class _AddTrackButton extends StatelessWidget {
  const _AddTrackButton({required this.onTap});

  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 34,
      height: 34,
      child: Material(
        color: DropTokens.accent,
        borderRadius: BorderRadius.circular(DropTokens.rXs),
        child: InkWell(
          borderRadius: BorderRadius.circular(DropTokens.rXs),
          onTap: onTap,
          child: const Icon(Icons.add, size: 18, color: DropTokens.onAccent),
        ),
      ),
    );
  }
}
