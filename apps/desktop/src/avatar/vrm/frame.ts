/** Vertical field of view used by the desktop stage. Matches the reference VRM viewer. */
export const VRM_FOV_DEG = 30;

export interface VrmBounds {
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
  minZ: number;
  maxZ: number;
}

export interface VrmCameraFrame {
  fov: number;
  distance: number;
  target: { x: number; y: number; z: number };
  position: { x: number; y: number; z: number };
}

/**
 * Frame a standing model so scale 1 shows the upper body in a portrait window.
 * The camera sits on +Z; VRM 1 faces that way, and VRM 0 is rotated to match.
 * Positive verticalPosition raises the look target, which lowers the body on screen.
 */
export function vrmCameraFrame(
  bounds: VrmBounds,
  view: { width: number; height: number },
  layoutScale: number,
  preferences: { scale: number; verticalPosition: number },
): VrmCameraFrame | null {
  const height = bounds.maxY - bounds.minY;
  const width = Math.max(0, bounds.maxX - bounds.minX);
  const userScale = layoutScale * preferences.scale;
  if (
    !(view.width > 0) ||
    !(view.height > 0) ||
    !(height > 0) ||
    !(userScale > 0) ||
    !Number.isFinite(height + width + userScale + preferences.verticalPosition)
  )
    return null;
  // Vertical span only. A T-pose's arm width must not pull the camera back to a full-body shot.
  const framedSpan = (height * 0.62) / userScale;
  const vFov = (VRM_FOV_DEG * Math.PI) / 180;
  const distance = framedSpan / 2 / Math.tan(vFov / 2);
  const targetY = bounds.minY + height * 0.76 + preferences.verticalPosition * height * 0.35;
  const x = (bounds.minX + bounds.maxX) / 2;
  const z = (bounds.minZ + bounds.maxZ) / 2;
  return {
    fov: VRM_FOV_DEG,
    distance,
    target: { x, y: targetY, z },
    position: { x, y: targetY, z: z + distance },
  };
}

/** MToon at the 640×900 window cap is too heavy past 2×. */
export function vrmPixelRatio(devicePixelRatio: number) {
  const dpr = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  return Math.min(2, dpr);
}
