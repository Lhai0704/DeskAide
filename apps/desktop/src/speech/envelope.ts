export class MouthEnvelope {
  level = 0;
  update(samples: Float32Array, dt: number) {
    let sum = 0;
    for (const value of samples) sum += value * value;
    const rms = Math.sqrt(sum / Math.max(1, samples.length));
    const target = Math.min(1, Math.max(0, (rms - 0.008) * 7));
    const tau = target > this.level ? 0.045 : 0.11;
    this.level += (target - this.level) * (1 - Math.exp(-Math.max(0, dt) / tau));
    if (this.level < 0.001) this.level = 0;
    return this.level;
  }
  reset() {
    this.level = 0;
  }
}
