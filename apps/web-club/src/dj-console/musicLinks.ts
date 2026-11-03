/**
 * External music links — how a DJ actually finds a requested track.
 *
 * The backend exposes `title`, `artist`, `isrc` and a 30-second `preview_url`
 * (`track_cache`), and nothing else: no Spotify/Apple/Deezer track ids were
 * ever ingested. So every link here opens that service's **search results** for
 * "artist title" — a search, never a claimed catalog match. The copy says so
 * (`dj.findInService` = "Find {track} in {service}"), and the surface labels
 * the group `dj.musicSources` ("Open in"), not "Listen on" or "In catalog".
 *
 * Pure helpers on purpose: no React, no DOM, no i18n. Service names are
 * product names and are not translated; everything a reader sees around them
 * comes from the message catalog.
 */

export type MusicSourceId = 'spotify' | 'apple' | 'deezer' | 'youtube';

export interface MusicSource {
  readonly id: MusicSourceId;
  /** The service's own name. Never translated — it is a brand, not a word. */
  readonly name: string;
  /** Search entry point for this service. */
  readonly searchUrl: (query: string) => string;
}

/**
 * The four services a working DJ checks, in the order the console shows them.
 * Each URL is the service's own public search route: a query with no ids in it,
 * so a result page can list several pressings and the DJ judges by title.
 */
export const MUSIC_SOURCES: readonly MusicSource[] = [
  {
    id: 'spotify',
    name: 'Spotify',
    searchUrl: (query) => `https://open.spotify.com/search/${encodeURIComponent(query)}`,
  },
  {
    id: 'apple',
    name: 'Apple Music',
    searchUrl: (query) => `https://music.apple.com/search?term=${encodeURIComponent(query)}`,
  },
  {
    id: 'deezer',
    name: 'Deezer',
    searchUrl: (query) => `https://www.deezer.com/search/${encodeURIComponent(query)}`,
  },
  {
    id: 'youtube',
    name: 'YouTube',
    searchUrl: (query) => `https://www.youtube.com/results?search_query=${encodeURIComponent(query)}`,
  },
];

export interface MusicLink {
  readonly id: MusicSourceId;
  readonly name: string;
  readonly url: string;
}

/**
 * The query every service is searched with. Artist first is what a search
 * engine expects; whitespace from a sloppy API row is collapsed so the query
 * never contains a double space (which some services percent-encode literally).
 */
export function trackQuery(artist: string, title: string): string {
  return `${artist} ${title}`.replace(/\s+/g, ' ').trim();
}

/**
 * What "copy track" puts on the clipboard: `Artist – Title` with an en dash,
 * the shape DJ software and chat windows both accept. Not a URL, not the
 * request id — the DJ pastes this into a player or a set list.
 */
export function trackClipboardText(artist: string, title: string): string {
  return `${artist.trim()} – ${title.trim()}`;
}

/** Every service, ready to render as a link. */
export function musicLinksFor(artist: string, title: string): MusicLink[] {
  const query = trackQuery(artist, title);
  return MUSIC_SOURCES.map((source) => ({ id: source.id, name: source.name, url: source.searchUrl(query) }));
}
