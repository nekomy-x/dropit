/** A camera-readable URL, shared by the session, board and QR studio. */
export function guestJoinUrl(token: string, origin = window.location.origin): string {
  const url = new URL('/join', origin);
  url.searchParams.set('token', token);
  return url.href;
}

export function qrToken(value: string): string | null {
  const input = value.trim();
  if (/^[a-f0-9]{64,}$/i.test(input)) return input;
  try {
    const url = new URL(input);
    if (!['https:', 'http:', 'dropit:'].includes(url.protocol)) return null;
    if (!(url.pathname === '/join' || url.pathname === '/room' || (url.protocol === 'dropit:' && url.hostname === 'join'))) return null;
    const token = url.searchParams.get('token') ?? '';
    return /^[a-f0-9]{64,}$/i.test(token) ? token : null;
  } catch { return null; }
}

export function guestAppUrl(token: string): string {
  const deepLink = `dropit://join?token=${encodeURIComponent(token)}`;
  if (!/Android/i.test(navigator.userAgent)) return deepLink;
  const fallback = new URL(guestJoinUrl(token));
  fallback.searchParams.set('app', 'web');
  const configuredPackage = import.meta.env.VITE_GUEST_ANDROID_PACKAGE;
  const packageName = configuredPackage && /^[A-Za-z]\w*(?:\.[A-Za-z]\w*)+$/.test(configuredPackage) ? configuredPackage : 'com.example.dropit_guest';
  return `intent://join?token=${encodeURIComponent(token)}#Intent;scheme=dropit;package=${packageName};S.browser_fallback_url=${encodeURIComponent(fallback.href)};end`;
}
