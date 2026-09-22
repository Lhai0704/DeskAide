import { describe, it, expect } from 'vitest';
import {
  initialResponseState,
  reduceResponseEvent,
  restoreSnapshot,
  type AssistantEvent,
} from './events';
const header = { version: 1 as const, conversationId: 'c', turnId: 't', sequence: 1 };
describe('assistant turn isolation', () => {
  it('streams text and keeps partial text on cancellation', () => {
    let s = initialResponseState('c', 't');
    s = reduceResponseEvent(s, { ...header, type: 'textDelta', messageId: 'm', text: 'hello' });
    s = reduceResponseEvent(s, { ...header, sequence: 2, type: 'turnCancelled', revision: 3 });
    expect(s.content).toBe('hello');
    expect(s.status).toBe('cancelled');
    expect(
      reduceResponseEvent(s, {
        ...header,
        sequence: 3,
        type: 'textDelta',
        messageId: 'm',
        text: 'late',
      }),
    ).toBe(s);
  });
  it('rejects stale turns, other conversations, duplicates, and unsolicited idle events', () => {
    const s = initialResponseState('c', 't');
    const e: AssistantEvent = { ...header, type: 'textDelta', messageId: 'm', text: 'bad' };
    expect(reduceResponseEvent(s, { ...e, turnId: 'old' })).toBe(s);
    expect(reduceResponseEvent(s, { ...e, conversationId: 'other' })).toBe(s);
    const idle = initialResponseState();
    expect(reduceResponseEvent(idle, e)).toBe(idle);
    const next = reduceResponseEvent(s, e);
    expect(reduceResponseEvent(next, e)).toBe(next);
  });
  it('does not display reasoning as assistant text and preserves provider errors', () => {
    let s = initialResponseState('c', 't');
    s = reduceResponseEvent(s, {
      ...header,
      type: 'reasoningDelta',
      messageId: 'm',
      text: 'private reasoning',
    });
    expect(s.content).toBe('');
    s = reduceResponseEvent(s, {
      ...header,
      sequence: 2,
      type: 'turnFailed',
      revision: 2,
      code: 'rate_limited',
      message: 'try later',
    });
    expect(s.error).toContain('429');
  });
  it('recovers a newer snapshot without accepting another turn', () => {
    const s = initialResponseState('c', 't');
    const snapshot = {
      conversationId: 'c',
      turnId: 't',
      sequence: 8,
      content: 'saved',
      status: 'completed' as const,
      approval: null,
      revision: 4,
      error: null,
    };
    expect(restoreSnapshot(s, snapshot).content).toBe('saved');
    expect(restoreSnapshot(s, { ...snapshot, turnId: 'old' })).toBe(s);
  });
});
