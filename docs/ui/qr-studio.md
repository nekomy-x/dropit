# QR generator — the code, styled, downloadable

Status: implemented in `apps/web-club/src/pages/QrStudioPage.tsx` (`/qr`).

## What it is

One screen, one job: **make the guest QR image**.

```
[ the code, big, always visible ]  [ link · dots · colours · mark · download ]
```

It draws no text onto the code and composes no printed sign. A venue that needs
a sign drops this image into whatever template it already has — that is what they
were doing before, and a print-shop screen inside an operations panel was the
wrong answer.

## Controls

| group | what it changes |
| --- | --- |
| **Link** | the encoded value: room link or a shift token |
| **Dots** | 10 presets, 5 dot shapes, corner rounding, gap, eye frame, eye centre |
| **Colours** | dots, background, eye, eye centre; single or gradient ink + angle; swap |
| **Mark in the middle** | upload an image, resize it |
| **Download** | PNG / SVG / JPG / PDF at 512, 1024 or 2048 px, plus print |

## Behaviours that are not cosmetic

- **A centre mark forces error correction to H.** Measured: at H a mark up to 28%
  of the width still decodes; at Q it stops at 22%. The control is disabled while
  a mark is present rather than letting an operator choose a combination that
  cannot scan.
- **The page decodes its own output** with `jsQR` and says "Scans correctly" or
  "This no longer scans". Checked at four raster sizes, because jsQR's binariser
  is resolution-sensitive while zbar and Apple Vision are not; a sign passes if
  any size reads it, which is what a camera actually does.
- **Contrast is stated, not implied.** Below 7:1 the panel warns; dots lighter
  than the background is an error with a one-click swap.

## Measured limits (enforced by the UI)

| variable | decoded | failed |
| --- | --- | --- |
| dot shape | square, rounded, dot, joined, liquid | — |
| mark at 28% of width | ECC H | — |
| mark at 34% | — | failed at ECC H |
| gap 30% between dots | — | failed |
| light-on-dark symbol | — | failed |
| circular eye frame | see note | a true circle failed on zbar, jsQR and Apple Vision |

The circular eye keeps its 1:1:3:1:1 finder run only on the two lines through its
centre, so `circle` renders as the furthest-rounded squircle (radius ≤ 0.45 of
the 7-module span) instead of a true circle.

## Files

| file | responsibility |
| --- | --- |
| `src/qr/types.ts` | the style vocabulary; the decode limits are documented here |
| `src/qr/encode.ts` | string → module grid (`qrcode` core, no renderer) |
| `src/qr/geometry.ts` | symbol markup in module units: shapes, eyes, mark plate, gradients |
| `src/qr/export.ts` | SVG / PNG / JPG / PDF / print, all client-side |
| `src/qr/color.ts` | WCAG maths and the contrast floors |
| `src/qr/decode-check.ts` | rasterise the real drawing and decode it |
| `src/qr/presets.ts` | the 10 presets |
| `src/qr/studio-state.ts` | the reducer: a mark raises H, an edit leaves the preset |
| `src/components/qr/QrControls.tsx` | colour / range / choice-tile controls |
| `src/components/qr/QrSignPreview.tsx` | renders the export document verbatim |
| `src/pages/qr-studio.css` | layout and control styling (token colours only) |

## Data honesty

- The room link is always available and is the default.
- A shift token is fetched only while a shift is open; the button is disabled
  otherwise with a reason.
- Nothing is uploaded: the mark is read with `FileReader`, the files are written
  by the browser.

## Verification

```bash
cd apps/web-club && npm run dev:demo     # terminal 1
node scripts/qr-studio-check.mjs         # terminal 2 (repo root)
node scripts/panel-audit.mjs --app club  # contract on every club route
```

The gate renders the page in Chromium and decodes every preset, every dot shape ×
eye combination and the exported PNG with **zbarimg** — an independent C decoder,
so the page cannot pass by agreeing with itself. It also checks that a swapped
ink/paper pair turns the page's own scan check red.
