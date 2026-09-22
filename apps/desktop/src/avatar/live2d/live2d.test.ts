import { describe, it, expect, vi } from 'vitest';
import { Gaze, localCursor } from './gaze';
import { MotionController } from './motion';
import { validateModelReferences } from './model';
import { backingStoreScale, frameDue, stageFrame } from './frame';
import { AvatarInteraction } from '../interaction';
import { assertManifest, safeAssetPath } from '../manifest';
const pack = {
  schemaVersion: 3,
  renderer: 'live2d',
  id: 'test',
  name: 'Test',
  version: '1',
  alt: 'Test',
  preview: 'preview.png',
  model: 'model/test.model3.json',
  defaultWidth: 240,
  defaultHeight: 320,
};
describe('Live2D manifests and assets', () => {
  it('accepts v3 without optional motions', () => expect(() => assertManifest(pack)).not.toThrow());
  it.each([
    '../secret',
    '/root',
    'C:/root',
    'a\\b',
    'https://evil/a',
    '%2e%2e/a',
    'a//b',
    'a?b',
    'a#b',
  ])('rejects %s', (path) => expect(() => safeAssetPath(path)).toThrow());
  it.each([
    { layout: { scale: NaN } },
    { motions: { idle: { group: 'Idle', index: -1 } } },
    { behavior: { mouseTracking: 'true' } },
    { expressions: { angry: 'x' } },
  ])('rejects bad fields %j', (patch) =>
    expect(() => assertManifest({ ...pack, ...patch })).toThrow(),
  );
  it('validates embedded optional resource references before fetching', () => {
    expect(() =>
      validateModelReferences({
        FileReferences: {
          Moc: 'a.moc3',
          Textures: ['a.png'],
          Motions: { Idle: [{ File: '../outside' }] },
        },
      }),
    ).toThrow();
  });
});
describe('stage framing', () => {
  const prefs = { scale: 1, verticalPosition: 0 };
  it('fills a portrait window with the upper body and keeps pixels square', () => {
    const view = { width: 240, height: 320 };
    const frame = stageFrame(view, { width: 2, height: 2 }, undefined, prefs);
    expect(frame).not.toBeNull();
    expect(frame!.fittedHeight).toBe(480);
    expect(frame!.fittedWidth).toBe(480);
    expect(frame!.tx).toBeCloseTo(0);
    expect(frame!.ty).toBeCloseTo(-1);
    expect(frame!.sx * (view.width / 2)).toBeCloseTo(frame!.sy * (view.height / 2));
  });
  it('honors an explicit centered layout without stretching a wide model', () => {
    const view = { width: 240, height: 320 };
    const frame = stageFrame(
      view,
      { width: 4, height: 2 },
      { scale: 1, anchor: { x: 0.5, y: 0.5 }, position: { x: 0.5, y: 0.5 } },
      prefs,
    );
    expect(frame!.fittedWidth / frame!.fittedHeight).toBeCloseTo(2);
    expect(frame!.tx).toBeCloseTo(0);
    expect(frame!.ty).toBeCloseTo(0);
    expect(frame!.sx * (view.width / 2)).toBeCloseTo(frame!.sy * (view.height / 2));
  });
  it.each([
    [1, 1],
    [1.25, 1.25],
    [1.5, 1.5],
    [2, 2],
    [4, 4],
  ])('keeps a %s display on whole device pixels', (dpr, scale) => {
    expect(backingStoreScale(dpr)).toBe(scale);
  });
  it('draws on a 60Hz cadence and drops duplicate timestamps', () => {
    expect(frameDue(100, 0)).toBe(true);
    expect(frameDue(110, 100)).toBe(false);
    expect(frameDue(115.2, 100)).toBe(true);
  });
});
describe('cursor and gesture math', () => {
  it.each([1, 1.25, 1.5, 2])('converts negative-screen physical pixels at %s DPI', (scale) =>
    expect(localCursor({ x: -1400, y: 300, originX: -1600, originY: 100, scale })).toEqual({
      x: 200 / scale,
      y: 200 / scale,
    }),
  );
  it('smooths/clamps far outside cursor and avoids large idle saccades', () => {
    const g = new Gaze();
    const b = { x: 120, y: 160, width: 240, height: 320 };
    let p = g.update({ x: 100000, y: -100000 }, b, 10, 0.033, true);
    expect(p.x).toBeGreaterThan(0);
    expect(p.x).toBeLessThan(0.3);
    for (let t = 100; t < 7000; t += 34) p = g.update(null, b, t, 0.034, true, () => 0.9);
    expect(Math.abs(p.x)).toBeLessThan(0.1);
  });
  it('never turns an out-and-back drag into a click', () => {
    const g = new AvatarInteraction();
    expect(g.active).toBe(false);
    g.down(0, 0);
    expect(g.active).toBe(true);
    expect(g.move(8, 0)).toBe(true);
    g.move(0, 0);
    expect(g.up()).toBe(false);
    g.down(0, 0);
    g.move(2, 0);
    expect(g.up()).toBe(true);
  });
});
describe('motion arbitration', () => {
  it('does not restart shared idle motion on semantic changes', () => {
    const m = { start: vi.fn(() => true), finished: () => false, stop: vi.fn() };
    const c = new MotionController(m, { idle: { group: 'Idle', index: 0 } });
    for (const state of ['idle', 'thinking', 'responding', 'speaking', 'idle'] as const)
      c.update(state, true, true);
    expect(m.start).toHaveBeenCalledTimes(1);
  });
  it('finishes a short reply gesture and shares it with speaking without restarting', () => {
    let done = false;
    const m = { start: vi.fn(() => true), finished: () => done, stop: vi.fn() };
    const c = new MotionController(m, {
      idle: { group: 'Idle', index: 0 },
      responding: { group: 'Reply', index: 0 },
    });
    c.update('responding', true, true);
    c.update('speaking', true, true);
    c.update('responding', true, true);
    c.update('idle', true, true);
    expect(m.start).toHaveBeenCalledTimes(1);
    expect(m.stop).not.toHaveBeenCalled();
    done = true;
    c.update('idle', true, true);
    expect(m.start).toHaveBeenLastCalledWith('Idle:0');
  });
  it('does not truncate tap at 1.2 seconds, but bounds broken/looping adapters', () => {
    const m = { start: vi.fn(() => true), finished: () => false, stop: vi.fn() };
    const c = new MotionController(m, { idle: { group: 'Idle', index: 0 } });
    c.oneShot({ group: 'Tap', index: 0 }, 1);
    for (let i = 0; i < 40; i++) c.update('idle', true, true, 0.05);
    expect(m.start).toHaveBeenCalledTimes(1);
    for (let i = 0; i < 140; i++) c.update('idle', true, true, 0.05);
    expect(m.start).toHaveBeenLastCalledWith('Idle:0');
  });
  it('one-shot wins then restores latest state; disabled motion stops', () => {
    let done = false;
    const m = { start: vi.fn(() => true), finished: () => done, stop: vi.fn() };
    const c = new MotionController(m, {
      idle: { group: 'Idle', index: 0 },
      thinking: { group: 'Think', index: 0 },
    });
    c.update('idle', true, true);
    c.oneShot({ group: 'Tap', index: 0 }, 1);
    c.update('thinking', true, true);
    expect(m.start).toHaveBeenLastCalledWith('Tap:0');
    done = true;
    c.update('thinking', true, true);
    expect(m.start).toHaveBeenLastCalledWith('Think:0');
    c.update('thinking', false, true);
    expect(m.stop).toHaveBeenCalled();
  });
  it('missing semantic motion falls back to idle', () => {
    const m = { start: vi.fn((k) => k === 'Idle:0'), finished: () => false, stop: vi.fn() };
    const c = new MotionController(m, {
      idle: { group: 'Idle', index: 0 },
      thinking: { group: 'Missing', index: 0 },
    });
    c.update('thinking', true, true);
    expect(m.start).toHaveBeenLastCalledWith('Idle:0');
    for (let i = 0; i < 60; i++) c.update('thinking', true, true);
    expect(m.start).toHaveBeenCalledTimes(2);
  });
});
