import { AnimationClip, VectorKeyframeTrack } from 'three';

/**
 * Shift VRMA position tracks so the hips' first frame matches the model's rest pose.
 * Compared in the bone's local space: the scene may be rotated to face the camera.
 */
export function reAnchorRootPositionTrack(
  clip: AnimationClip,
  hipNodeName: string,
  rest: { x: number; y: number; z: number },
): boolean {
  const hips = clip.tracks.find(
    (track) => track.name === `${hipNodeName}.position` && track instanceof VectorKeyframeTrack,
  );
  if (!(hips instanceof VectorKeyframeTrack) || hips.values.length < 3) return false;
  const dx = hips.values[0] - rest.x;
  const dy = hips.values[1] - rest.y;
  const dz = hips.values[2] - rest.z;
  if (!Number.isFinite(dx + dy + dz)) return false;
  for (const track of clip.tracks) {
    if (!(track instanceof VectorKeyframeTrack) || !track.name.endsWith('.position')) continue;
    for (let i = 0; i < track.values.length; i += 3) {
      track.values[i] -= dx;
      track.values[i + 1] -= dy;
      track.values[i + 2] -= dz;
    }
  }
  return true;
}
