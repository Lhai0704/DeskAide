import { invoke } from '@tauri-apps/api/core';

/** At most 10 CSS pixels per cell in the largest supported window; 768 bytes. */
export const HIT_COLS = 64;
export const HIT_ROWS = 96;
/** Extra CSS pixels around each visible part so edges stay easy to grab. */
export const HIT_MARGIN_PX = 3;

/** Fill triangle/row intersections, preserving gaps inside drawable bounds.
 * Runs at the mask cadence, never per animation frame; no GPU readback.
 */
export function fillTriangleHitMask(
  view: { width: number; height: number },
  origin: { x: number; y: number },
  center: { x: number; y: number },
  fittedHeight: number,
  triangles: ArrayLike<number>,
  margin = HIT_MARGIN_PX,
): Uint8Array | null {
  if (!(view.width > 0) || !(view.height > 0) || !(fittedHeight > 0)) return null;
  const bits = new Uint8Array(Math.ceil((HIT_COLS * HIT_ROWS) / 8));
  const cellW = view.width / HIT_COLS,
    cellH = view.height / HIT_ROWS;
  const points = [
    { x: 0, y: 0 },
    { x: 0, y: 0 },
    { x: 0, y: 0 },
  ];
  const scale = fittedHeight / 2;
  for (let i = 0; i + 5 < triangles.length; i += 6) {
    let minX = Infinity,
      maxX = -Infinity,
      minY = Infinity,
      maxY = -Infinity;
    for (let vertex = 0; vertex < 3; vertex++) {
      const p = points[vertex];
      p.x = origin.x + center.x + triangles[i + vertex * 2] * scale;
      p.y = origin.y + center.y - triangles[i + vertex * 2 + 1] * scale;
      minX = Math.min(minX, p.x);
      maxX = Math.max(maxX, p.x);
      minY = Math.min(minY, p.y);
      maxY = Math.max(maxY, p.y);
    }
    if (!Number.isFinite(minX + maxX + minY + maxY)) continue;
    const top = minY - margin,
      bottom = maxY + margin;
    if (bottom < 0 || top >= view.height || maxX + margin < 0 || minX - margin >= view.width)
      continue;
    const r0 = clampIndex(Math.floor(top / cellH), HIT_ROWS);
    const r1 = clampIndex(Math.floor(bottom / cellH), HIT_ROWS);
    const firstCol = clampIndex(Math.floor((minX - margin) / cellW), HIT_COLS);
    const lastCol = clampIndex(Math.floor((maxX + margin) / cellW), HIT_COLS);
    // Most face/clothing triangles overlap cells already covered by another mesh.
    // Skip those before clipping; coverage is a union, independent of draw order.
    let covered = true;
    for (let row = r0; covered && row <= r1; row++) {
      for (let col = firstCol; col <= lastCol; col++) {
        const bit = row * HIT_COLS + col;
        if (!(bits[bit >> 3] & (1 << (bit & 7)))) {
          covered = false;
          break;
        }
      }
    }
    if (covered) continue;
    for (let row = r0; row <= r1; row++) {
      const lo = row * cellH - margin,
        hi = (row + 1) * cellH + margin;
      let left = Infinity,
        right = -Infinity;
      for (let edge = 0; edge < 3; edge++) {
        const a = points[edge],
          b = points[(edge + 1) % 3];
        if (a.y >= lo && a.y <= hi) {
          left = Math.min(left, a.x);
          right = Math.max(right, a.x);
        }
        for (const y of [lo, hi]) {
          if (a.y === b.y || y < Math.min(a.y, b.y) || y > Math.max(a.y, b.y)) continue;
          const x = a.x + (b.x - a.x) * ((y - a.y) / (b.y - a.y));
          left = Math.min(left, x);
          right = Math.max(right, x);
        }
      }
      left -= margin;
      right += margin;
      if (left > right || right < 0 || left >= view.width) continue;
      const c0 = clampIndex(Math.floor(left / cellW), HIT_COLS);
      const c1 = clampIndex(Math.floor(right / cellW), HIT_COLS);
      for (let col = c0; col <= c1; col++) setBit(bits, row * HIT_COLS + col);
    }
  }
  return bits;
}

export function modelToWindow(
  modelX: number,
  modelY: number,
  origin: { x: number; y: number },
  center: { x: number; y: number },
  fittedHeight: number,
) {
  return {
    x: origin.x + center.x + (modelX * fittedHeight) / 2,
    y: origin.y + center.y - (modelY * fittedHeight) / 2,
  };
}

/**
 * Rasterize model-space part bounds into a bit mask for the window.
 * `bounds` is a flat list of [minX, minY, maxX, maxY] after the model matrix.
 * Bit 0 of each byte is the leftmost cell of that group.
 */
export function fillHitMask(
  view: { width: number; height: number },
  origin: { x: number; y: number },
  center: { x: number; y: number },
  fittedHeight: number,
  bounds: ArrayLike<number>,
  margin = HIT_MARGIN_PX,
): Uint8Array | null {
  if (!(view.width > 0) || !(view.height > 0) || !(fittedHeight > 0)) return null;
  const cols = HIT_COLS;
  const rows = HIT_ROWS;
  const bits = new Uint8Array(Math.ceil((cols * rows) / 8));
  const cellW = view.width / cols;
  const cellH = view.height / rows;
  for (let i = 0; i + 3 < bounds.length; i += 4) {
    const x0 = bounds[i];
    const y0 = bounds[i + 1];
    const x1 = bounds[i + 2];
    const y1 = bounds[i + 3];
    if (![x0, y0, x1, y1].every(Number.isFinite)) continue;
    const a = modelToWindow(x0, y0, origin, center, fittedHeight);
    const b = modelToWindow(x1, y1, origin, center, fittedHeight);
    const left = Math.min(a.x, b.x) - margin;
    const right = Math.max(a.x, b.x) + margin;
    const top = Math.min(a.y, b.y) - margin;
    const bottom = Math.max(a.y, b.y) + margin;
    if (right < 0 || bottom < 0 || left >= view.width || top >= view.height) continue;
    const c0 = clampIndex(Math.floor(left / cellW), cols);
    const c1 = clampIndex(Math.floor(Math.min(right, view.width - 1e-4) / cellW), cols);
    const r0 = clampIndex(Math.floor(top / cellH), rows);
    const r1 = clampIndex(Math.floor(Math.min(bottom, view.height - 1e-4) / cellH), rows);
    if (c1 < c0 || r1 < r0) continue;
    for (let row = r0; row <= r1; row++) {
      for (let col = c0; col <= c1; col++) setBit(bits, row * cols + col);
    }
  }
  return bits;
}

export function maskHits(
  bits: Uint8Array,
  cols: number,
  rows: number,
  x: number,
  y: number,
  width: number,
  height: number,
) {
  if (!(width > 0) || !(height > 0) || x < 0 || y < 0 || x >= width || y >= height) return false;
  const col = Math.min(cols - 1, Math.floor((x / width) * cols));
  const row = Math.min(rows - 1, Math.floor((y / height) * rows));
  const index = row * cols + col;
  return (bits[index >> 3] & (1 << (index & 7))) !== 0;
}

export function sameHitMask(a: Uint8Array, b: Uint8Array) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

let hitSeq = 0;
let current: Uint8Array | null = null;
let released = false;

/** True when the window should keep the pointer. No mask means the whole window is solid. */
export function pointHitsAvatar(x: number, y: number) {
  if (!current) return true;
  return maskHits(current, HIT_COLS, HIT_ROWS, x, y, window.innerWidth, window.innerHeight);
}

export function publishHitMask(bits: Uint8Array) {
  current = bits;
  const seq = ++hitSeq;
  return invoke('set_avatar_hit_mask', {
    seq,
    cols: HIT_COLS,
    rows: HIT_ROWS,
    bits: Array.from(bits),
  }).catch((error: unknown) => {
    if (current === bits) current = null;
    throw error;
  });
}

export function clearHitMask() {
  current = null;
  released = false;
  const seq = ++hitSeq;
  void invoke('clear_avatar_hit_mask', { seq }).catch(() => {});
}

/** Ask Windows to pass the next click through. Ignored while a press is in progress. */
export function releaseAvatarPointer() {
  if (!current || released) return;
  released = true;
  void invoke('set_avatar_passthrough', { passthrough: true }).catch(() => {
    released = false;
  });
}

export function noteAvatarPointer(x: number, y: number, holding: boolean) {
  if (holding || !current) return;
  if (pointHitsAvatar(x, y)) {
    released = false;
    return;
  }
  releaseAvatarPointer();
}

function setBit(bits: Uint8Array, index: number) {
  bits[index >> 3] |= 1 << (index & 7);
}

function clampIndex(value: number, count: number) {
  if (!Number.isFinite(value) || value < 0) return 0;
  if (value >= count) return count - 1;
  return value;
}
