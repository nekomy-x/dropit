/// Accept the old raw token and the camera-readable web/custom-scheme links.
/// The link's host is never used as an API address.
String? guestQrToken(String value) {
  final input = value.trim();
  final pattern = RegExp(r'^[a-fA-F0-9]{64,}$');
  if (pattern.hasMatch(input)) return input;
  final uri = Uri.tryParse(input);
  if (uri == null ||
      !['http', 'https', 'dropit'].contains(uri.scheme) ||
      !(uri.path == '/join' || uri.path == '/room' ||
          (uri.scheme == 'dropit' && uri.host == 'join'))) {
    return null;
  }
  final token = uri.queryParameters['token'];
  return token != null && pattern.hasMatch(token) ? token : null;
}
