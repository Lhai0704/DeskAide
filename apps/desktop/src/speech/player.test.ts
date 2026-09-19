import { afterEach, describe, expect, it, vi } from 'vitest';
import { SpeechPlayer } from './player';

describe('PCM playback', () => {
  afterEach(() => vi.unstubAllGlobals());
  it('schedules chunks on a shared clock and ignores chunks after stop', async () => {
    const starts: number[] = [];
    const close = vi.fn(async () => {});
    vi.stubGlobal(
      'AudioContext',
      class {
        state = 'running';
        currentTime = 1;
        destination = {};
        resume = async () => {};
        close = close;
        createGain = () => ({ gain: { value: 0 }, connect: () => {} });
        createBuffer = (_channels: number, samples: number, rate: number) => ({
          duration: samples / rate,
          copyToChannel: () => {},
        });
        createBufferSource = () => ({
          buffer: null,
          connect: () => {},
          disconnect: () => {},
          start: (at: number) => starts.push(at),
        });
      },
    );
    const player = new SpeechPlayer();
    await player.start(0.8);
    const pcm = btoa(String.fromCharCode(...new Uint8Array(new Float32Array(240).buffer)));
    player.append(pcm, 24000);
    player.append(pcm, 24000);
    expect(starts[0]).toBeCloseTo(1.15);
    expect(starts[1]).toBeCloseTo(1.16);
    expect(player.buffered).toBeCloseTo(0.17);
    expect(() => player.append('AA==', 24000)).toThrow('无效音频片段');
    player.stop();
    player.append(pcm, 24000);
    expect(close).toHaveBeenCalledOnce();
    expect(starts).toHaveLength(2);
  });
});
