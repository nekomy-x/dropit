# Hub + guest-app download — build contract

Status: implementation contract for the public front door at `/` and the guest-app
download page at `/download` (`apps/web-club`). Read `AGENTS.md`, `PRODUCT.md` and
`DESIGN.md` first; this file fixes the parts that two implementers must not invent
twice (i18n keys, routes, assets, tokens).

## Why this surface exists

The domain root was a redirect to the club landing (`/connect`). A guest who typed
the domain, or scanned a generic DROP IT code, landed on a page written for club
owners. The root is now a **two-sided gate**: "Я гость" and "Я клуб". The club side
goes to the existing club landing; the guest side goes to `/download`, which
detects the device and offers the right store, with a manual override.

The design frame is `DROP IT — Hub · Choose side` in
`docs/ui/reference/dropit-design.pen`. Its structure (nav → split → footer) is the
constraint. The two halves are **photographic**, per the product owner's request:
one photo per side, and when a side is hovered/focused the other photo turns
greyscale — "you party" vs "you serve the club".

## Routes

| Path | Page | Notes |
| --- | --- | --- |
| `/` | `HubPage` | production build only; demo build keeps `/` → `/dj` and exposes `/hub` |
| `/hub` | → `/` | alias so the demo build can review the page |
| `/download` | `DownloadPage` | both builds |
| `/connect` | `StartPage` | unchanged club landing |

Routes live in `apps/web-club/src/App.tsx` and are owned by the Lead; do not edit
that file.

## Assets (do not add new photography)

| Side | File | Crop |
| --- | --- | --- |
| Guest | `/hero-club.jpg` (1920×1280) | crowd from the floor, warm red |
| Club | `/phone-hero.jpg` (1080×1620) | crowd from behind the booth, blue, mixer visible |

Both are already in `apps/web-club/public/` and already ship in the bundle. The
greyscale/colour switch is a CSS `filter` on the photo, not a second asset.

## i18n

All new copy goes into the existing Connect catalog
`apps/web-club/src/connect/messages.ts` in **six languages** (en, es, fr, de, pt,
ru). Keys below are added to every block; English values are the reference. No
placeholder is introduced except the ones already there (`dlDetected {os}`,
`dlOpenStore {store}`). `node scripts/connect-catalog-check.mjs` is the gate.

### Hub keys

| Key | English reference |
| --- | --- |
| `hubTag` | `music requests at your club` |
| `hubNavGuests` | `Guest app` |
| `hubNavClubs` | `For clubs` |
| `hubSwitchHint` | `Choose your side` |
| `hubSideGuest` | `SIDE A · FOR GUESTS` |
| `hubSideClub` | `SIDE B · FOR VENUES` |
| `hubGuestTitle` | `I’m a guest.` |
| `hubClubTitle` | `I’m the club.` |
| `hubGuestBullet1` | `Scan the QR on the table or at the bar` |
| `hubGuestBullet2` | `Pick a track and pay in the app` |
| `hubGuestBullet3` | `Get a ping when the DJ drops it` |
| `hubClubBullet1` | `The club’s share lands on your account` |
| `hubClubBullet2` | `The queue and the DJs under your control` |
| `hubClubBullet3` | `Every night, track and table in one report` |
| `hubGuestStatPrice` | `example request price — set by the club` |
| `hubGuestStatFree` | `app and account, no charge` |
| `hubClubStatShare` | `example club share per request` |
| `hubClubStatSetup` | `setup, subscription and hardware` |
| `hubGuestImageAlt` | `Dancing crowd under the club lights` |
| `hubClubImageAlt` | `The dance floor seen from behind the DJ booth` |
| `hubFooterGuests` | `Guest app` |
| `hubFooterClubs` | `For clubs` |
| `hubFooterSupport` | `Support` |

The four stat **values** are money and are rendered in code with
`Intl.NumberFormat(locale, { style: 'currency', currency: 'EUR' })` from the
existing sample scenario (`components/connect/landing/scenario.ts`: request
`2,00 €`, club share `1,70 €`, service fee `0,30 €`). No amount is written inside
a translated string. The two `0 €` stats are not money claims — render them as the
number `0` formatted the same way.

### Download keys

| Key | English reference |
| --- | --- |
| `dlApk` | `Download for Android (APK)` |
| `dlApkNote` | `Beta build — installs directly, no store needed.` |
| `dlQrTitle` | `Open this page on your phone` |
| `dlQrNote` | `Point your camera at the code and the same page opens on your phone.` |
| `dlBackHub` | `Back to the choice` |
| `dlUnavailable` | `The guest app is not in the stores yet` |
| `dlSupport` | `Ask support` |
| `dlScreenScan` | `Scan the table QR` |
| `dlScreenPick` | `Pick a track, pay in a tap` |
| `dlScreenQueue` | `Watch the DJ’s queue` |
| `dlPriceNote` | `The club sets the request price. The service fee is already inside it.` |

Existing `dl*` keys are reused as-is: `dlEyebrow`, `dlTitle`, `dlCopy`, `dlNote`,
`dlDetected`, `dlManual`, `dlIos`, `dlAndroid`, `dlDesktop`, `dlDesktopNote`,
`dlOpenStore`, `dlSoon`, `dlSoonNote`, `dlBack`. Existing `gate*` keys are reused
verbatim for the gate headline and card copy (`gateTitle`, `gateNote`, `gateGuestLabel`,
`gateGuestTag`, `gateGuestDesc`, `gateGuestCta`, `gateClubLabel`, `gateClubTag`,
`gateClubDesc`, `gateClubCta`).

## Visual contract

The public surface is fixed dark in both themes (`.connect-root`, as the existing
Connect pages). Palette classes are `drop-*` from `tailwind.config.js`; the only
accent is `#00FF87`. Do not introduce a second accent, raw hex text colours, or a
new font. Type: Inter (already loaded); mono labels use `font-mono` (which
`.connect-root` maps to Inter with tabular numerals).

- Header: reuse `<PublicBar />` (`src/components/connect/PublicBar.tsx`), 72px,
  sticky, opaque `drop-rail`, hairline bottom border, brand lockup + tag + language
  select. Home links and the download page's back link point to `/hub` in demo
  builds so returning to the choice does not open the DJ console.
- Hub: a two-column grid that fills the viewport under the header. Each half is a
  single `<Link>` with a full-bleed photo, a legibility scrim, and the copy stack
  (side label, title, lead, bullets, stats, CTA). Narrow screens stack the halves.
- Photo state: the **inactive** photo is `filter: grayscale(1) brightness(.55)`;
  the hovered/focused side stays in colour. Use CSS `:has()` on the split container
  plus `:focus-visible`; no React state is needed. Keyboard focus takes priority
  over hover on the other half; hover styles apply only to hover-capable devices.
  Transitions: `filter` only,
  `var(--dur-base) var(--ease-out)`; reduced motion is already forced to ~0 by the
  global rule.
- Card CTAs: guest = accent fill with ink text; club = ink fill with accent text
  (the design frame's pairing). Buttons are 44px minimum.
- Scrims are functional only: a bottom-up `linear-gradient` from near-black so the
  copy keeps ≥4.5:1 contrast over any part of the photo. No glow, no glass, no
  decorative gradients anywhere else.
- Download: one centred column or two-column (copy + app preview) at ≥1024px.
  The preview is drawn in HTML/CSS with the brand tokens (device frame, three
  labelled app moments from `dlScreen*`), not a new bitmap. No invented metrics.
- Mobile width: one 12px outer gutter at ≤640px, 16px through 1023px. The
  download column uses the available tablet width rather than keeping the
  desktop 640px cap. On phones the APK file size sits below its description,
  leaving the download title and installation note the full text column.
- The page must not depend on animation for comprehension and must render with
  `prefers-reduced-motion: reduce` without running animations.

## Store links (honest states)

There is no App Store or Google Play listing yet and no signed public APK. The
page therefore has three states per store:

1. **Live** — `VITE_GUEST_IOS_URL` / `VITE_GUEST_ANDROID_URL` is set: the store
   button is a real link (`target="_blank" rel="noreferrer"`) and shows
   `dlOpenStore {store}`.
2. **Direct APK** — `/downloads/dropit-guest.apk` exists in the build (verified
   with a `HEAD` request at mount): show the `dlApk` row before the store buttons
   for Android/Desktop, with `dlApkNote` and the file size when the response
   exposes `Content-Length`. It becomes the accent download action on Android
   when no Google Play URL is configured, and is hidden when iOS is selected.
   Accept only an APK MIME type or a non-empty binary/ZIP response (including
   static hosts that omit MIME but provide a positive `Content-Length`); HTML, empty
   responses and unrelated response types do not prove a download exists.
   This is a test-stand capability, not a store.
3. **Soon** — neither: the button carries the `dlSoon` chip, an inline note shows
   `dlSoonNote`, and the section heading is `dlUnavailable`. Clicking still
   selects that platform (it is a chooser, not a dead end).

The `dlSoonNote` only promises links after publication; it must not claim that
the app is currently in store review without evidence.

Store names stay in code (`App Store`, `Google Play`) — they are brand names and
are not translated. `apps/web-club/.env.example` documents the two variables.
`scripts/publish-guest-apk.mjs` copies a built guest APK into
`public/downloads/dropit-guest.apk` (gitignored) for local/stage verification.

## OS detection

`src/connect/guest-app.ts` (owned by the download page implementer):

- iPhone/iPad/iPod in the UA → `ios`; iPadOS reports `Macintosh` with
  `maxTouchPoints > 1` → `ios`.
- `Android` in the UA → `android`.
- everything else → `desktop`.
- The detected platform preselects the store button; the segmented control
  (`iOS / Android / Desktop`, `aria-pressed`) overrides it. Desktop shows the QR
  block (`dlQrTitle` / `dlQrNote`) with the current page URL encoded, plus both
  store buttons.

## Verification

`node scripts/connect-catalog-check.mjs`, `node scripts/i18n-check.mjs`,
`cd apps/web-club && npx tsc --noEmit` and `npm run build` must be green. Screens
are measured with Playwright at 320, 390, 768, 1024 and 1440 px: no horizontal
overflow, 44px hit areas, axe clean, no console errors, focus ring visible, the
greyscale switch measured from `getComputedStyle(photo).filter`, and the three
download states photographed. Evidence lands in `output/hub-download/`.
