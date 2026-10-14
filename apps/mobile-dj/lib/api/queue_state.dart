import 'api_client.dart';
import 'ws_client.dart';

/// Reducer live-очереди DJ: снапшот API + инкрементальные WS-события.
///
/// Правила (контракт gateway-ws):
///   * request.created: вставка карточки сверху (sort=new) или
///     перестановка по ×N (sort=hot);
///   * request.accepted/rejected/played: обновление статуса карточки;
///   * no-op-дубли НЕ приходят (сервер не пушит);
///   * session.paused/resumed: баннер.
class QueueState {
  QueueState({required this.items, this.paused = false});

  final List<QueueItem> items;
  final bool paused;

  QueueState copyWith({List<QueueItem>? items, bool? paused}) => QueueState(
        items: items ?? this.items,
        paused: paused ?? this.paused,
      );

  /// Применяет событие → новое состояние (иммутабельно).
  QueueState apply(WsEvent event, String sort) {
    switch (event.kind) {
      case 'request.created':
        final item = QueueItem(
          id: event.requestId ?? '',
          status: 'pending',
          createdAt: DateTime.now(),
          title: event.title ?? '…',
          artist: '',
          sameTrackCount: event.sameTrackCount ?? 1,
        );
        final next = [item, ..._bumpSameTrack(items, item.id, event.sameTrackCount)];
        return copyWith(items: _sorted(next, sort));
      case 'request.accepted':
      case 'request.rejected':
      case 'request.played':
        final status = switch (event.kind) {
          'request.accepted' => 'accepted',
          'request.rejected' => 'rejected',
          _ => 'played',
        };
        return copyWith(
          items: items
              .map((i) => i.id == event.requestId
                  ? QueueItem(
                      id: i.id,
                      status: status,
                      createdAt: i.createdAt,
                      title: i.title,
                      artist: i.artist,
                      sameTrackCount: i.sameTrackCount,
                    )
                  : i)
              .toList(),
        );
      case 'session.paused':
        return copyWith(paused: true);
      case 'session.resumed':
        return copyWith(paused: false);
      default:
        return this;
    }
  }

  /// ×N: счётчик приходит с сервера в каждом created (canonical-ключ
  /// — серверная логика; клиент просто показывает значение).
  List<QueueItem> _bumpSameTrack(List<QueueItem> items, String newId, int? count) {
    if (count == null || count <= 1) return items;
    return items;
  }

  List<QueueItem> _sorted(List<QueueItem> items, String sort) {
    if (sort == 'hot') {
      final list = [...items];
      list.sort((a, b) => b.sameTrackCount.compareTo(a.sameTrackCount));
      return list;
    }
    return items; // новые сверху (вставка в начало).
  }
}
