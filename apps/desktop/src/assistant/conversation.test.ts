import { describe, expect, it } from 'vitest';
import {
  buildModelMessages,
  hasSavableConversation,
  responseToConversationMessage,
  type ConversationMessage,
} from './conversation';

describe('model conversation assembly', () => {
  it('preserves the complete multi-turn role order', () => {
    const messages: ConversationMessage[] = [
      { id: '1', role: 'user', content: 'first' },
      { id: '2', role: 'assistant', content: 'answer' },
      { id: '3', role: 'user', content: 'follow-up' },
    ];
    expect(buildModelMessages(messages)).toEqual([
      { role: 'user', content: [{ type: 'text', text: 'first' }] },
      { role: 'assistant', content: [{ type: 'text', text: 'answer' }] },
      { role: 'user', content: [{ type: 'text', text: 'follow-up' }] },
    ]);
  });

  it('does not send empty UI messages or presentation notes', () => {
    expect(
      buildModelMessages([
        { id: '1', role: 'assistant', content: '  ', note: 'failed' },
        { id: '2', role: 'user', content: 'hello', note: 'UI only' },
      ]),
    ).toEqual([{ role: 'user', content: [{ type: 'text', text: 'hello' }] }]);
  });

  it('only persists conversations after a real user message exists', () => {
    expect(hasSavableConversation([])).toBe(false);
    expect(hasSavableConversation([{ id: '1', role: 'assistant', content: 'welcome' }])).toBe(
      false,
    );
    expect(hasSavableConversation([{ id: '2', role: 'user', content: ' hello ' }])).toBe(true);
  });

  it('archives completed, stopped, and failed responses for history', () => {
    expect(
      responseToConversationMessage(
        { requestId: '1', content: 'done', status: 'completed', error: '' },
        'answer',
      ),
    ).toEqual({ id: 'answer', role: 'assistant', content: 'done', note: undefined });
    expect(
      responseToConversationMessage(
        { requestId: '2', content: 'partial', status: 'cancelled', error: '' },
        'stopped',
      ),
    ).toEqual({ id: 'stopped', role: 'assistant', content: 'partial', note: '已停止' });
    expect(
      responseToConversationMessage(
        { requestId: '3', content: '', status: 'failed', error: 'offline' },
        'failed',
      ),
    ).toEqual({
      id: 'failed',
      role: 'assistant',
      content: '',
      note: '生成失败：offline',
    });
  });
});
