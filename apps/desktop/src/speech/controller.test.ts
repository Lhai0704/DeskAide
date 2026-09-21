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
    presentation = () => ({ playing: mocks.buffered > 0, level: 0 });
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
    controller.begin(settings, 'r');
    controller.event({
      version: 1,
      conversationId: 'c',
      turnId: 'r',
      sequence: 1,
      messageId: 'm',
      type: 'textDelta',
      text: '一。二。',
    });
    controller.event({
      version: 1,
      conversationId: 'c',
      turnId: 'r',
      sequence: 2,
      messageId: 'm',
      type: 'messageCompleted',
      content: '一。二。',
    });
    controller.event({
      version: 1,
      conversationId: 'c',
      turnId: 'r',
      sequence: 3,
      type: 'turnCompleted',
      revision: 1,
    });
    await vi.advanceTimersByTimeAsync(1);
    expect(
      mocks.invoke.mock.calls.filter((c) => c[0] === 'speak_segment').map((c) => c[1].input.text),
    ).toEqual(['一。', '二。']);
  });
  it('waits for buffered playback before requesting another segment', async () => {
    mocks.buffered = 16;
    controller.begin(settings, 'r');
    controller.event({
      version: 1,
      conversationId: 'c',
      turnId: 'r',
      sequence: 1,
      messageId: 'm',
      type: 'textDelta',
      text: '一句。',
    });
    await vi.advanceTimersByTimeAsync(500);
    expect(mocks.invoke.mock.calls.filter((c) => c[0] === 'speak_segment')).toHaveLength(0);
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
    controller.begin(settings, 'r');
    controller.event({
      version: 1,
      conversationId: 'c',
      turnId: 'r',
      sequence: 1,
      messageId: 'm',
      type: 'textDelta',
      text: '一句。',
    });
    await vi.advanceTimersByTimeAsync(1);
    const args = mocks.invoke.mock.calls.find((c) => c[0] === 'speak_segment')![1];
    controller.stop();
    args.onEvent.onmessage({
      ...args.input,
      event: { type: 'audio', pcm: 'AAAAAA==', sample_rate: 24000 },
    });
    controller.event({
      version: 1,
      conversationId: 'c',
      turnId: 'r',
      sequence: 1,
      messageId: 'm',
      type: 'textDelta',
      text: '不要读。',
    });
    release();
    await vi.advanceTimersByTimeAsync(1);
    expect(mocks.append).not.toHaveBeenCalled();
    expect(mocks.invoke.mock.calls.filter((c) => c[0] === 'speak_segment')).toHaveLength(1);
  });
  it('reads each model message once and ignores reasoning, tools and stale turns', async () => {
    controller.begin(settings, 'r');
    const base = { version: 1 as const, conversationId: 'c', turnId: 'r', sequence: 1 };
    controller.event({ ...base, type: 'messageStarted', messageId: 'm1', modelStep: 1 });
    controller.event({ ...base, type: 'textDelta', messageId: 'm1', text: '先查询' });
    controller.event({ ...base, type: 'reasoningDelta', messageId: 'm1', text: '不要朗读推理。' });
    controller.event({ ...base, type: 'messageCompleted', messageId: 'm1', content: '先查询' });
    controller.event({
      ...base,
      type: 'toolProposed',
      call: { id: 't', name: 'echo', arguments: '{"text":"不要朗读参数"}' },
    });
    controller.event({ ...base, type: 'messageStarted', messageId: 'm2', modelStep: 2 });
    controller.event({ ...base, type: 'messageCompleted', messageId: 'm2', content: '完成。' });
    controller.event({
      ...base,
      turnId: 'old',
      type: 'textDelta',
      messageId: 'old',
      text: '迟到。',
    });
    controller.event({ ...base, type: 'turnCompleted', revision: 1 });
    await vi.advanceTimersByTimeAsync(1);
    expect(
      mocks.invoke.mock.calls.filter((c) => c[0] === 'speak_segment').map((c) => c[1].input.text),
    ).toEqual(['先查询', '完成。']);
  });
  it('a new question owns the audio session and cancelling a turn stops it', async () => {
    let release!: () => void;
    mocks.invoke.mockImplementation((command: string) =>
      command === 'speak_segment'
        ? new Promise<void>((resolve) => {
            release = resolve;
          })
        : Promise.resolve(),
    );
    controller.begin(settings, 'old');
    controller.event({
      version: 1,
      conversationId: 'c',
      turnId: 'old',
      sequence: 1,
      type: 'messageCompleted',
      messageId: 'old',
      content: '旧问题。',
    });
    await vi.advanceTimersByTimeAsync(1);
    const args = mocks.invoke.mock.calls.find((c) => c[0] === 'speak_segment')![1];
    controller.begin(settings, 'new');
    args.onEvent.onmessage({
      ...args.input,
      event: { type: 'audio', pcm: 'AAAAAA==', sample_rate: 24000 },
    });
    release();
    controller.event({
      version: 1,
      conversationId: 'c',
      turnId: 'new',
      sequence: 1,
      type: 'turnCancelled',
      revision: 1,
    });
    controller.event({
      version: 1,
      conversationId: 'c',
      turnId: 'new',
      sequence: 2,
      type: 'textDelta',
      messageId: 'new',
      text: '不要播报。',
    });
    await vi.advanceTimersByTimeAsync(1);
    expect(mocks.append).not.toHaveBeenCalled();
    expect(mocks.invoke.mock.calls.filter((c) => c[0] === 'speak_segment')).toHaveLength(1);
    expect(mocks.invoke.mock.calls.every((c) => c[0] !== 'cancel_turn')).toBe(true);
  });
});
