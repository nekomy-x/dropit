/// Guest · 03 · Search — выбор трека после join.
///
/// Макет pen.dev: шапка комнаты, вводный блок, поле поиска, жанровые
/// фильтры, результаты с превью и полосой прогресса, нижний блок
/// подтверждения выбранного трека и кнопка «Continue to payment».
library;

import 'package:flutter/material.dart';
import 'dart:async';

import '../data/demo_data.dart';
import '../data/models.dart';
import '../state/app_state.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class SearchScreen extends StatefulWidget {
  const SearchScreen({
    super.key,
    required this.session,
    required this.onBack,
    required this.onContinue,
  });

  final JoinSession session;
  final VoidCallback onBack;
  final ValueChanged<Track> onContinue;

  @override
  State<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends State<SearchScreen> {
  final _searchController = TextEditingController();
  String _query = '';
  String _genre = 'All';
  List<Track> _tracks = DemoData.tracks;
  Timer? _searchDebounce;
  bool _loading = false;
  String? _searchError;

  /// Трек с проигрываемым превью (демо-плеер: позиция 0:09 из 0:30).
  Track? _previewing;
  double _previewPos = 0.30; // 0:09 из 0:30

  static const _genres = ['All', 'House', 'Afro house'];

  List<Track> get _visible {
    final q = _query.trim().toLowerCase();
    return _tracks.where((t) {
      final matchQuery = q.isEmpty ||
          t.title.toLowerCase().contains(q) ||
          t.artist.toLowerCase().contains(q);
      final matchGenre =
          _genre == 'All' || t.genre.toLowerCase() == _genre.toLowerCase();
      return matchQuery && matchGenre;
    }).toList();
  }

  void _onSearchChanged(String value) {
    setState(() {
      _query = value;
      _searchError = null;
    });
    _searchDebounce?.cancel();
    if (value.trim().length < 2) {
      setState(() => _tracks = DemoData.tracks);
      return;
    }
    _searchDebounce = Timer(const Duration(milliseconds: 350), () async {
      if (!mounted) return;
      final state = AppState.of(context);
      setState(() => _loading = true);
      try {
        final tracks = await state.searchTracks(value);
        if (!mounted) return;
        setState(() => _tracks = tracks);
      } catch (_) {
        if (!mounted) return;
        setState(() {
          _tracks = const [];
          _searchError = 'Could not search the catalog. Try again.';
        });
      } finally {
        if (mounted) setState(() => _loading = false);
      }
    });
  }

  void _togglePreview(Track track) {
    setState(() {
      if (_previewing?.id == track.id) {
        _previewing = null;
      } else {
        _previewing = track;
        _previewPos = 0.30;
      }
    });
  }

  @override
  void dispose() {
    _searchDebounce?.cancel();
    _searchController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = AppState.of(context);
    final selected = state.selectedTrack;
    final tracks = _visible;

    return DropScaffold(
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const SizedBox(height: 24),
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: 20),
              child: BrandCityRow(),
            ),
            const SizedBox(height: 18),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: JoinRoomCard(room: widget.session.room),
            ),
            const SizedBox(height: 22),

            // ── Заголовок ──
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: 20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('Choose your track.', style: DropType.screenTitle),
                  SizedBox(height: 6),
                  Text(
                    'Preview a track. Select your next drop.',
                    style: DropType.caption,
                  ),
                ],
              ),
            ),
            const SizedBox(height: 18),

            // ── Поле поиска ──
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: Container(
                height: 54,
                decoration: const BoxDecoration(
                  border: Border(
                    bottom: BorderSide(color: DropTokens.border),
                  ),
                ),
                child: Row(
                  children: [
                    const Icon(Icons.search, size: 20, color: DropTokens.muted),
                    const SizedBox(width: 10),
                    Expanded(
                      child: TextField(
                        controller: _searchController,
                        onChanged: _onSearchChanged,
                        style: DropType.bodySm.copyWith(fontSize: 13),
                        cursorColor: DropTokens.accent,
                        decoration: InputDecoration(
                          border: InputBorder.none,
                          enabledBorder: InputBorder.none,
                          focusedBorder: InputBorder.none,
                          isCollapsed: true,
                          filled: false,
                          hintText: 'Move',
                          hintStyle: DropType.bodySm
                              .copyWith(fontSize: 13, color: DropTokens.text),
                        ),
                      ),
                    ),
                    if (_query.isNotEmpty)
                      GestureDetector(
                        onTap: () {
                          _searchController.clear();
                          _onSearchChanged('');
                        },
                        child: const Icon(Icons.close,
                            size: 18, color: DropTokens.muted),
                      ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 16),

            // ── Жанровые фильтры ──
            SizedBox(
              height: 36,
              child: ListView(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(horizontal: 20),
                children: [
                  for (final g in _genres) ...[
                    FilterChipPill(
                      label: g,
                      active: _genre == g,
                      // В макете активный «All» — светлый чип с тёмным текстом.
                      activeFill: g == 'All' ? DropTokens.text : null,
                      activeText: g == 'All' ? DropTokens.bg : null,
                      onTap: () => setState(() => _genre = g),
                    ),
                    const SizedBox(width: 8),
                  ],
                ],
              ),
            ),
            const SizedBox(height: 20),

            // ── Результаты ──
            Expanded(
              child: ListView(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                children: [
                  Text(
                    _loading
                        ? 'SEARCHING…'
                        : '${tracks.length} TRACKS · SELECT ONE',
                    style: DropType.labelTight,
                  ),
                  const SizedBox(height: 12),
                  if (_searchError != null)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 12),
                      child: Text(
                        _searchError!,
                        style: DropType.caption.copyWith(color: DropTokens.red),
                      ),
                    ),
                  if (!_loading && tracks.isEmpty)
                    const Padding(
                      padding: EdgeInsets.symmetric(vertical: 32),
                      child: Center(
                          child: Text('No tracks found.',
                              style: DropType.caption)),
                    ),
                  for (final track in tracks) ...[
                    _TrackResultRow(
                      track: track,
                      selected: selected?.id == track.id,
                      previewing: _previewing?.id == track.id,
                      previewPos: _previewPos,
                      onSelect: () => state.selectTrack(track),
                      onPreview: () => _togglePreview(track),
                    ),
                    const SizedBox(height: 8),
                  ],
                ],
              ),
            ),

            // ── Подтверждение выбранного трека ──
            if (selected != null)
              Container(
                decoration: const BoxDecoration(
                  color: DropTokens.bg,
                  border: Border(
                    top: BorderSide(color: DropTokens.border),
                  ),
                ),
                padding: const EdgeInsets.fromLTRB(20, 16, 20, 20),
                child: Column(
                  children: [
                    Row(
                      children: [
                        const SectionLabel('SELECTED TRACK'),
                        const Spacer(),
                        Text(selected.priceLabel, style: DropType.caption),
                      ],
                    ),
                    const SizedBox(height: 12),
                    Row(
                      children: [
                        ArtTile(path: selected.art, size: 50),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(selected.title, style: DropType.trackTitle),
                              const SizedBox(height: 2),
                              Text(
                                selected.artist,
                                style: DropType.caption.copyWith(height: 1.2),
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 16),
                    PrimaryButton(
                      label: 'Continue to payment',
                      onPressed: () => widget.onContinue(selected),
                    ),
                  ],
                ),
              )
            else
              const SizedBox(height: 20),
          ],
        ),
      ),
    );
  }
}

class _TrackResultRow extends StatelessWidget {
  const _TrackResultRow({
    required this.track,
    required this.selected,
    required this.previewing,
    required this.previewPos,
    required this.onSelect,
    required this.onPreview,
  });

  final Track track;
  final bool selected;
  final bool previewing;
  final double previewPos;
  final VoidCallback onSelect;
  final VoidCallback onPreview;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(vertical: 10, horizontal: 12),
      decoration: BoxDecoration(
        color: selected ? DropTokens.surface : Colors.transparent,
        borderRadius: BorderRadius.circular(DropTokens.rMd),
        border: Border.all(
          color: selected ? DropTokens.tintStrong2 : DropTokens.border,
        ),
      ),
      child: Column(
        children: [
          Row(
            children: [
              GestureDetector(
                onTap: onSelect,
                child: ArtTile(path: track.art, size: 50),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: GestureDetector(
                  onTap: onSelect,
                  behavior: HitTestBehavior.opaque,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(track.title,
                          style: DropType.trackTitle,
                          overflow: TextOverflow.ellipsis),
                      const SizedBox(height: 2),
                      Text('${track.artist} · ${track.genre}',
                          style: DropType.caption,
                          overflow: TextOverflow.ellipsis),
                    ],
                  ),
                ),
              ),
              const SizedBox(width: 10),
              _SelectControl(
                selected: selected,
                onTap: onSelect,
              ),
            ],
          ),
          const SizedBox(height: 6),
          Row(
            children: [
              InkWell(
                onTap: onPreview,
                borderRadius: BorderRadius.circular(999),
                child: Padding(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                  child: Row(
                    children: [
                      Icon(
                        previewing ? Icons.pause : Icons.play_arrow,
                        size: 15,
                        color:
                            previewing ? DropTokens.accent : DropTokens.muted4,
                      ),
                      const SizedBox(width: 6),
                      Text(
                        previewing ? 'Playing preview' : 'Preview',
                        style: TextStyle(
                          fontFamily: 'Inter',
                          fontSize: 12,
                          fontWeight: FontWeight.w500,
                          color: previewing
                              ? DropTokens.accent
                              : DropTokens.muted4,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
              const Spacer(),
              if (previewing) ...[
                const Text('0:09 / 0:30', style: DropType.caption),
                const SizedBox(width: 12),
                SizedBox(
                  width: 80,
                  child: ClipRRect(
                    borderRadius: BorderRadius.circular(2),
                    child: LinearProgressIndicator(
                      value: previewPos,
                      minHeight: 4,
                      backgroundColor: DropTokens.railTrack,
                      valueColor:
                          const AlwaysStoppedAnimation(DropTokens.accent),
                    ),
                  ),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }
}

class _SelectControl extends StatelessWidget {
  const _SelectControl({required this.selected, required this.onTap});

  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      child: Container(
        width: 44,
        height: 44,
        alignment: Alignment.center,
        child: Container(
          width: 24,
          height: 24,
          decoration: BoxDecoration(
            color: selected ? DropTokens.accent : Colors.transparent,
            shape: BoxShape.circle,
            border: selected
                ? null
                : Border.all(color: DropTokens.inactive, width: 2),
          ),
          child: selected
              ? const Icon(Icons.check, size: 15, color: DropTokens.onAccent)
              : null,
        ),
      ),
    );
  }
}
