export interface MouthVisemes {
  aa: number;
  ih: number;
  ou: number;
  ee: number;
  oh: number;
}

const silent: MouthVisemes = { aa: 0, ih: 0, ou: 0, ee: 0, oh: 0 };

/** Map a 0–1 playback level onto VRM vowel presets. The flap keeps a held tone from freezing. */
export function mouthVisemes(level: number, time: number): MouthVisemes {
  const open = Math.max(0, Math.min(1, Number.isFinite(level) ? level : 0));
  if (open === 0) return silent;
  const flap = 0.55 + 0.45 * (0.5 + 0.5 * Math.sin(time * 14));
  return {
    aa: open * flap,
    ih: open * (1 - flap) * 0.7,
    ou: 0,
    ee: 0,
    oh: 0,
  };
}

/**
 * Smooth the published speech level and release the mouth presets once it closes,
 * so an idle VRMA can drive the mouth again.
 */
export class MouthDriver {
  private weight = 0;
  private holding = false;

  update(level: number, speaking: boolean, dt: number, time: number): MouthVisemes | null {
    const target = speaking ? Math.max(0, Math.min(1, Number.isFinite(level) ? level : 0)) : 0;
    const alpha = 1 - Math.exp(-Math.min(Math.max(dt, 0), 0.1) / 0.08);
    this.weight += (target - this.weight) * alpha;
    if (speaking || this.weight > 0.02) {
      this.holding = true;
      return mouthVisemes(this.weight, time);
    }
    if (this.holding) {
      this.holding = false;
      this.weight = 0;
      return silent;
    }
    return null;
  }
}
