import { describe, expect, it } from 'vitest';
import { initialResponseState, reduceResponseEvent } from './events';

describe('response event reducer', () => {
  it('accumulates stream deltas', () => {
    let state = reduceResponseEvent(initialResponseState(), {
      type: 'started',
      requestId: 'one',
    });
    state = reduceResponseEvent(state, { type: 'delta', requestId: 'one', text: 'Desk' });
    state = reduceResponseEvent(state, { type: 'delta', requestId: 'one', text: 'Aide' });
    expect(state.content).toBe('DeskAide');
    expect(state.status).toBe('streaming');
  });

  it('ignores events for another request', () => {
    const state = { ...initialResponseState(), requestId: 'one' };
    expect(reduceResponseEvent(state, { type: 'delta', requestId: 'two', text: 'wrong' })).toBe(
      state,
    );
  });

  it('keeps partial content when generation is cancelled', () => {
    const streaming = {
      ...initialResponseState(),
      requestId: 'one',
      content: 'partial',
      status: 'streaming' as const,
    };
    const cancelled = reduceResponseEvent(streaming, { type: 'cancelled', requestId: 'one' });

    expect(cancelled.content).toBe('partial');
    expect(cancelled.status).toBe('cancelled');
  });

  it('presents provider error categories without discarding details', () => {
    const failed = reduceResponseEvent(initialResponseState(), {
      type: 'failed',
      requestId: 'one',
      code: 'rate_limited',
      message: 'retry after 60 seconds',
    });
    expect(failed.error).toContain('请求过于频繁');
    expect(failed.error).toContain('retry after 60 seconds');
  });
});
