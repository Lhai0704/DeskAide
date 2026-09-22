import { AnimationClip, NumberKeyframeTrack, VectorKeyframeTrack } from 'three';
import { describe, expect, it } from 'vitest';
import { HIT_COLS, HIT_ROWS, maskHits } from '../live2d/passthrough';
import { assertManifest } from '../manifest';
import { avatarDefaults, type VrmAvatarPackManifest } from '../types';
import { reAnchorRootPositionTrack } from './anchor';
import { VrmBlink } from './blink';
import { selectVrmClip, semanticExpression } from './clips';
import { VRM_FOV_DEG, vrmCameraFrame, vrmPixelRatio, type VrmBounds } from './frame';
import { fillCapsuleHitMask } from './hit';
import { MouthDriver, mouthVisemes } from './lips';

const pack: VrmAvatarPackManifest = {
  schemaVersion: 4,
  renderer: 'vrm',
  id: 'vrm-assistant',
  name: 'VRM',
  version: '1.0.0',
  alt: '助手',
  preview: 'preview.png',
  model: 'avatar.vrm',
  defaultWidth: 560,
  defaultHeight: 720,
};

const human: VrmBounds = {
  minX: -0.22,
  maxX: 0.22,
  minY: 0,
  maxY: 1.6,
  minZ: -0.12,
  maxZ: 0.12,
};

describe('VRM manifest', () => {
  it('accepts a model without motions', () => {
    expect(() => assertManifest(pack)).not.toThrow();
    expect(() =>
      assertManifest({
        ...pack,
        layout: { scale: 1 },
        motions: { idle: 'motions/idle.vrma', activated: 'motions/wave.vrma' },
        expressions: { neutral: 'neutral', thinking: 'relaxed', tap: 'happy' },
        behavior: { mouseTracking: true, idleAnimation: true, motions: true, blink: 'auto' },
        metadata: { author: '作者', license: '许可' },
      }),
    ).not.toThrow();
  });

  it('rejects a layout position and a non-vrma motion', () => {
    expect(() => assertManifest({ ...pack, layout: { position: { x: 0.5, y: 1 } } })).toThrow(
      /scale/,
    );
    expect(() => assertManifest({ ...pack, motions: { idle: 'motions/idle.bvh' } })).toThrow(
      /vrma/,
    );
  });

  it('turns blink off only when the pack asks', () => {
    expect(avatarDefaults(pack).autoBlink).toBe(true);
    expect(avatarDefaults({ ...pack, behavior: { blink: 'off' } }).autoBlink).toBe(false);
    expect(avatarDefaults(pack).motions).toBe(true);
  });
});

describe('VRM camera', () => {
  const view = { width: 560, height: 720 };

  it('keeps the head inside the frame and the feet below it at scale 1', () => {
    const frame = vrmCameraFrame(human, view, 1, { scale: 1, verticalPosition: 0 });
    expect(frame).not.toBeNull();
    const half = frame!.distance * Math.tan(((VRM_FOV_DEG / 2) * Math.PI) / 180);
    expect(frame!.target.y + half).toBeGreaterThanOrEqual(human.maxY);
    expect(frame!.target.y - half).toBeGreaterThan(human.minY);
    expect(frame!.position.z).toBeGreaterThan(frame!.target.z);
  });

  it('moves closer as the user scale grows and shifts up with verticalPosition', () => {
    const near = vrmCameraFrame(human, view, 1, { scale: 1, verticalPosition: 0 })!;
    const far = vrmCameraFrame(human, view, 1, { scale: 2, verticalPosition: 0 })!;
    const raised = vrmCameraFrame(human, view, 1, { scale: 1, verticalPosition: 0.4 })!;
    expect(far.distance).toBeLessThan(near.distance);
    expect(raised.target.y).toBeGreaterThan(near.target.y);
  });

  it('keeps a T-pose from zooming out to the whole body', () => {
    const narrow = vrmCameraFrame(human, view, 1, { scale: 1, verticalPosition: 0 })!;
    const wide = vrmCameraFrame(
      { ...human, minX: -1.4, maxX: 1.4 },
      view,
      1,
      { scale: 1, verticalPosition: 0 },
    )!;
    expect(wide.distance).toBeCloseTo(narrow.distance);
  });

  it('caps the backing store at 2 device pixels', () => {
    expect(vrmPixelRatio(1.5)).toBe(1.5);
    expect(vrmPixelRatio(3)).toBe(2);
    expect(vrmPixelRatio(Number.NaN)).toBe(1);
  });
});

describe('VRM behavior', () => {
  it('blinks once and then waits', () => {
    const blink = new VrmBlink(() => 0);
    const weights = Array.from({ length: 20 }, () => blink.update(0.1, true));
    expect(weights.some((weight) => weight > 0)).toBe(true);
    expect(weights.at(-1)).toBe(0);
    expect(blink.update(0.1, false)).toBe(0);
  });

  it('opens on aa while speaking and releases the mouth afterwards', () => {
    expect(mouthVisemes(0, 1).aa).toBe(0);
    const open = mouthVisemes(1, 0);
    expect(open.aa).toBeGreaterThan(open.ih);
    expect(open.aa).toBeLessThanOrEqual(1);
    const mouth = new MouthDriver();
    expect(mouth.update(0.8, true, 0.05, 0)?.aa).toBeGreaterThan(0);
    let sample: ReturnType<MouthDriver['update']> = mouth.update(0, false, 0.05, 0);
    let previous = sample;
    for (let i = 0; i < 40 && sample; i++) {
      previous = sample;
      sample = mouth.update(0, false, 0.05, 0);
    }
    expect(sample).toBeNull();
    expect(previous).toEqual({ aa: 0, ih: 0, ou: 0, ee: 0, oh: 0 });
  });

  it('plays a state clip once, then idle, and can suppress the replay', () => {
    const clips = new Set(['idle', 'speaking', 'activated']);
    expect(selectVrmClip('speaking', true, true, clips)).toEqual({ name: 'speaking', loop: true });
    expect(selectVrmClip('activated', true, true, clips)).toEqual({
      name: 'activated',
      loop: false,
    });
    expect(selectVrmClip('activated', true, true, clips, 'activated')).toEqual({
      name: 'idle',
      loop: true,
    });
    expect(selectVrmClip('thinking', false, true, clips)).toEqual({ name: 'idle', loop: true });
    expect(selectVrmClip('idle', false, false, clips)).toBeNull();
    expect(semanticExpression('thinking', 0, { thinking: 'relaxed', tap: 'happy' })).toBe(
      'relaxed',
    );
    expect(semanticExpression('idle', 4, { thinking: 'relaxed', tap: 'happy' })).toBe('happy');
  });
});

describe('VRM hit mask and animation anchor', () => {
  it('marks the capsule and leaves a far corner open', () => {
    const bits = fillCapsuleHitMask({ width: 640, height: 960 }, [
      { ax: 200, ay: 400, bx: 440, by: 400, radius: 30 },
    ]);
    expect(bits).not.toBeNull();
    expect(maskHits(bits!, HIT_COLS, HIT_ROWS, 320, 400, 640, 960)).toBe(true);
    expect(maskHits(bits!, HIT_COLS, HIT_ROWS, 8, 8, 640, 960)).toBe(false);
  });

  it('moves position tracks so the first hips frame matches the rest pose', () => {
    const clip = new AnimationClip('idle', 1, [
      new VectorKeyframeTrack('hips.position', [0, 1], [1, 2, 3, 4, 5, 6]),
      new VectorKeyframeTrack('chest.position', [0], [3, 3, 3]),
      new NumberKeyframeTrack('hips.scale[x]', [0], [1]),
    ]);
    expect(reAnchorRootPositionTrack(clip, 'hips', { x: 0, y: 0, z: 0 })).toBe(true);
    const hips = clip.tracks[0] as VectorKeyframeTrack;
    expect(Array.from(hips.values)).toEqual([0, 0, 0, 3, 3, 3]);
    const chest = clip.tracks[1] as VectorKeyframeTrack;
    expect(Array.from(chest.values)).toEqual([2, 1, 0]);
    expect((clip.tracks[2] as NumberKeyframeTrack).values[0]).toBe(1);
  });
});
