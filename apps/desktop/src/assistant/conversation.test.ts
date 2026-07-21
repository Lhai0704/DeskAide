import { describe, expect, it } from 'vitest';
import { buildModelMessages, type ConversationMessage } from './conversation';

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
});
