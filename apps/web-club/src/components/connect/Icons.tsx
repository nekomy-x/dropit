/**
 * Compact inline icon set for the Connect flow.
 *
 * Stroke-based, 24×24 viewBox, `currentColor` — the same Lucide geometry the
 * .pen design uses, inlined so the public pages ship no icon dependency.
 */
import type { SVGProps } from 'react';

const PATHS = {
  headphones:
    'M4 14v-2a8 8 0 0 1 16 0v2M4 14a2 2 0 0 1 2-2h1v7H6a2 2 0 0 1-2-2v-3Zm16 0a2 2 0 0 0-2-2h-1v7h1a2 2 0 0 0 2-2v-3Z',
  arrowRight: 'M5 12h14M13 6l6 6-6 6',
  arrowUpRight: 'M7 17 17 7M9 7h8v8',
  check: 'M4 12.5l5 5L20 6.5',
  plus: 'M12 5v14M5 12h14',
  minus: 'M5 12h14',
  x: 'M6 6l12 12M18 6 6 18',
  qr: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h2v2h-2zM18 18h2v2h-2zM14 20h2M20 14h-2',
  pin: 'M12 21s7-6.1 7-11a7 7 0 1 0-14 0c0 4.9 7 11 7 11Z M12 12.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z',
  mail: 'M3 6h18v12H3zM3 7l9 6 9-6',
  phone: 'M6 3h3l2 5-2.5 1.5a12 12 0 0 0 6 6L16 13l5 2v3a2 2 0 0 1-2 2A16 16 0 0 1 4 5a2 2 0 0 1 2-2Z',
  globe: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18ZM3 12h18M12 3c2.5 2.4 3.8 5.6 3.8 9S14.5 18.6 12 21c-2.5-2.4-3.8-5.6-3.8-9S9.5 5.4 12 3Z',
  users:
    'M16 20v-1.5a4 4 0 0 0-4-4H7a4 4 0 0 0-4 4V20M9.5 10.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7ZM21 20v-1.5a4 4 0 0 0-3-3.87M16.5 3.8a3.5 3.5 0 0 1 0 6.4',
  layers: 'M12 3 3 8l9 5 9-5-9-5ZM3 13l9 5 9-5M3 17.5l9 5 9-5',
  music: 'M9 18V6l10-2v12M9 18a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Zm10-2a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Z',
  music2: 'M9 18V5l12-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0Zm12-2a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z',
  coins:
    'M8 2a6 6 0 1 0 0 12A6 6 0 0 0 8 2Zm10.09 8.37A6 6 0 1 1 10.34 18M7 6h1v4M16.71 13.88l.7.71-2.82 2.82',
  scanQrCode:
    'M3 7V5a2 2 0 0 1 2-2h2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2M8 8h.01M12 12h.01M16 8h.01M12 16h.01',
  compass:
    'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm4.2-13.2-2.1 6.3-6.3 2.1 2.1-6.3 6.3-2.1Z',
  listMusic: 'M21 15V6M18.5 18a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5ZM12 12H3M16 6H3M12 18H3',
  userRound: 'M12 13a4 4 0 1 0 0-8 4 4 0 0 0 0 8ZM20 21a8 8 0 0 0-16 0',
  chart: 'M4 20V10M10 20V4M16 20v-7M22 20H2',
  wallet: 'M3 7h16a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Zm0 0 3-3h11M17 13h.01',
  shield: 'M12 3l8 3v6c0 4.5-3.3 7.8-8 9-4.7-1.2-8-4.5-8-9V6l8-3Z',
  spark: 'M12 3v4M12 17v4M3 12h4M17 12h4M6.3 6.3l2.8 2.8M14.9 14.9l2.8 2.8M17.7 6.3l-2.8 2.8M9.1 14.9l-2.8 2.8',
  info: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18ZM12 11v6M12 7.6h.01',
  chevronDown: 'M6 9l6 6 6-6',
  chevronRight: 'M9 6l6 6-6 6',
  clock: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18ZM12 7.5V12l3 2',
  printer: 'M7 8V3h10v5M7 18H5a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2h-2M7 14h10v7H7z',
  copy: 'M9 9h10v12H9zM5 15H3V3h12v2',
  pause: 'M9 5v14M15 5v14',
  play: 'M8 5l11 7-11 7V5Z',
  live: 'M12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM8.5 15.5a5 5 0 0 1 0-7M15.5 8.5a5 5 0 0 1 0 7M5.6 18.4a9 9 0 0 1 0-12.8M18.4 5.6a9 9 0 0 1 0 12.8',
  calendar: 'M4 6h16v14H4zM8 3v4M16 3v4M4 11h16',
  store: 'M4 9h16v11H4zM3 9l1.5-5h15L21 9M9 20v-6h6v6',
  door: 'M5 21V3h11v18M16 21h3M12 12h.01',
  doc: 'M6 3h8l4 4v14H6zM14 3v4h4',
  // Used by the owner-panel preview: the panel's search field and its theme switch.
  search: 'M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16ZM21 21l-4.3-4.3',
  moon: 'M21 12.8A8.5 8.5 0 1 1 11.2 3a6.6 6.6 0 0 0 9.8 9.8Z',
} as const;

export type IconName = keyof typeof PATHS;

interface IconProps extends Omit<SVGProps<SVGSVGElement>, 'name'> {
  name: IconName;
  size?: number;
  strokeWidth?: number;
}

export default function Icon({ name, size = 16, strokeWidth = 1.6, ...rest }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...rest}
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
