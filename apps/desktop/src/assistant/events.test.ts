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
});
