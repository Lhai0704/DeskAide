import { describe, expect, it } from 'vitest';
import {
  fillHitMask,
  fillTriangleHitMask,
  HIT_COLS,
  HIT_ROWS,
  maskHits,
  modelToWindow,
  sameHitMask,
} from './passthrough';

describe('Live2D click-through mask', () => {
  const view = { width: 320, height: 480 };
  const origin = { x: 0, y: 0 };
  const center = { x: 160, y: 480 };

  it('keeps the empty corner of a diagonal mesh transparent', () => {
    const bits = fillTriangleHitMask(
      view,
      origin,
      { x: 0, y: 0 },
      2,
      [20, -20, 280, -20, 20, -400],
      0,
    )!;
    expect(maskHits(bits, HIT_COLS, HIT_ROWS, 30, 30, 320, 480)).toBe(true);
    expect(maskHits(bits, HIT_COLS, HIT_ROWS, 250, 350, 320, 480)).toBe(false);
    expect(maskHits(bits, HIT_COLS, HIT_ROWS, 1, 1, 320, 480)).toBe(false);
  });

  it('preserves thin triangles crossing a row without containing a cell center', () => {
    const bits = fillTriangleHitMask(
      view,
      origin,
      { x: 0, y: 0 },
      2,
      [1, -1, 299, -299, 299, -300],
      0,
    )!;
    expect(maskHits(bits, HIT_COLS, HIT_ROWS, 152, 152, 320, 480)).toBe(true);
    expect(maskHits(bits, HIT_COLS, HIT_ROWS, 152, 172, 320, 480)).toBe(false);
  });

  it('clears a vanished model and ignores offscreen or invalid triangles', () => {
    for (const triangles of [[], [-30, 30, -20, 30, -20, 20], [NaN, 0, 1, 1, 2, 2]]) {
      const bits = fillTriangleHitMask(view, origin, { x: 0, y: 0 }, 2, triangles)!;
      expect(bits.every((b) => b === 0)).toBe(true);
    }
  });

  it('maps model space the same way hit testing does, with Y up', () => {
    const point = modelToWindow(0.5, 1, origin, center, 200);
    expect(point).toEqual({ x: 210, y: 380 });
    expect(((point.x - center.x) * 2) / 200).toBeCloseTo(0.5);
    expect(((center.y - point.y) * 2) / 200).toBeCloseTo(1);
  });

  it('marks the character and leaves the opposite corner empty', () => {
    const bits = fillHitMask(view, origin, center, 200, new Float32Array([0, 0, 0.2, 0.2]), 0);
    expect(bits).not.toBeNull();
    const mask = bits!;
    const onCharacter = modelToWindow(0.1, 0.1, origin, center, 200);
    expect(
      maskHits(mask, HIT_COLS, HIT_ROWS, onCharacter.x, onCharacter.y, view.width, view.height),
    ).toBe(true);
    expect(maskHits(mask, HIT_COLS, HIT_ROWS, 1, 1, view.width, view.height)).toBe(false);
    expect(maskHits(mask, HIT_COLS, HIT_ROWS, view.width - 1, 1, view.width, view.height)).toBe(
      false,
    );
  });

  it('shifts the mask when the canvas is inset in the window', () => {
    const inset = { x: 40, y: 20 };
    const placed = { x: 100, y: 200 };
    const bits = fillHitMask(view, inset, placed, 200, new Float32Array([0, 0, 0, 0]), 0);
    expect(bits).not.toBeNull();
    const painted = modelToWindow(0, 0, inset, placed, 200);
    expect(maskHits(bits!, HIT_COLS, HIT_ROWS, painted.x, painted.y, view.width, view.height)).toBe(
      true,
    );
    expect(maskHits(bits!, HIT_COLS, HIT_ROWS, placed.x, placed.y, view.width, view.height)).toBe(
      false,
    );
  });

  it('uses the low bit of the first byte for the top-left cell', () => {
    const bits = fillHitMask(
      { width: 80, height: 10 },
      origin,
      { x: 0, y: 0 },
      2,
      new Float32Array([0, 0, 0, 0]),
      0,
    );
    expect(bits![0] & 1).toBe(1);
    expect(maskHits(bits!, HIT_COLS, HIT_ROWS, 0.1, 0.01, 80, 10)).toBe(true);
    expect(maskHits(bits!, HIT_COLS, HIT_ROWS, 79, 0.01, 80, 10)).toBe(false);
    expect(maskHits(bits!, HIT_COLS, HIT_ROWS, 80, 0, 80, 10)).toBe(false);
  });

  it('grows the grabbable edge without filling a distant corner', () => {
    const placed = { x: 160, y: 240 };
    const tight = fillHitMask(view, origin, placed, 200, new Float32Array([0, 0, 0, 0]), 0);
    const loose = fillHitMask(view, origin, placed, 200, new Float32Array([0, 0, 0, 0]), 8);
    expect(tight).not.toBeNull();
    expect(loose).not.toBeNull();
    expect(sameHitMask(tight!, loose!)).toBe(false);
    let extra = 0;
    for (let i = 0; i < loose!.length; i++) extra += bitCount(loose![i] & ~tight![i]);
    expect(extra).toBeGreaterThan(0);
    expect(maskHits(loose!, HIT_COLS, HIT_ROWS, 1, 1, view.width, view.height)).toBe(false);
  });
});

function bitCount(value: number) {
  let count = 0;
  for (let bits = value; bits; bits >>>= 1) count += bits & 1;
  return count;
}
