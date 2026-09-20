export class SpeechPlayer {
  private context: AudioContext | null = null;
  private gain: GainNode | null = null;
  private end = 0;
  get buffered(): number {
    return this.context ? Math.max(0, this.end - this.context.currentTime) : 0;
  }
  async start(volume: number) {
    this.stop();
    const context = new AudioContext();
    this.context = context;
    this.gain = context.createGain();
    this.gain.gain.value = volume;
    this.gain.connect(context.destination);
    await context.resume();
    if (context.state !== 'running') throw new Error('音频播放未获允许，请点击试听后重试');
  }
  setVolume(volume: number) {
    if (this.gain) this.gain.gain.value = volume;
  }
  append(pcm: string, rate: number) {
    if (!this.context || !this.gain) return;
    const bytes = Uint8Array.from(atob(pcm), (c) => c.charCodeAt(0));
    if (
      !bytes.length ||
      bytes.length % 4 ||
      !Number.isInteger(rate) ||
      rate < 8000 ||
      rate > 192000
    )
      throw new Error('无效音频片段');
    const view = new DataView(bytes.buffer);
    const samples = new Float32Array(bytes.length / 4);
    for (let i = 0; i < samples.length; i++) {
      samples[i] = view.getFloat32(i * 4, true);
      if (!Number.isFinite(samples[i])) throw new Error('无效音频采样');
    }
    const buffer = this.context.createBuffer(1, samples.length, rate);
    buffer.copyToChannel(samples, 0);
    const source = this.context.createBufferSource();
    source.buffer = buffer;
    source.connect(this.gain);
    const at = Math.max(this.end, this.context.currentTime + (this.buffered ? 0 : 0.15));
    source.start(at);
    source.onended = () => source.disconnect();
    this.end = at + buffer.duration;
  }
  stop() {
    if (this.context) void this.context.close().catch(() => {});
    this.context = null;
    this.gain = null;
    this.end = 0;
  }
}
