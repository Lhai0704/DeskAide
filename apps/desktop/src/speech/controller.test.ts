import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), append: vi.fn(), stop: vi.fn(), buffered: 0 }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
  Channel: class {
    onmessage = () => {};
  },
}));
vi.mock('./player', () => ({
  SpeechPlayer: class {
    get buffered() {
      return mocks.buffered;
    }
    start = async () => {};
    append = mocks.append;
    stop = mocks.stop;
    setVolume = () => {};
  },
}));
import { SpeechController, defaultSpeechSettings } from './controller';

describe('speech lifecycle', () => {
  let controller: SpeechController;
  const settings = { ...defaultSpeechSettings(), enabled: true, reference: 'voice.wav' };
  beforeEach(() => {
    vi.useFakeTimers();
    mocks.buffered = 0;
    mocks.invoke.mockReset().mockResolvedValue(undefined);
    mocks.append.mockClear();
    controller = new SpeechController(vi.fn());
  });
  afterEach(() => {
    controller.dispose();
    vi.useRealTimers();
  });
  it('serializes segments and does not repeat completion', async () => {
    controller.begin(settings);
    controller.event({ type: 'delta', requestId: 'r', text: '一。二。' });
    controller.event({
      type: 'completed',
      requestId: 'r',
      response: { content: '一。二。', finishReason: 'stop' },
    });
    await vi.advanceTimersByTimeAsync(1);
    expect(
      mocks.invoke.mock.calls.filter((c) => c[0] === 'speak_segment').map((c) => c[1].input.text),
    ).toEqual(['一。', '二。']);
  });
  it('waits for buffered playback before requesting another segment', async () => {
    mocks.buffered = 16;
    controller.begin(settings);
    controller.event({ type: 'delta', requestId: 'r', text: '一句。' });
    await vi.advanceTimersByTimeAsync(500);
    expect(mocks.invoke).not.toHaveBeenCalled();
    mocks.buffered = 0;
    await vi.advanceTimersByTimeAsync(100);
    expect(mocks.invoke).toHaveBeenCalledWith('speak_segment', expect.anything());
  });
  it('drops late audio after stopping and does not restart for further deltas', async () => {
    let release!: () => void;
    mocks.invoke.mockImplementation((command: string) =>
      command === 'speak_segment'
        ? new Promise<void>((resolve) => {
            release = resolve;
          })
        : Promise.resolve(),
    );
    controller.begin(settings);
    controller.event({ type: 'delta', requestId: 'r', text: '一句。' });
    await vi.advanceTimersByTimeAsync(1);
    const args = mocks.invoke.mock.calls.find((c) => c[0] === 'speak_segment')![1];
    controller.stop();
    args.onEvent.onmessage({
      ...args.input,
      event: { type: 'audio', pcm: 'AAAAAA==', sample_rate: 24000 },
    });
    controller.event({ type: 'delta', requestId: 'r', text: '不要读。' });
    release();
    await vi.advanceTimersByTimeAsync(1);
    expect(mocks.append).not.toHaveBeenCalled();
    expect(mocks.invoke.mock.calls.filter((c) => c[0] === 'speak_segment')).toHaveLength(1);
  });
});
