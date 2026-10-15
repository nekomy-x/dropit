import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../theme/tokens.dart';
import '../widgets/artwork.dart';
import '../widgets/app_button.dart';
import '../widgets/app_field.dart';
import '../widgets/chips.dart';
import '../widgets/nav.dart';
import '../widgets/track_row.dart';

/// Guest · 03 · Search — «Choose your track.»
///
/// Макет: кадр «Guest · 03 · Search» (`docs/ui/reference/dropit-design.pen`).
/// Поле поиска 54 с радиусом 12, жанровые фильтры-таблетки 36, строки
/// результатов с обложкой 50 и контролом выбора 44, у выбранной строки
/// раскрываются превью-контролы и полоса проигрывания 4 px, внизу —
/// закреплённый блок «SELECTED TRACK» с ценой сессии и переходом к оплате.
///
/// ЗАГЛУШКА (правило 5): каталог — локальный демо-список; клиентский поиск
/// iTunes и серверная whitelist-валидация (F3/B5) ждут лицензий. Контракт
/// запроса (`POST /v1/requests`) при этом настоящий.
class SearchScreen extends StatefulWidget {
  const SearchScreen({
    super.key,
    required this.session,
    required this.onRequestTrack,
    this.onOpenRequests,
  });

  final JoinResponse session;
  final void Function(CatalogTrack) onRequestTrack;
  final VoidCallback? onOpenRequests;

  @override
  State<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends State<SearchScreen> {
  final _controller = TextEditingController();
  String _query = 'Move';
  String _genre = 'All';
  int? _selected;

  static const _genres = ['All', 'House', 'Afro house'];

  // Демо-каталог (ЗАГЛУШКА, правило 5) — данные как в макете.
  static const _catalog = <_DemoTrack>[
    _DemoTrack('itunes:1733045646', 'Move', 'Adam Port, Stryv · Afro house',
        genre: 'Afro house'),
    _DemoTrack('itunes:1678172291', 'Move Your Body', 'Marshall Jefferson · House',
        genre: 'House'),
    _DemoTrack('itunes:1573339886', 'Move (Extended Mix)',
        'Adam Port, Stryv · Afro house',
        genre: 'Afro house'),
  ];

  List<_DemoTrack> get _results {
    final q = _query.trim().toLowerCase();
    return _catalog.where((t) {
      final matchesGenre = _genre == 'All' || t.genre == _genre;
      final matchesQuery =
          q.isEmpty || t.title.toLowerCase().contains(q) ||
              t.artistLine.toLowerCase().contains(q);
      return matchesGenre && matchesQuery;
    }).toList();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _continue() {
    final track = _results[_selected!];
    widget.onRequestTrack(
      CatalogTrack(id: track.id, title: track.title, artist: track.artistLine),
    );
  }

  @override
  Widget build(BuildContext context) {
    final results = _results;
    final selected = _selected != null && _selected! < results.length
        ? results[_selected!]
        : null;

    return Scaffold(
      backgroundColor: DropTokens.bg,
      body: SafeArea(
        bottom: false,
        child: Column(
          children: [
            Expanded(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(20, 24, 20, 8),
                children: [
                  BrandBar(
                    trailing: widget.onOpenRequests == null
                        ? null
                        : IconButton(
                            icon: const Icon(Icons.receipt_long_outlined,
                                size: 22, color: DropTokens.text),
                            tooltip: 'My requests',
                            onPressed: widget.onOpenRequests,
                          ),
                  ),
                  const SizedBox(height: 14),
                  JoinedRoomBar(
                    roomName: widget.session.clubName,
                    sessionLine: _sessionLine(),
                    padding: const EdgeInsets.fromLTRB(0, 8, 0, 0),
                  ),
                  const SizedBox(height: 20),
                  const ScreenHeading(
                    title: 'Choose your track.',
                    subtitle: 'Preview a track. Select your next drop.',
                  ),
                  const SizedBox(height: 18),
                  AppTextField(
                    controller: _controller,
                    hint: 'Search a track…',
                    prefixIcon: Icons.search,
                    height: DropTokens.searchHeight,
                    onChanged: (v) => setState(() {
                      _query = v;
                      _selected = null;
                    }),
                    suffix: IconButton(
                      icon: const Icon(Icons.clear,
                          size: 18, color: DropTokens.muted),
                      onPressed: () {
                        _controller.clear();
                        setState(() {
                          _query = '';
                          _selected = null;
                        });
                      },
                    ),
                  ),
                  const SizedBox(height: 16),
                  SizedBox(
                    height: 36,
                    child: ListView.separated(
                      scrollDirection: Axis.horizontal,
                      itemCount: _genres.length,
                      separatorBuilder: (_, __) => const SizedBox(width: 6),
                      itemBuilder: (_, i) => FilterPill(
                        label: _genres[i],
                        selected: _genre == _genres[i],
                        onTap: () => setState(() {
                          _genre = _genres[i];
                          _selected = null;
                        }),
                      ),
                    ),
                  ),
                  const SizedBox(height: 22),
                  SectionLabel('${results.length} '
                      '${results.length == 1 ? 'TRACK' : 'TRACKS'} · SELECT ONE',
                      style: DropType.labelTight),
                  const SizedBox(height: 12),
                  if (results.isEmpty)
                    Text('Nothing found — try another search.',
                        style: DropType.bodySm.copyWith(
                          color: DropTokens.muted,
                        ))
                  else
                    for (var i = 0; i < results.length; i++)
                      _ResultRow(
                        track: results[i],
                        selected: _selected == i,
                        playing: _selected == i,
                        isLast: i == results.length - 1,
                        onTap: () => setState(() => _selected = i),
                      ),
                ],
              ),
            ),
            if (selected != null)
              _SelectedFooter(
                track: selected,
                priceLabel: widget.session.priceLabel,
                onContinue: _continue,
              ),
          ],
        ),
      ),
    );
  }

  String _sessionLine() {
    final dj = widget.session.djDisplayName;
    const floor = 'Main floor';
    return dj == null ? '$floor · Live now' : '$floor · $dj · Live now';
  }
}

class _DemoTrack {
  const _DemoTrack(this.id, this.title, this.subtitle, {required this.genre});

  final String id;
  final String title;
  final String subtitle;
  final String genre;
}

/// Строка результата с контролом выбора и (у выбранной) превью-контролами.
class _ResultRow extends StatelessWidget {
  const _ResultRow({
    required this.track,
    required this.selected,
    required this.playing,
    required this.isLast,
    required this.onTap,
  });

  final _DemoTrack track;
  final bool selected;
  final bool playing;
  final bool isLast;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return TrackRow(
      title: track.title,
      subtitle: track.subtitle,
      divider: !isLast,
      below: selected
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    const Icon(Icons.play_arrow,
                        size: 15, color: DropTokens.accent),
                    const SizedBox(width: 6),
                    Text('Playing preview',
                        style: DropType.metaSm.copyWith(
                          color: DropTokens.accent,
                          fontWeight: FontWeight.w500,
                        )),
                    const Spacer(),
                    const Text('0:09 / 0:30', style: DropType.metaSm),
                  ],
                ),
                const SizedBox(height: 10),
                Container(
                  height: 4,
                  decoration: BoxDecoration(
                    color: const Color(0xFF343B36),
                    borderRadius: BorderRadius.circular(2),
                  ),
                  child: FractionallySizedBox(
                    alignment: Alignment.centerLeft,
                    widthFactor: 0.3,
                    child: Container(
                      decoration: BoxDecoration(
                        color: DropTokens.accent,
                        borderRadius: BorderRadius.circular(2),
                      ),
                    ),
                  ),
                ),
              ],
            )
          : null,
      trailing: _SelectionControl(selected: selected, onTap: onTap),
      onTap: onTap,
    );
  }
}

/// Контрол выбора трека: 44×44 радиус 12, внутри мятный круг 24 с галочкой.
class _SelectionControl extends StatelessWidget {
  const _SelectionControl({required this.selected, required this.onTap});

  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      selected: selected,
      label: selected ? 'Selected track' : 'Select track',
      child: GestureDetector(
        onTap: onTap,
        behavior: HitTestBehavior.opaque,
        child: Container(
          width: 44,
          height: 44,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(DropTokens.rMd),
            border: Border.all(
              color: selected ? DropTokens.text : DropTokens.ghostStroke,
            ),
          ),
          alignment: Alignment.center,
          child: Container(
            width: 24,
            height: 24,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: selected ? DropTokens.accent : Colors.transparent,
              border: Border.all(
                color: selected ? DropTokens.accent : DropTokens.inactive,
                width: 1.5,
              ),
            ),
            child: selected
                ? const Icon(Icons.check, size: 15, color: DropTokens.ink)
                : null,
          ),
        ),
      ),
    );
  }
}

/// Закреплённый низ экрана: «SELECTED TRACK» + цена + переход к оплате.
class _SelectedFooter extends StatelessWidget {
  const _SelectedFooter({
    required this.track,
    required this.priceLabel,
    required this.onContinue,
  });

  final _DemoTrack track;
  final String priceLabel;
  final VoidCallback onContinue;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: const BoxDecoration(
        color: DropTokens.bg,
        border: Border(top: BorderSide(color: DropTokens.divider)),
      ),
      padding: const EdgeInsets.fromLTRB(20, 16, 20, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const SectionLabel('SELECTED TRACK', style: DropType.labelTight),
          const SizedBox(height: 12),
          Row(
            children: [
              const Artwork(url: null),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(track.title, style: DropType.trackTitle),
                    const SizedBox(height: 5),
                    Text(track.artistLine, style: DropType.metaSm),
                  ],
                ),
              ),
              Text(priceLabel,
                  style: DropType.bodySm.copyWith(
                    fontSize: 13,
                    color: DropTokens.muted,
                  )),
            ],
          ),
          const SizedBox(height: 14),
          AppButton(label: 'Continue to payment', onPressed: onContinue),
        ],
      ),
    );
  }
}

extension on _DemoTrack {
  /// Первая часть подписи — исполнитель (до разделителя «·»).
  String get artistLine => subtitle.split(' · ').first;
}
