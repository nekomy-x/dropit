import 'package:flutter_test/flutter_test.dart';
import 'package:dropit_guest/api/guest_link.dart';

void main() {
  final token = 'ab' * 32;
  test('old QR, HTTPS QR and app handoff resolve the same room', () {
    for (final link in [token, 'https://dropit.club/join?token=$token',
      'http://localhost:5173/room?token=$token', 'dropit://join?token=$token']) {
      expect(guestQrToken(link), token);
    }
  });
  test('Stripe return and unrelated or malformed links are ignored', () {
    for (final link in ['dropit://stripe-redirect', 'https://dropit.club/login?token=$token',
      'javascript://join?token=$token', 'dropit://join?token=short', 'z' * 64]) {
      expect(guestQrToken(link), isNull);
    }
  });
}
