import { useMemo } from 'react';

const SIZE = 21;

/** Deterministic 0..1 from a string — stable preview for the same seed. */
function hashSeed(seed: string, index: number): number {
  let h = 2166136261 ^ index;
  for (let i = 0; i < seed.length; i += 1) {
    h ^= seed.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return ((h >>> 0) % 1000) / 1000;
}

function isFinderZone(row: number, col: number): boolean {
  const inTopLeft = row < 8 && col < 8;
  const inTopRight = row < 8 && col >= SIZE - 8;
  const inBottomLeft = row >= SIZE - 8 && col < 8;
  return inTopLeft || inTopRight || inBottomLeft;
}

function isFinderDark(row: number, col: number): boolean {
  const inRing = (r: number, c: number) => {
    const ring = Math.max(Math.abs(r - 3), Math.abs(c - 3));
    return ring === 3 || ring <= 1;
  };
  if (row < 8 && col < 8) return inRing(row, col);
  if (row < 8 && col >= SIZE - 8) return inRing(row, col - (SIZE - 8));
  if (row >= SIZE - 8 && col < 8) return inRing(row - (SIZE - 8), col);
  return false;
}

/**
 * Decorative QR preview for the "QR и тариф" step.
 *
 * The real, scannable code is issued by the backend after moderation
 * (`POST /v1/club/sessions/:id/qr/generate`) — this block only shows the
 * layout of the printed sticker, so it is explicitly labelled as a preview.
 */
export default function QrPreview({ seed = 'dropit' }: { seed?: string }) {
  const cells = useMemo(() => {
    const grid: boolean[] = [];
    for (let row = 0; row < SIZE; row += 1) {
      for (let col = 0; col < SIZE; col += 1) {
        grid.push(
          isFinderZone(row, col)
            ? isFinderDark(row, col)
            : hashSeed(seed, row * SIZE + col) > 0.52,
        );
      }
    }
    return grid;
  }, [seed]);

  return (
    <div className="inline-flex flex-col items-center rounded-card border border-drop-line bg-white p-4">
      <div
        className="grid gap-[2px]"
        style={{ gridTemplateColumns: `repeat(${SIZE}, minmax(0, 1fr))`, width: 148, height: 148 }}
        aria-hidden="true"
      >
        {cells.map((dark, index) => (
          <span
            key={index}
            className={dark ? 'bg-[#0A0C0B]' : 'bg-transparent'}
            style={{ aspectRatio: '1 / 1' }}
          />
        ))}
      </div>
      <p className="mt-2.5 font-mono text-[8px] uppercase tracking-[1.2px] text-[#0A0C0B]/60">
        предпросмотр стикера
      </p>
    </div>
  );
}
