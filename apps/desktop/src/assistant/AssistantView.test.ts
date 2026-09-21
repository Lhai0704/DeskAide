// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import type { AssistantEvent } from './events';
import type { ConversationRecord } from './conversation';
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  begin: vi.fn(),
  event: vi.fn(),
  stop: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  emitTo: vi.fn(),
  listen: vi.fn(async (name: string, callback: (event: { payload: unknown }) => void) => {
    mocks.listeners.set(name, callback);
    return () => mocks.listeners.delete(name);
  }),
}));
vi.mock('../speech/controller', () => ({
  defaultSpeechSettings: () => ({ enabled: false }),
  SpeechController: class {
    begin = mocks.begin;
    event = mocks.event;
    stop = mocks.stop;
    dispose = vi.fn();
    volume = vi.fn();
  },
}));
import AssistantView from './AssistantView.svelte';
interface Input {
  conversationId: string;
  turnId: string;
  expectedRevision: number;
  prompt: string;
  contextDrafts: unknown[];
}
const record: ConversationRecord = {
  id: 'saved',
  title: '历史标题',
  modelProfileId: 'mock',
  revision: 4,
  createdAtMs: 1,
  updatedAtMs: 2,
  messages: [
    { id: 'old-u', role: 'user', content: '历史问题' },
    { id: 'old-a', role: 'assistant', content: '历史回复' },
  ],
};
let submitted: Input[] = [];
function emit(event: AssistantEvent) {
  mocks.listeners.get('assistant-event')!({ payload: event });
}
function envelope(input: Input, sequence: number) {
  return {
    version: 1 as const,
    conversationId: input.conversationId,
    turnId: input.turnId,
    sequence,
  };
}
beforeEach(() => {
  submitted = [];
  mocks.listeners.clear();
  mocks.begin.mockClear();
  mocks.event.mockClear();
  mocks.stop.mockClear();
  mocks.invoke
    .mockReset()
    .mockImplementation(async (command: string, args: Record<string, unknown> = {}) => {
      if (command === 'get_assistant_bootstrap')
        return {
          activeModelProfileId: 'mock',
          modelProfiles: [
            {
              id: 'mock',
              name: 'Mock',
              providerType: 'mock',
              capabilities: { supportsText: true, supportsTools: false },
            },
          ],
        };
      if (command === 'get_speech_settings') return { enabled: false };
      if (command === 'submit_turn') {
        submitted.push(args.input as Input);
        return;
      }
      if (command === 'get_turn_snapshot') return null;
      if (command === 'load_conversation')
        return args.conversationId === 'saved'
          ? record
          : {
              ...record,
              id: args.conversationId,
              revision: 2,
              messages: [{ id: 'u', role: 'user', content: 'first' }],
            };
      if (command === 'list_conversation_summaries') return [{ ...record, messageCount: 2 }];
    });
});
describe('assistant runtime integration', () => {
  it('resubscribes to an active runtime turn without replaying its stored messages', async () => {
    const previous = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation(async (command: string, args: Record<string, unknown> = {}) => {
      if (command === 'get_active_turn_snapshot' || command === 'get_turn_snapshot')
        return {
          conversationId: 'saved',
          turnId: 'restored',
          sequence: 6,
          content: '本轮已生成文字',
          status: 'running',
          approval: null,
          revision: 4,
          error: null,
        };
      if (command === 'load_conversation')
        return {
          ...record,
          messages: [
            ...record.messages,
            { id: 'step', turnId: 'restored', role: 'assistant', content: '本轮已生成文字' },
          ],
        };
      return previous(command, args);
    });
    const view = render(AssistantView);
    await waitFor(() => expect(view.getAllByText('本轮已生成文字')).toHaveLength(1));
    expect(view.getByText('历史回复')).toBeTruthy();
    expect(mocks.begin).not.toHaveBeenCalled();
    expect(mocks.event).not.toHaveBeenCalled();
    await waitFor(() =>
      expect((view.getByTitle('新建会话') as HTMLButtonElement).disabled).toBe(false),
    );
    await fireEvent.click(view.getByTitle('新建会话'));
    await waitFor(() => expect(view.queryByText('本轮已生成文字')).toBeNull());
    expect(mocks.invoke).toHaveBeenCalledWith('cancel_turn', { turnId: 'restored' });
  });
  it('replaces a generation only after cancelling it and ignores late events', async () => {
    const view = render(AssistantView);
    await waitFor(() => expect(view.getByText('Mock')).toBeTruthy());
    const input = view.getByPlaceholderText('现在需要我帮你做什么？');
    await fireEvent.input(input, { target: { value: 'first' } });
    await fireEvent.click(view.getByRole('button', { name: '发送' }));
    await waitFor(() => expect(submitted).toHaveLength(1));
    const first = submitted[0];
    emit({ ...envelope(first, 1), type: 'turnStarted' });
    await fireEvent.input(input, { target: { value: 'second' } });
    await fireEvent.click(view.getByRole('button', { name: '发送并替换' }));
    await waitFor(() => expect(submitted).toHaveLength(2));
    const calls = mocks.invoke.mock.calls.map((c) => c[0]);
    expect(calls.indexOf('cancel_turn')).toBeLessThan(calls.lastIndexOf('submit_turn'));
    expect(submitted[1].expectedRevision).toBe(2);
    expect(submitted[1]).not.toHaveProperty('messages');
    const count = mocks.event.mock.calls.length;
    emit({ ...envelope(first, 2), type: 'textDelta', messageId: 'old', text: '迟到污染' });
    expect(mocks.event).toHaveBeenCalledTimes(count);
    expect(view.queryByText('迟到污染')).toBeNull();
    const second = submitted[1];
    emit({ ...envelope(second, 1), type: 'turnStarted' });
    emit({ ...envelope(second, 2), type: 'textDelta', messageId: 'new', text: '新的回答' });
    await waitFor(() => expect(view.getByText('新的回答')).toBeTruthy());
  });
  it('cancels an active turn when loading history and never reads historical text aloud', async () => {
    const view = render(AssistantView);
    await waitFor(() => expect(view.getByText('Mock')).toBeTruthy());
    await fireEvent.input(view.getByPlaceholderText('现在需要我帮你做什么？'), {
      target: { value: 'first' },
    });
    await fireEvent.click(view.getByRole('button', { name: '发送' }));
    await waitFor(() => expect(submitted).toHaveLength(1));
    await fireEvent.click(view.getByTitle('历史对话'));
    await fireEvent.click(await view.findByText('历史标题'));
    await waitFor(() => expect(view.getByText('历史回复')).toBeTruthy());
    expect(mocks.invoke).toHaveBeenCalledWith('cancel_turn', { turnId: submitted[0].turnId });
    expect(mocks.begin).toHaveBeenCalledTimes(1);
    expect(mocks.event).not.toHaveBeenCalled();
    emit({ ...envelope(submitted[0], 1), type: 'textDelta', messageId: 'old', text: '迟到' });
    expect(mocks.event).not.toHaveBeenCalled();
    expect(view.queryByText('迟到')).toBeNull();
  });
  it('recovers an event gap without speaking a partial or recovered snapshot', async () => {
    const view = render(AssistantView);
    await waitFor(() => expect(view.getByText('Mock')).toBeTruthy());
    await fireEvent.input(view.getByPlaceholderText('现在需要我帮你做什么？'), {
      target: { value: 'first' },
    });
    await fireEvent.click(view.getByRole('button', { name: '发送' }));
    await waitFor(() => expect(submitted).toHaveLength(1));
    const first = submitted[0];
    mocks.invoke.mockImplementation(async (command: string) =>
      command === 'get_turn_snapshot'
        ? {
            ...envelope(first, 3),
            content: '完整快照',
            status: 'running',
            approval: null,
            revision: 1,
            error: null,
          }
        : undefined,
    );
    emit({ ...envelope(first, 3), type: 'textDelta', messageId: 'm', text: '后半段' });
    await waitFor(() => expect(view.getByText('完整快照')).toBeTruthy());
    expect(mocks.event).not.toHaveBeenCalled();
    expect(view.queryByText('后半段')).toBeNull();
  });
});
