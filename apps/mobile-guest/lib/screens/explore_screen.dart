/// Guest · 11 · Explore — Nearby rooms.
///
/// Макет pen.dev: поиск комнат, жанровые чипы, featured-карточка с
/// live-бейджем и кнопкой открытия, список комнат с «NOW PLAYING».
library;

import 'package:flutter/material.dart';

import '../data/demo_data.dart';
import '../data/models.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class ExploreScreen extends StatefulWidget {
  const ExploreScreen({super.key, required this.onOpenRoom});

  final ValueChanged<Room> onOpenRoom;

  @override
  State<ExploreScreen> createState() => _ExploreScreenState();
}

class _ExploreScreenState extends State<ExploreScreen> {
  String _query = '';
  // В макете чип House подсвечен, но показаны все комнаты: фильтр
  // применяется только после явного выбора жанра.
  String? _genre;
  bool _nearestFirst = true;

  static const _genres = ['House', 'Techno', 'Afro house', 'Disco', 'Trance'];

  List<Room> get _visible {
    final q = _query.trim().toLowerCase();
    final rooms = DemoData.rooms.where((r) {
      final matchQuery = q.isEmpty ||
          r.name.toLowerCase().contains(q) ||
          r.dj.toLowerCase().contains(q) ||
          r.genre.toLowerCase().contains(q);
      final matchGenre =
          _genre == null || r.genre.toLowerCase() == _genre!.toLowerCase();
      return matchQuery && matchGenre;
    }).toList();
    rooms.sort((a, b) {
      final order =
          _distanceMeters(a.distance).compareTo(_distanceMeters(b.distance));
      return _nearestFirst ? order : -order;
    });
    return rooms;
  }

  int _distanceMeters(String value) {
    final number =
        double.tryParse(value.replaceAll(RegExp(r'[^0-9.]'), '')) ?? 0;
    return (number * (value.contains('km') ? 1000 : 1)).round();
  }

  @override
  Widget build(BuildContext context) {
    final rooms = _visible;
    final featured = rooms.where((r) => r.featured).firstOrNull;

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
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Explore', style: DropType.screenTitle),
                SizedBox(height: 6),
                Text(
                  'Nearby rooms in Berlin. Pick your floor, drop your track.',
                  style: DropType.caption,
                ),
              ],
            ),
          ),
          const SizedBox(height: 18),

          // ── Поиск комнат ──
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
                        hintText: 'Search rooms, clubs or DJs',
                        hintStyle: DropType.bodySm
                            .copyWith(fontSize: 13, color: DropTokens.muted),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 14),

          // ── Жанровые чипы ──
          SizedBox(
            height: 34,
            child: ListView(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: 20),
              children: [
                for (final g in _genres) ...[
                  FilterChipPill(
                    label: g,
                    active: (_genre ?? 'House') == g,
                    radius: DropTokens.rSm,
                    onTap: () =>
                        setState(() => _genre = _genre == g ? null : g),
                  ),
                  const SizedBox(width: 8),
                ],
              ],
            ),
          ),
          const SizedBox(height: 20),

          // ── Сводка и сортировка ──
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: Wrap(
              alignment: WrapAlignment.spaceBetween,
              spacing: 8,
              runSpacing: 8,
              children: [
                Text(
                    rooms.length == 1
                        ? '1 ROOM WITHIN 2 KM'
                        : '${rooms.length} ROOMS WITHIN 2 KM',
                    style: DropType.labelTight),
                InkWell(
                  onTap: () => setState(() => _nearestFirst = !_nearestFirst),
                  borderRadius: BorderRadius.circular(999),
                  child: Padding(
                    padding:
                        const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const Icon(Icons.swap_vert,
                            size: 12, color: DropTokens.accent),
                        const SizedBox(width: 6),
                        Text(_nearestFirst ? 'Distance ↑' : 'Distance ↓',
                            style: DropType.caption
                                .copyWith(color: DropTokens.text)),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 12),

          // ── Featured-карточка ──
          if (featured != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: _FeaturedRoomCard(
                room: featured,
                onOpen: () => widget.onOpenRoom(featured),
              ),
            ),
          const SizedBox(height: 12),

          // ── Список комнат ──
          for (final room in rooms.where((r) => !r.featured))
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 0, 20, 4),
              child: _RoomRow(
                room: room,
                onOpen: () => widget.onOpenRoom(room),
              ),
            ),
          const SizedBox(height: 24),
        ],
      ),
    );
  }
}

class _FeaturedRoomCard extends StatelessWidget {
  const _FeaturedRoomCard({required this.room, required this.onOpen});

  final Room room;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onOpen,
      child: ClipRRect(
        borderRadius: BorderRadius.circular(DropTokens.rSm),
        child: SizedBox(
          height: 132 + (MediaQuery.textScalerOf(context).scale(12) - 12) * 14,
          child: Stack(
            fit: StackFit.expand,
            children: [
              Image.asset(room.image, fit: BoxFit.cover),
              const DecoratedBox(
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                    colors: [Color(0x00101016), Color(0xF2000000)],
                  ),
                ),
              ),
              Padding(
                padding: const EdgeInsets.all(12),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Container(
                          padding: const EdgeInsets.symmetric(
                              horizontal: 8, vertical: 4),
                          decoration: BoxDecoration(
                            color: DropTokens.liveTint,
                            borderRadius: BorderRadius.circular(8),
                          ),
                          child: Row(
                            children: [
                              Container(
                                width: 6,
                                height: 6,
                                decoration: const BoxDecoration(
                                  color: DropTokens.live,
                                  shape: BoxShape.circle,
                                ),
                              ),
                              const SizedBox(width: 6),
                              const Text(
                                'LIVE NOW',
                                style: TextStyle(
                                  fontFamily: 'Inter',
                                  fontSize: 12,
                                  fontWeight: FontWeight.w700,
                                  color: Colors.white,
                                ),
                              ),
                            ],
                          ),
                        ),
                        const Spacer(),
                        GestureDetector(
                          onTap: onOpen,
                          child: Container(
                            width: 36,
                            height: 36,
                            decoration: const BoxDecoration(
                              color: DropTokens.accent,
                              shape: BoxShape.circle,
                            ),
                            child: const Icon(Icons.arrow_forward,
                                size: 16, color: DropTokens.onAccent),
                          ),
                        ),
                      ],
                    ),
                    const Spacer(),
                    Text(
                      '${room.name} · ${room.session}',
                      style:
                          DropType.featuredRoom.copyWith(color: Colors.white),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      '${room.genre} · ${room.distance} · ${room.dj} behind the decks',
                      style: DropType.caption
                          .copyWith(color: DropTokens.featuredMeta),
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

class _RoomRow extends StatelessWidget {
  const _RoomRow({required this.room, required this.onOpen});

  final Room room;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onOpen,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 12),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(room.name,
                      style: DropType.trackTitle,
                      overflow: TextOverflow.ellipsis),
                  const SizedBox(height: 3),
                  Text('${room.genre} · ${room.distance}',
                      style: DropType.caption),
                  const SizedBox(height: 3),
                  Text('DJ · ${room.dj}',
                      style: DropType.caption.copyWith(
                        fontSize: 12,
                        fontWeight: FontWeight.w500,
                        color: DropTokens.accent,
                      )),
                ],
              ),
            ),
            const SizedBox(width: 10),
            Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                const Text('NOW PLAYING', style: DropType.labelTight),
                const SizedBox(height: 3),
                Text(
                  room.nowPlaying ?? '—',
                  style: DropType.caption.copyWith(color: DropTokens.text),
                ),
              ],
            ),
            const SizedBox(width: 6),
            const Icon(Icons.chevron_right, size: 14, color: DropTokens.muted),
          ],
        ),
      ),
    );
  }
}
