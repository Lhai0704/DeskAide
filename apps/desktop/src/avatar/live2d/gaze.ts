export interface CursorSample {
  x: number;
  y: number;
  originX: number;
  originY: number;
  scale: number;
}
export function localCursor(s: CursorSample) {
  if (!Number.isFinite(s.scale) || s.scale <= 0) return { x: 0, y: 0 };
  return { x: (s.x - s.originX) / s.scale, y: (s.y - s.originY) / s.scale };
}
export const clamp = (v: number, min = -1, max = 1) =>
  Math.max(min, Math.min(max, Number.isFinite(v) ? v : 0));
export class Gaze {
  x = 0;
  y = 0;
  private last = { x: 0, y: 0 };
  private moved = 0;
  private next = 0;
  private idle = { x: 0, y: 0 };
  update(
    point: { x: number; y: number } | null,
    bounds: { x: number; y: number; width: number; height: number },
    now: number,
    dt: number,
    idle: boolean,
    random = Math.random,
  ) {
    if (point && Math.hypot(point.x - this.last.x, point.y - this.last.y) > 2) {
      this.moved = now;
      this.last = point;
    }
    if (now > this.next) {
      this.idle = { x: (random() - 0.5) * 0.12, y: (random() - 0.5) * 0.08 };
      this.next = now + 2000 + random() * 3500;
    }
    const resting = !point || now - this.moved > 5000;
    const target = resting
      ? idle
        ? this.idle
        : { x: 0, y: 0 }
      : {
          x: clamp((point.x - bounds.x) / Math.max(1, bounds.width / 2)),
          y: clamp((bounds.y - point.y) / Math.max(1, bounds.height / 2)),
        };
    const alpha = 1 - Math.exp(-Math.min(dt, 0.1) / 0.16);
    this.x += ((Math.abs(target.x) < 0.025 ? 0 : target.x) - this.x) * alpha;
    this.y += ((Math.abs(target.y) < 0.025 ? 0 : target.y) - this.y) * alpha;
    return { x: this.x, y: this.y };
  }
}
