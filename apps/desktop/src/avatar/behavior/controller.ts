import type { SemanticState } from '../types';
export interface Presentation {
  revision: number;
  turnId: string | null;
  phase: string;
  failed: boolean;
  speech: { sessionId: string; playing: boolean; level: number } | null;
}
export class AvatarBehavior {
  private snapshot: Presentation = {
    revision: -1,
    turnId: null,
    phase: 'terminal',
    failed: false,
    speech: null,
  };
  private action: { state: 'activated' | 'error'; expires: number; token: number } | null = null;
  private token = 0;
  accept(next: Presentation, now: number, recovering = false) {
    if (next.revision <= this.snapshot.revision) return;
    if (next.turnId !== this.snapshot.turnId) this.action = null;
    if (next.failed && !this.snapshot.failed && !recovering) this.flash('error', now);
    this.snapshot = next;
  }
  flash(state: 'activated' | 'error', now: number) {
    const token = ++this.token;
    this.action = { state, expires: now + (state === 'activated' ? 1200 : 1500), token };
    return token;
  }
  finish(token: number) {
    if (this.action?.token === token) this.action = null;
  }
  resetInteraction() {
    this.action = null;
    ++this.token;
  }
  read(now: number): { state: SemanticState; speakingLevel: number; interaction: number } {
    if (this.action && now >= this.action.expires) this.action = null;
    const speaking = this.snapshot.speech?.playing ?? false;
    const state = speaking
      ? 'speaking'
      : (this.action?.state ??
        (this.snapshot.phase === 'responding'
          ? 'responding'
          : this.snapshot.phase !== 'terminal'
            ? 'thinking'
            : 'idle'));
    return {
      state,
      speakingLevel: speaking ? (this.snapshot.speech?.level ?? 0) : 0,
      interaction: this.action?.state === 'activated' ? this.action.token : 0,
    };
  }
}
