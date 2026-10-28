/// Демо-данные гостевого приложения — все названия, треки, комнаты,
/// запросы, FAQ взяты 1:1 из макета pen.dev (Guest 01–15).
///
/// Используются при отсутствии бэкенда или как заглушка перед реальными
/// данными — гарантируют работу APK в любом окружении.
library;

import 'models.dart';

abstract class DemoData {
  // ──────────────────────────────────────────────────────────────────────
  // Комнаты (Explore + Search/Payment/Request sent joined room)
  // ──────────────────────────────────────────────────────────────────────

  static const katerBlau = Room(
    id: 'kater-blau',
    name: 'Kater Blau',
    session: 'Main floor',
    dj: 'DJ Markus',
    genre: 'House',
    distance: '200 m',
    image: 'assets/images/venue_kater_blau.jpg',
    liveCount: 12,
    openNow: true,
    featured: false,
  );

  static const katerBlauGarten = Room(
    id: 'kater-blau-garten',
    name: 'Kater Blau',
    session: 'Garten',
    dj: 'DJ Monark',
    genre: 'Techno',
    distance: '200 m',
    image: 'assets/images/room_featured.jpg',
    liveCount: 24,
    openNow: true,
    nowPlaying: 'Move',
    featured: true,
  );

  static const rooms = [
    katerBlauGarten,
    Room(
      id: 'tresor',
      name: 'Tresor',
      session: 'Globus Floor',
      dj: 'DJ Linnea',
      genre: 'House',
      distance: '450 m',
      image: 'assets/images/venue_kater_blau.jpg',
      liveCount: 8,
      openNow: true,
      nowPlaying: 'Show Me Love',
    ),
    Room(
      id: 'renate',
      name: 'Salon zur Wilden Renate',
      session: 'Main',
      dj: 'Kollektiv Ost',
      genre: 'Disco',
      distance: '900 m',
      image: 'assets/images/venue_kater_blau.jpg',
      liveCount: 5,
      openNow: true,
      nowPlaying: 'I Feel Love',
    ),
    Room(
      id: 'sisyphos',
      name: 'Sisyphos',
      session: 'Main',
      dj: 'Nia Waves',
      genre: 'Afro house',
      distance: '1.4 km',
      image: 'assets/images/venue_kater_blau.jpg',
      liveCount: 11,
      openNow: true,
      nowPlaying: 'Jerusalema',
    ),
    Room(
      id: 'berghain',
      name: 'Berghain',
      session: 'Panorama',
      dj: 'Paul Kalkbrenner',
      genre: 'Trance',
      distance: '1.9 km',
      image: 'assets/images/venue_kater_blau.jpg',
      liveCount: 18,
      openNow: true,
      nowPlaying: 'Adagio for Strings',
    ),
  ];

  // ──────────────────────────────────────────────────────────────────────
  // Треки каталога (Search, Recommendations)
  // ──────────────────────────────────────────────────────────────────────

  static const trackMove = Track(
    id: 'move',
    title: 'Move',
    artist: 'Adam Port, Stryv',
    genre: 'Afro house',
    art: 'assets/images/art_move.jpg',
    priceLabel: '3,00 €',
  );

  static const trackOneMoreTime = Track(
    id: 'one-more-time',
    title: 'One More Time',
    artist: 'Daft Punk',
    genre: 'Dance classics',
    art: 'assets/images/art_one_more_time.jpg',
    priceLabel: '3,00 €',
  );

  /// Каталог экрана Search — ровно три трека из макета.
  static const tracks = [
    trackMove,
    Track(
      id: 'move-body',
      title: 'Move Your Body',
      artist: 'Marshall Jefferson',
      genre: 'House',
      art: 'assets/images/art_move.jpg',
      priceLabel: '3,00 €',
    ),
    Track(
      id: 'move-extended',
      title: 'Move (Extended Mix)',
      artist: 'Adam Port, Stryv',
      genre: 'Afro house',
      art: 'assets/images/art_move.jpg',
      priceLabel: '3,00 €',
    ),
  ];

  // ──────────────────────────────────────────────────────────────────────
  // Мои запросы (My requests, Guest 06)
  // ──────────────────────────────────────────────────────────────────────

  static final myRequests = [
    const GuestRequest(
      id: '#DI-0284',
      track: trackMove,
      room: 'Kater Blau',
      status: RequestStatus.queue,
      timeLabel: '00:42',
      priceLabel: '3,00 €',
      paymentLabel: 'Paid',
    ),
    const GuestRequest(
      id: '#DI-0283',
      track: trackOneMoreTime,
      room: 'Kater Blau',
      status: RequestStatus.awaiting,
      timeLabel: '00:38',
      priceLabel: '3,00 €',
      paymentLabel: 'Paid',
    ),
    const GuestRequest(
      id: '#DI-0282',
      track: trackMove,
      room: 'Kater Blau',
      status: RequestStatus.played,
      timeLabel: '00:12',
      priceLabel: '3,00 €',
      paymentLabel: 'Paid',
    ),
    const GuestRequest(
      id: '#DI-0281',
      track: trackOneMoreTime,
      room: 'Kater Blau',
      status: RequestStatus.declined,
      timeLabel: '23:56',
      priceLabel: '3,00 €',
      paymentLabel: 'Refunded',
      declineReason:
          '"This track doesn\'t fit tonight\'s set.\nTry something with an Afro house vibe."',
    ),
    const GuestRequest(
      id: '#DI-0280',
      track: trackMove,
      room: 'Kater Blau',
      status: RequestStatus.played,
      timeLabel: '23:21',
      priceLabel: '3,00 €',
      paymentLabel: 'Paid',
    ),
  ];

  // ──────────────────────────────────────────────────────────────────────
  // FAQ (Help & support, Guest 14)
  // ──────────────────────────────────────────────────────────────────────

  static const faq = [
    (
      question: 'How do track requests work?',
      answer:
          'Once you’ve joined a room, you can request any track in the catalog. '
              'The DJ sees your request in their queue, votes and mood included, '
              'and decides what makes it to the decks.',
    ),
    (question: 'When am I charged?', answer: ''),
    (question: 'How do refunds work?', answer: ''),
    (question: 'Why was my request declined?', answer: ''),
  ];

  // ──────────────────────────────────────────────────────────────────────
  // Профиль гостя (Identity, Guest 15)
  // ──────────────────────────────────────────────────────────────────────

  static const memberEmail = 'mara@nightmail.de';
  static const memberCode = 'MK-4821';
  static const homeCity = 'Berlin, DE';
  static const favoriteGenres = ['House', 'Afro house', 'Techno', 'Disco'];

  // Статистика профиля (Guest 07)
  static const statPlayed = 18;
  static const statRooms = 6;
  static const statRequests = 24;

  // Город для шапок экранов
  static const currentCity = 'Berlin';
}
