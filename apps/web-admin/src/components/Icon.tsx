/**
 * Icon — the single icon surface for the DROP IT panels.
 *
 * Icons are LOCAL: the map below resolves the existing `lucide:<kebab-case>`
 * names to bundled lucide-react components. No icon CDN request is made on
 * boot; a nightclub screen must not depend on a third-party host.
 *
 * The public API is unchanged: `<Icon name="lucide:..." />` keeps working for
 * every existing call site. Unknown names fall back to a neutral dot so a typo
 * degrades visibly instead of crashing the screen.
 *
 * Aliases are semantic choices, not accidents:
 *   lucide:history        -> Clock3
 *   lucide:music-2        -> ListMusic
 *   lucide:check-circle-2 -> CircleCheckBig
 */
import type { ComponentType, CSSProperties, SVGProps } from 'react';
import {
  Activity,
  ArrowDownToLine,
  ArrowRight,
  ArrowUpRight,
  BadgeEuro,
  Banknote,
  Bell,
  Building2,
  Calendar,
  CalendarPlus,
  ChartColumn,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  ChevronsUpDown,
  CircleAlert,
  CircleCheck,
  CircleCheckBig,
  CircleDashed,
  Clock,
  Clock3,
  Command,
  Copy,
  CreditCard,
  Disc3,
  Download,
  Ellipsis,
  ExternalLink,
  Headphones,
  Hourglass,
  Info,
  KeyRound,
  LayoutDashboard,
  ListChecks,
  ListMusic,
  LoaderCircle,
  LogOut,
  Menu,
  Minus,
  Moon,
  Music,
  Pause,
  Play,
  PlugZap,
  Plus,
  Power,
  QrCode,
  RadioTower,
  RefreshCw,
  RotateCcw,
  ScanSearch,
  Search,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  Sun,
  Ticket,
  ToggleRight,
  TrendingDown,
  TrendingUp,
  TriangleAlert,
  Undo2,
  UserRoundPlus,
  UserRoundSearch,
  Users,
  UsersRound,
  WalletCards,
  X,
} from 'lucide-react';

type LucideComponent = ComponentType<SVGProps<SVGSVGElement> & { size?: number | string; strokeWidth?: number | string }>;

const ICONS: Record<string, LucideComponent> = {
  activity: Activity,
  'arrow-down-to-line': ArrowDownToLine,
  'arrow-right': ArrowRight,
  'arrow-up-right': ArrowUpRight,
  'badge-euro': BadgeEuro,
  banknote: Banknote,
  bell: Bell,
  'building-2': Building2,
  calendar: Calendar,
  'calendar-plus': CalendarPlus,
  'chart-column': ChartColumn,
  check: Check,
  'check-circle-2': CircleCheckBig,
  'chevron-down': ChevronDown,
  'chevron-left': ChevronLeft,
  'chevron-right': ChevronRight,
  'chevron-up': ChevronUp,
  'chevrons-up-down': ChevronsUpDown,
  'circle-alert': CircleAlert,
  'circle-check': CircleCheck,
  'circle-dashed': CircleDashed,
  clock: Clock,
  'clock-3': Clock3,
  command: Command,
  copy: Copy,
  'credit-card': CreditCard,
  'disc-3': Disc3,
  download: Download,
  ellipsis: Ellipsis,
  'external-link': ExternalLink,
  headphones: Headphones,
  history: Clock3,
  hourglass: Hourglass,
  info: Info,
  'key-round': KeyRound,
  'layout-dashboard': LayoutDashboard,
  'list-checks': ListChecks,
  'list-music': ListMusic,
  'loader-circle': LoaderCircle,
  'log-out': LogOut,
  menu: Menu,
  minus: Minus,
  moon: Moon,
  music: Music,
  'music-2': ListMusic,
  pause: Pause,
  play: Play,
  'plug-zap': PlugZap,
  plus: Plus,
  power: Power,
  'qr-code': QrCode,
  'radio-tower': RadioTower,
  'refresh-cw': RefreshCw,
  'rotate-ccw': RotateCcw,
  'scan-search': ScanSearch,
  search: Search,
  'settings-2': Settings2,
  'shield-check': ShieldCheck,
  'sliders-horizontal': SlidersHorizontal,
  sun: Sun,
  ticket: Ticket,
  'toggle-right': ToggleRight,
  'trending-down': TrendingDown,
  'trending-up': TrendingUp,
  'triangle-alert': TriangleAlert,
  'undo-2': Undo2,
  'user-round-plus': UserRoundPlus,
  'user-round-search': UserRoundSearch,
  users: Users,
  'users-round': UsersRound,
  'wallet-cards': WalletCards,
  x: X,
};

interface IconProps {
  name: `lucide:${string}`;
  size?: number;
  className?: string;
  /** Decorative by default; pass a label to expose the icon to assistive tech. */
  label?: string;
}

export default function Icon({ name, size = 16, className, label }: IconProps) {
  const key = name.slice('lucide:'.length);
  const Glyph = ICONS[key];
  const style: CSSProperties = { display: 'inline-flex', flex: '0 0 auto', width: size, height: size };

  if (!Glyph) {
    return (
      <span className={className} style={style} role={label ? 'img' : undefined} aria-label={label} aria-hidden={label ? undefined : true}>
        <svg viewBox="0 0 16 16" width={size} height={size} aria-hidden="true">
          <circle cx="8" cy="8" r="2" fill="currentColor" />
        </svg>
      </span>
    );
  }

  return (
    <span className={className} style={style} role={label ? 'img' : undefined} aria-label={label} aria-hidden={label ? undefined : true}>
      <Glyph size={size} strokeWidth={1.75} aria-hidden="true" focusable="false" />
    </span>
  );
}
