import { describe, it, expect } from 'vitest';
import { AvatarBehavior, type Presentation } from './controller';
const snapshot = (
  revision: number,
  phase = 'terminal',
  turnId: string | null = 't',
): Presentation => ({ revision, phase, turnId, failed: false, speech: null });
describe('avatar authoritative behavior', () => {
  it('maps turns, tools and real playback without revealing text', () => {
    const b = new AvatarBehavior();
    expect(b.read(0).state).toBe('idle');
    for (const [n, phase, state] of [
      [1, 'preparing', 'thinking'],
      [2, 'responding', 'responding'],
      [3, 'approval', 'thinking'],
      [4, 'tool', 'thinking'],
      [5, 'responding', 'responding'],
    ] as const) {
      b.accept(snapshot(n, phase), 0);
      expect(b.read(0).state).toBe(state);
    }
    b.accept(
      { ...snapshot(6, 'responding'), speech: { sessionId: 's', playing: true, level: 0.4 } },
      0,
    );
    expect(b.read(0)).toMatchObject({ state: 'speaking', speakingLevel: 0.4 });
    b.accept(snapshot(7, 'responding'), 0);
    expect(b.read(0).state).toBe('responding');
    b.accept(snapshot(8), 0);
    expect(b.read(0).state).toBe('idle');
  });
  it('short actions return to the latest state and stale timers cannot clear a newer action', () => {
    const b = new AvatarBehavior();
    b.accept(snapshot(1, 'preparing'), 0);
    const old = b.flash('activated', 0);
    b.accept(snapshot(2, 'responding'), 10);
    expect(b.read(100).state).toBe('activated');
    expect(b.read(1300).state).toBe('responding');
    b.flash('activated', 1400);
    b.finish(old);
    expect(b.read(1401).state).toBe('activated');
    b.accept(snapshot(3, 'preparing', 'new'), 1402);
    expect(b.read(1403).state).toBe('thinking');
    b.accept(snapshot(2, 'responding'), 1404);
    expect(b.read(1404).state).toBe('thinking');
  });
  it('does not replay an error on history restoration', () => {
    const b = new AvatarBehavior();
    b.accept({ ...snapshot(1), failed: true }, 0, true);
    expect(b.read(0).state).toBe('idle');
    b.accept(snapshot(2, 'preparing', 'new'), 0);
    b.accept({ ...snapshot(3, 'terminal', 'new'), failed: true }, 10);
    expect(b.read(10).state).toBe('error');
    expect(b.read(1600).state).toBe('idle');
  });
});
