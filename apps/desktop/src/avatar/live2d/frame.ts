import type { Live2DAvatarPackManifest, AvatarPreferences } from '../types';

/** Draw at least this often. A 1.5ms slack keeps a 60Hz display from falling to 30. */
export const TARGET_FRAME_MS = 1000 / 60;

export function frameDue(now: number, last: number): boolean {
  return !last || now - last >= TARGET_FRAME_MS - 1.5;
}

/**
 * Backing-store pixels per CSS pixel.
 * Match the display to avoid another fractional resampling by the compositor.
 * Extra samples are area-averaged in LineResolve.
 */
export function backingStoreScale(devicePixelRatio: number): number {
  const dpr = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  return Math.min(4, dpr);
}

export interface StageFrame {
  fittedWidth: number;
  fittedHeight: number;
  center: { x: number; y: number };
  sx: number;
  sy: number;
  tx: number;
  ty: number;
}

/**
 * Place the model the way AIRI's stage does.
 * Scale 1 is twice a contain fit, and the model center sits on the bottom edge,
 * so the upper body fills the window. Screen pixels stay square.
 * An omitted layout position is bottom-center; an explicit position is honored.
 */
export function stageFrame(
  view: { width: number; height: number },
  model: { width: number; height: number },
  layout: Live2DAvatarPackManifest['layout'],
  preferences: Pick<AvatarPreferences, 'scale' | 'verticalPosition'>,
): StageFrame | null {
  if (!(view.width > 0) || !(view.height > 0) || !(model.width > 0) || !(model.height > 0))
    return null;
  const ratio = model.width / model.height;
  const userScale = (layout?.scale ?? 1) * preferences.scale;
  if (!(userScale > 0) || !Number.isFinite(userScale) || !Number.isFinite(ratio) || ratio <= 0)
    return null;
  const contain = Math.min(view.height, view.width / ratio);
  const fittedHeight = 2 * contain * userScale;
  const fittedWidth = fittedHeight * ratio;
  const anchor = layout?.anchor ?? { x: 0.5, y: 0.5 };
  const position = layout?.position ?? { x: 0.5, y: 1 };
  const center = {
    x: view.width * position.x + (0.5 - anchor.x) * fittedWidth,
    y: view.height * (position.y + preferences.verticalPosition) + (0.5 - anchor.y) * fittedHeight,
  };
  const sx = fittedHeight / view.width;
  const sy = fittedHeight / view.height;
  // Cubism applies this translation in clip space after the view scale, so it is
  // already in NDC and must not be divided by sx/sy.
  return {
    fittedWidth,
    fittedHeight,
    center,
    sx,
    sy,
    tx: (center.x / view.width) * 2 - 1,
    ty: 1 - (center.y / view.height) * 2,
  };
}
