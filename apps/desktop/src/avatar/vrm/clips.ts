import type { SemanticState, VrmAvatarPackManifest } from '../types';

export interface VrmClipChoice {
  name: SemanticState;
  loop: boolean;
}

/**
 * Prefer the clip mapped to the current state. Activated and error play once.
 * `suppressOnce` keeps a finished one-shot from restarting while that state is still showing.
 * Otherwise fall back to the idle clip when idle animation is on.
 */
export function selectVrmClip(
  state: SemanticState,
  motionsOn: boolean,
  idleOn: boolean,
  available: ReadonlySet<string>,
  suppressOnce: SemanticState | null = null,
): VrmClipChoice | null {
  const held = suppressOnce !== null && state === suppressOnce;
  if (!held && motionsOn && state !== 'idle' && available.has(state)) {
    const once = state === 'activated' || state === 'error';
    return { name: state, loop: !once };
  }
  if (idleOn && available.has('idle')) return { name: 'idle', loop: true };
  return null;
}

export function semanticExpression(
  state: SemanticState,
  interaction: number,
  expressions: VrmAvatarPackManifest['expressions'],
): string | undefined {
  if (interaction) return expressions?.tap;
  if (state === 'thinking' || state === 'responding') return expressions?.thinking;
  return expressions?.neutral;
}
