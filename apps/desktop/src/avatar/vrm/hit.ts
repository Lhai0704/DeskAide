import { HIT_COLS, HIT_MARGIN_PX, HIT_ROWS } from '../live2d/passthrough';

export interface HitSegment {
  ax: number;
  ay: number;
  bx: number;
  by: number;
  radius: number;
}

function clampIndex(value: number, count: number) {
  return Math.max(0, Math.min(count - 1, value));
}

function distanceToSegment(px: number, py: number, ax: number, ay: number, bx: number, by: number) {
  const abx = bx - ax;
  const aby = by - ay;
  const lengthSquared = abx * abx + aby * aby;
  const t =
    lengthSquared > 0
      ? Math.max(0, Math.min(1, ((px - ax) * abx + (py - ay) * aby) / lengthSquared))
      : 0;
  const dx = px - (ax + abx * t);
  const dy = py - (ay + aby * t);
  return Math.hypot(dx, dy);
}

/** Rasterize bone capsules into the same bit mask Live2D publishes for click-through. */
export function fillCapsuleHitMask(
  view: { width: number; height: number },
  segments: HitSegment[],
): Uint8Array | null {
  if (!(view.width > 0) || !(view.height > 0) || segments.length === 0) return null;
  const bits = new Uint8Array(Math.ceil((HIT_COLS * HIT_ROWS) / 8));
  const cellW = view.width / HIT_COLS;
  const cellH = view.height / HIT_ROWS;
  for (const segment of segments) {
    const { ax, ay, bx, by, radius } = segment;
    if (![ax, ay, bx, by, radius].every((value) => Number.isFinite(value))) continue;
    const reach = radius + HIT_MARGIN_PX;
    if (!(reach > 0)) continue;
    const minX = Math.min(ax, bx) - reach;
    const maxX = Math.max(ax, bx) + reach;
    const minY = Math.min(ay, by) - reach;
    const maxY = Math.max(ay, by) + reach;
    if (maxX < 0 || maxY < 0 || minX >= view.width || minY >= view.height) continue;
    const c0 = clampIndex(Math.floor(minX / cellW), HIT_COLS);
    const c1 = clampIndex(Math.floor(maxX / cellW), HIT_COLS);
    const r0 = clampIndex(Math.floor(minY / cellH), HIT_ROWS);
    const r1 = clampIndex(Math.floor(maxY / cellH), HIT_ROWS);
    for (let row = r0; row <= r1; row++) {
      for (let col = c0; col <= c1; col++) {
        const cx = (col + 0.5) * cellW;
        const cy = (row + 0.5) * cellH;
        if (distanceToSegment(cx, cy, ax, ay, bx, by) > reach) continue;
        const bit = row * HIT_COLS + col;
        bits[bit >> 3] |= 1 << (bit & 7);
      }
    }
  }
  return bits;
}
