import { MouthEnvelope } from './envelope';
export class SpeechPlayer {
  private analyser: AnalyserNode | null = null;
  private envelope = new MouthEnvelope();
  private sources = new Map<AudioBufferSourceNode, { start: number; end: number }>();
  private samples = new Float32Array(1024);
  presentation() {
    const time = this.context?.currentTime ?? 0;
    const playing =
      this.context?.state === 'running' &&
      [...this.sources.values()].some((s) => time >= s.start && time < s.end);
    if (!playing) {
      this.envelope.reset();
      return { playing: false, level: 0 };
    }
    if (this.analyser) {
      this.analyser.getFloatTimeDomainData(this.samples);
      return { playing: true, level: this.envelope.update(this.samples, 1 / 30) };
    }
    return {
      playing: true,
      level: (0.15 + Math.abs(Math.sin(time * 13)) * 0.3) * (this.gain?.gain.value ?? 0),
    };
  }
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
    try {
      this.analyser = context.createAnalyser();
      this.analyser.fftSize = 2048;
      this.gain.connect(this.analyser);
      this.analyser.connect(context.destination);
    } catch {
      this.analyser = null;
      this.gain.connect(context.destination);
    }
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
    this.sources.set(source, { start: at, end: at + buffer.duration });
    source.onended = () => {
      this.sources.delete(source);
      source.disconnect();
    };
    this.end = at + buffer.duration;
  }
  stop() {
    for (const source of this.sources.keys()) {
      source.onended = null;
      try {
        source.stop();
        source.disconnect();
      } catch {
        /* Already ended. */
      }
    }
    this.sources.clear();
    this.analyser?.disconnect();
    this.analyser = null;
    this.envelope.reset();
    if (this.context) void this.context.close().catch(() => {});
    this.context = null;
    this.gain = null;
    this.end = 0;
  }
}
