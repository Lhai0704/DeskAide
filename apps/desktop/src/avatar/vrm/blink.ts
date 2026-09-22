/** Sine blink on the VRM `blink` expression. Interval is 1–6s, one close takes 0.2s. */
export class VrmBlink {
  private blinking = false;
  private progress = 0;
  private since = 0;
  private next = 0;

  constructor(private random: () => number = Math.random) {
    this.schedule();
  }

  private schedule() {
    this.next = 1 + this.random() * 5;
  }

  /** Weight from 0 to 1. While disabled the lid stays open and the next blink waits a full interval. */
  update(dt: number, enabled: boolean): number {
    if (!enabled) {
      this.blinking = false;
      this.progress = 0;
      this.since = 0;
      return 0;
    }
    const step = Math.min(0.1, Math.max(0, dt));
    this.since += step;
    if (!this.blinking && this.since >= this.next) {
      this.blinking = true;
      this.progress = 0;
    }
    if (!this.blinking) return 0;
    this.progress += step / 0.2;
    if (this.progress >= 1) {
      this.blinking = false;
      this.since = 0;
      this.schedule();
      return 0;
    }
    return enabled ? Math.sin(Math.PI * this.progress) : 0;
  }
}
