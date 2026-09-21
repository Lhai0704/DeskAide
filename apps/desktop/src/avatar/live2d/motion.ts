import type { MotionRef, SemanticState } from '../types';
export interface MotionAdapter {
  start(key: string): boolean;
  finished(): boolean;
  stop(): void;
}
export const motionKey = (m: MotionRef) => `${m.group}:${m.index}`;
export class MotionController {
  private active = '';
  private shot = false;
  private activeState: SemanticState = 'idle';
  private token = 0;
  private shotElapsed = 0;
  private elapsed = 0;
  private missing = new Set<string>();
  constructor(
    private model: MotionAdapter,
    private mappings: Partial<Record<SemanticState, MotionRef>>,
  ) {}
  update(state: SemanticState, enabled: boolean, idle: boolean, dt = 1 / 30) {
    this.elapsed += Math.max(0, dt);
    if (!enabled) {
      this.cancel();
      return;
    }
    if (this.shot) {
      this.shotElapsed += dt;
      if (this.shotElapsed < 8 && !this.model.finished()) return;
    }
    if (this.shot) {
      this.shot = false;
      this.active = '';
    }
    const mapped =
      (state === 'activated' || (state === 'idle' && !idle) ? undefined : this.mappings[state]) ??
      (state === 'speaking' ? this.mappings.responding : undefined) ??
      (idle ? this.mappings.idle : undefined);
    let key = mapped ? motionKey(mapped) : '';
    const fallback = idle && this.mappings.idle ? motionKey(this.mappings.idle) : '';
    if (this.missing.has(key)) key = this.missing.has(fallback) ? '' : fallback;
    // The semantic state is already idle, but a brief reply gesture can finish visually.
    // New turns and explicit state motions still interrupt it through the SDK crossfade.
    if (
      state === 'idle' &&
      idle &&
      this.active &&
      this.active !== fallback &&
      (this.activeState === 'responding' || this.activeState === 'speaking') &&
      this.elapsed < 8 &&
      !this.model.finished()
    )
      return;
    if (!key) {
      if (this.active) this.cancel();
      return;
    }
    if (key !== this.active || this.model.finished()) {
      this.active = this.start(key) ? key : '';
      this.activeState = state;
      this.elapsed = 0;
      if (!this.active && fallback && !this.missing.has(fallback)) {
        this.active = this.start(fallback) ? fallback : '';
      }
    }
  }
  oneShot(motion: MotionRef | undefined, token: number) {
    if (token === this.token || !motion) return;
    this.token = token;
    this.shotElapsed = 0;
    this.shot = this.start(motionKey(motion));
    if (this.shot) this.active = motionKey(motion);
  }
  cancel() {
    this.model.stop();
    this.shot = false;
    this.active = '';
    this.elapsed = 0;
  }
  private start(key: string) {
    if (this.missing.has(key)) return false;
    if (this.model.start(key)) return true;
    this.missing.add(key);
    return false;
  }
}
