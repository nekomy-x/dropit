import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../main.dart';

/// Поиск трека. B5: реальные интеграции каталога (iTunes клиентский
/// поиск + серверная whitelist-валидация) заблокированы проверкой
/// лицензий — здесь локальная заглушка-список + поле ввода.
/// Контракт сервера (F3-валидация) готов: треки идут через
/// track_cache, canonical-ключи, счётчики «той же песни».
class SearchScreen extends StatefulWidget {
  const SearchScreen({
    super.key,
    required this.session,
    required this.onRequestTrack,
  });

  final JoinResponse session;
  final void Function(CatalogTrack) onRequestTrack;

  @override
  State<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends State<SearchScreen> {
  final _controller = TextEditingController();
  List<CatalogTrack> _results = [];

  // Демо-данные (ЗАГЛУШКА, правило 5): имитация кэш-хита.
  static const _demoTracks = [
    (
      title: 'Vivir Mi Vida',
      artist: 'Marc Anthony',
      isrc: 'USSM19902117'
    ),
    (
      title: 'Bailando',
      artist: 'Enrique Iglesias',
      isrc: 'USSM11400556'
    ),
    ('Despacito', 'Luis Fonsi', 'USSM11700114'),
    ('Taki Taki', 'DJ Snake', 'USSM11804228'),
    ('Gasolina', 'Daddy Yankee', 'USSM10402974'),
  ];

  void _search(String query) {
    final q = query.trim().toLowerCase();
    setState(() {
      _results = _demoTracks
          .where((t) =>
              t.title.toLowerCase().contains(q) ||
              t.artist.toLowerCase().contains(q))
          .map((t) => CatalogTrack(
                // Демо-id: реальный track_cache.id придёт с сервера (B5).
                id: 'demo_${t.title.hashCode}',
                title: t.title,
                artist: t.artist,
                isrc: t.isrc,
              ))
          .toList();
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Find your song')),
      body: SafeArea(
        child: Column(
          children: [
            Padding(
              padding: const EdgeInsets.all(16),
              child: TextField(
                controller: _controller,
                autofocus: true,
                decoration: InputDecoration(
                  hintText: 'Song or artist',
                  prefixIcon: const Icon(Icons.search),
                  suffixIcon: IconButton(
                    icon: const Icon(Icons.clear),
                    onPressed: () {
                      _controller.clear();
                      setState(() => _results = []);
                    },
                  ),
                ),
                onSubmitted: _search,
              ),
            ),
            Expanded(
              child: _results.isEmpty
                  ? Center(
                      child: Text(
                        'Type to search\ncatalog integration: pending licensing (B5)',
                        textAlign: TextAlign.center,
                        style: TextStyle(color: DropItTheme.grey),
                      ),
                    )
                  : ListView.builder(
                      padding: const EdgeInsets.symmetric(horizontal: 16),
                      itemCount: _results.length,
                      itemBuilder: (_, i) {
                        final track = _results[i];
                        return Card(
                          child: ListTile(
                            leading: Container(
                              width: 48,
                              height: 48,
                              decoration: BoxDecoration(
                                color: DropItTheme.surface,
                                borderRadius: BorderRadius.circular(8),
                              ),
                              child: const Icon(Icons.music_note,
                                  color: DropItTheme.grey),
                            ),
                            title: Text(track.title),
                            subtitle: Text(track.artist),
                            trailing: const Icon(Icons.chevron_right,
                                color: DropItTheme.neon),
                            onTap: () => widget.onRequestTrack(track),
                          ),
                        );
                      },
                    ),
            ),
          ],
        ),
      ),
    );
  }
}
