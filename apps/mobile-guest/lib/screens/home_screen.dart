import 'package:flutter/material.dart';

import '../theme/tokens.dart';
import '../widgets/artwork.dart';
import '../widgets/app_button.dart';
import '../widgets/nav.dart';
import '../widgets/track_row.dart';

/// Guest · 01 · Home
///
/// Макет: `docs/ui/reference/dropit-design.pen`, кадр «Guest · 01 · Home».
/// Порядок и размеры из макета: бренд-строка 36, герой-блок 184 с плитками
/// действий 136, лейбл «YOUR NIGHT STARTS HERE», строка заведения 72,
/// блок «Set the mood» с рекомендациями и парящий таб-бар 64 (радиус 22).
class HomeScreen extends StatelessWidget {
  const HomeScreen({
    super.key,
    required this.onScan,
    required this.onNearby,
    this.onExplore,
    this.onRequests,
    this.onProfile,
    this.onTrackTap,
    this.city = 'Berlin',
    this.liveRooms,
    this.roomName,
    this.roomMeta,
  });

  final VoidCallback onScan;
  final VoidCallback onNearby;
  final VoidCallback? onExplore;
  final VoidCallback? onRequests;
  final VoidCallback? onProfile;
  final ValueChanged<String>? onTrackTap;
  final String city;
  final String? liveRooms;
  final String? roomName;
  final String? roomMeta;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: DropTokens.bg,
      body: SafeArea(
        bottom: false,
        child: Column(
          children: [
            Expanded(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(20, 24, 20, 110),
                children: [
                  BrandBar(city: city),
                  const SizedBox(height: 18),
                  const _HeaderBlock(),
                  const SizedBox(height: 16),
                  _ActionTiles(onScan: onScan, onNearby: onNearby),
                  const SizedBox(height: 60),
                  const SectionLabel('YOUR NIGHT STARTS HERE'),
                  const SizedBox(height: 10),
                  _VenueRow(
                    roomName: roomName,
                    roomMeta: roomMeta,
                    onJoin: onNearby,
                  ),
                  const SizedBox(height: 36),
                  _SetTheMood(onTrackTap: onTrackTap),
                ],
              ),
            ),
          ],
        ),
      ),
      bottomNavigationBar: _HomeTabBar(
        onRequests: onRequests,
        onExplore: onExplore,
        onProfile: onProfile,
      ),
    );
  }
}

/// Герой-блок: фон-изображение 184 с контрастным оверлеем, бейдж
/// «12 ROOMS LIVE TONIGHT» и заголовок Anton 34.
class _HeaderBlock extends StatelessWidget {
  const _HeaderBlock({this.liveRooms});

  final String? liveRooms;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8),
      child: SizedBox(
        height: 184,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(DropTokens.rHero),
          child: Stack(
            fit: StackFit.expand,
            children: [
              const DecoratedBox(
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.topLeft,
                    end: Alignment.bottomRight,
                    colors: [DropTokens.surface2, DropTokens.surface],
                  ),
                ),
              ),
              const DecoratedBox(
                decoration: BoxDecoration(gradient: DropTokens.heroOverlay),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 18),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Container(
                          width: 6,
                          height: 6,
                          decoration: const BoxDecoration(
                            color: DropTokens.accent,
                            shape: BoxShape.circle,
                          ),
                        ),
                        const SizedBox(width: 6),
                        Text(
                          liveRooms ?? '12 ROOMS LIVE TONIGHT',
                          style: DropType.metaSm.copyWith(
                            fontSize: 9,
                            fontWeight: FontWeight.w700,
                            letterSpacing: 1,
                            color: Colors.white,
                          ),
                        ),
                      ],
                    ),
                    const Spacer(),
                    const Text('Your track.\nTheir next move.',
                        style: DropType.screenTitle),
                    const SizedBox(height: 8),
                    Text(
                      'Join a club. Request a track. Own the night.',
                      style: DropType.meta.copyWith(color: DropTokens.caption),
                    ),
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

/// Пара плиток действий: слева «Nearby rooms», справа акцентная «Scan to enter»
/// (внешние углы 22, внутренние прямые — как в макете).
class _ActionTiles extends StatelessWidget {
  const _ActionTiles({required this.onScan, required this.onNearby});

  final VoidCallback onScan;
  final VoidCallback onNearby;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 136,
      child: Row(
        children: [
          Expanded(
            child: _Tile(
              title: 'Nearby rooms',
              hint: '12 live  ·  0.2 km',
              borderRadius: const BorderRadius.only(
                topLeft: Radius.circular(DropTokens.rTile),
                bottomLeft: Radius.circular(DropTokens.rTile),
              ),
              background: const LinearGradient(
                begin: Alignment.topCenter,
                end: Alignment.bottomCenter,
                colors: [DropTokens.surface2, DropTokens.rail],
              ),
              titleColor: Colors.white,
              onTap: onNearby,
            ),
          ),
          Expanded(
            child: _Tile(
              title: 'Scan to enter',
              hint: 'Club QR · instant access',
              borderRadius: const BorderRadius.only(
                topRight: Radius.circular(DropTokens.rTile),
                bottomRight: Radius.circular(DropTokens.rTile),
              ),
              background: const LinearGradient(
                begin: Alignment.topCenter,
                end: Alignment.bottomCenter,
                colors: [DropTokens.accent, DropTokens.accent],
              ),
              titleColor: DropTokens.onAccent,
              hintColor: const Color(0xFF17452F),
              icon: _ScanBadge(),
              onTap: onScan,
            ),
          ),
        ],
      ),
    );
  }
}

class _Tile extends StatelessWidget {
  const _Tile({
    required this.title,
    required this.hint,
    required this.borderRadius,
    required this.background,
    required this.titleColor,
    this.hintColor,
    this.icon,
    required this.onTap,
  });

  final String title;
  final String hint;
  final BorderRadius borderRadius;
  final Gradient background;
  final Color titleColor;
  final Color? hintColor;
  final Widget? icon;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      label: '$title. $hint',
      child: ClipRRect(
        borderRadius: borderRadius,
        child: Material(
          color: Colors.transparent,
          child: Ink(
            decoration: BoxDecoration(gradient: background),
            child: InkWell(
              onTap: onTap,
              child: Stack(
                children: [
                  if (icon != null)
                    Positioned(top: 16, right: 16, child: icon!),
                  Positioned(
                    left: 18,
                    right: 12,
                    bottom: 16,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          title,
                          style: DropType.bodySm.copyWith(
                            fontSize: 16,
                            fontWeight: FontWeight.w600,
                            height: 1.25,
                            color: titleColor,
                          ),
                        ),
                        const SizedBox(height: 8),
                        Text(
                          hint,
                          style: DropType.metaSm.copyWith(
                            color: hintColor ?? DropTokens.muted,
                          ),
                        ),
                      ],
                    ),
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

class _ScanBadge extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Container(
      width: 48,
      height: 48,
      decoration: BoxDecoration(
        color: DropTokens.onAccent,
        borderRadius: BorderRadius.circular(DropTokens.rLg),
      ),
      alignment: Alignment.center,
      child: const Icon(Icons.qr_code_2, size: 28, color: DropTokens.accent),
    );
  }
}

/// Строка ближайшего заведения: фото 40, название, «Join» акцентом.
class _VenueRow extends StatelessWidget {
  const _VenueRow({this.roomName, this.roomMeta, required this.onJoin});

  final String? roomName;
  final String? roomMeta;
  final VoidCallback onJoin;

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onJoin,
      child: Container(
        height: 72,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: const BoxDecoration(
          border: Border(bottom: BorderSide(color: DropTokens.borderSoft)),
        ),
        child: Row(
          children: [
            const Artwork(url: null, size: 40, radius: DropTokens.rXs),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Text(roomName ?? 'Kater Blau',
                      style: DropType.bodySm.copyWith(
                        fontSize: 14,
                        fontWeight: FontWeight.w500,
                        height: 1,
                        color: DropTokens.textSoft,
                      )),
                  const SizedBox(height: 5),
                  Text(roomMeta ?? 'House · 200 m away',
                      style: DropType.caption.copyWith(
                        color: DropTokens.muted2,
                        height: 1,
                      )),
                ],
              ),
            ),
            Text('Join',
                style: DropType.bodySm.copyWith(
                  fontSize: 14,
                  fontWeight: FontWeight.w600,
                  height: 1,
                  color: DropTokens.accent,
                )),
            const SizedBox(width: 6),
            const Icon(Icons.chevron_right, size: 18, color: DropTokens.muted2),
          ],
        ),
      ),
    );
  }
}

/// «Set the mood» + «Browse all» + две рекомендации треков.
class _SetTheMood extends StatelessWidget {
  const _SetTheMood({this.onTrackTap});

  final ValueChanged<String>? onTrackTap;

  static const _tracks = [
    ('Move', 'Adam Port, Stryv · Afro house'),
    ('One More Time', 'Daft Punk · Dance classics'),
  ];

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            const Text('Set the mood', style: DropType.sectionTitle),
            Row(
              children: [
                Text('Browse all',
                    style: DropType.meta.copyWith(
                      fontWeight: FontWeight.w600,
                      color: DropTokens.accent,
                    )),
                const SizedBox(width: 4),
                const Icon(Icons.arrow_forward,
                    size: 14, color: DropTokens.accent),
              ],
            ),
          ],
        ),
        const SizedBox(height: 12),
        for (final (title, meta) in _tracks) ...[
          TrackRow(
            title: title,
            subtitle: meta,
            trailing: AppIconButton(
              icon: Icons.add,
              tooltip: 'Request $title',
              onPressed: () => onTrackTap?.call(title),
            ),
          ),
          const SizedBox(height: 12),
        ],
        const SizedBox(height: 4),
        Row(
          children: [
            const Icon(Icons.info_outline, size: 12, color: DropTokens.muted),
            const SizedBox(width: 6),
            Expanded(
              child: Text(
                'Join a room to send your pick to the DJ.',
                style: DropType.metaSm.copyWith(color: DropTokens.muted),
              ),
            ),
          ],
        ),
      ],
    );
  }
}

/// Парящий таб-бар 64 с радиусом 22 (Listen · Explore · Requests · Profile).
class _HomeTabBar extends StatelessWidget {
  const _HomeTabBar({this.onRequests, this.onExplore, this.onProfile});

  final VoidCallback? onRequests;
  final VoidCallback? onExplore;
  final VoidCallback? onProfile;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
      child: Container(
        height: DropTokens.tabBarHeight,
        decoration: BoxDecoration(
          color: DropTokens.bg,
          borderRadius: BorderRadius.circular(DropTokens.rTile),
          border: const Border(top: BorderSide(color: DropTokens.borderSoft)),
        ),
        padding: const EdgeInsets.symmetric(horizontal: 8),
        child: Row(
          children: [
            const _TabItem(
                icon: Icons.graphic_eq,
                label: 'Listen',
                active: true),
            _TabItem(
                icon: Icons.explore_outlined,
                label: 'Explore',
                onTap: onExplore),
            _TabItem(
                icon: Icons.receipt_long_outlined,
                label: 'Requests',
                onTap: onRequests),
            _TabItem(
                icon: Icons.person_outline, label: 'Profile', onTap: onProfile),
          ],
        ),
      ),
    );
  }
}

class _TabItem extends StatelessWidget {
  const _TabItem({
    required this.icon,
    required this.label,
    this.active = false,
    this.onTap,
  });

  final IconData icon;
  final String label;
  final bool active;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final color = active ? DropTokens.accent : DropTokens.muted3;
    return Expanded(
      child: InkWell(
        onTap: onTap,
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 21, color: color),
            const SizedBox(height: 5),
            Text(
              label,
              style: DropType.meta.copyWith(
                fontSize: 11,
                fontWeight: active ? FontWeight.w600 : FontWeight.w500,
                color: active ? DropTokens.textTabActive : color,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
