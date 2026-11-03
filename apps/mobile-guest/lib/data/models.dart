/// Модели данных гостевого приложения DROP IT.
///
/// Поля соответствуют макету (pen.dev, фреймы Guest 01–15) и контрактам
/// бэкенда (docs/openapi.yaml): сессия после join, трек каталога, запрос
/// гостя, способ оплаты.
library;

/// Комната клуба (Explore, Room, шапки экранов).
class Room {
  const Room({
    required this.id,
    required this.name,
    required this.session,
    required this.dj,
    required this.genre,
    required this.distance,
    required this.image,
    this.liveCount = 0,
    this.openNow = true,
    this.nowPlaying,
    this.featured = false,
  });

  final String id;
  final String name;
  final String session;
  final String dj;
  final String genre;
  final String distance;
  final String image;
  final int liveCount;
  final bool openNow;
  final String? nowPlaying;
  final bool featured;
}

/// Трек каталога.
class Track {
  const Track({
    required this.id,
    required this.title,
    required this.artist,
    required this.genre,
    required this.art,
    required this.priceLabel,
  });

  final String id;
  final String title;
  final String artist;
  final String genre;
  final String art;
  final String priceLabel;
}

/// Статус запроса гостя (макет Guest 06–06d, 13).
enum RequestStatus { queue, awaiting, played, declined }

extension RequestStatusLabels on RequestStatus {
  String get label => switch (this) {
        RequestStatus.queue => 'In queue · #3',
        RequestStatus.awaiting => 'Awaiting DJ',
        RequestStatus.played => 'Played · 00:24',
        RequestStatus.declined => 'Declined',
      };
}

/// Запрс трека из «My requests».
class GuestRequest {
  const GuestRequest({
    required this.id,
    required this.track,
    required this.room,
    required this.status,
    required this.timeLabel,
    required this.priceLabel,
    required this.paymentLabel,
    this.declineReason,
  });

  final String id;
  final Track track;
  final String room;
  final RequestStatus status;
  final String timeLabel;
  final String priceLabel;
  final String paymentLabel;
  final String? declineReason;
}

/// Сессия после join по QR (шапки Search/Payment/Request sent).
class JoinSession {
  const JoinSession({
    required this.room,
    required this.dj,
    required this.priceLabel,
    this.testPayments = false,
  });

  final Room room;
  final String dj;
  final String priceLabel;
  final bool testPayments;
}
