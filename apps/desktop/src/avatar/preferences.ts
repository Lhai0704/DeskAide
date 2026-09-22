import { invoke } from '@tauri-apps/api/core';
import { avatarDefaults, type AvatarPackManifest, type AvatarPreferences } from './types';
export interface AvatarSettings {
  packId: string | null;
  preferences: Record<string, AvatarPreferences>;
}
export async function loadSettings(): Promise<AvatarSettings> {
  return invoke('get_avatar_settings');
}
export async function persistSettings(value: AvatarSettings) {
  await invoke('save_avatar_settings', { value });
}
export function preferencesFor(
  settings: AvatarSettings,
  pack: AvatarPackManifest,
): AvatarPreferences {
  const defaults = avatarDefaults(pack);
  const p = settings.preferences?.[pack.id];
  if (!p) return defaults;
  for (const key of ['mouseTracking', 'idleAnimation', 'autoBlink', 'motions'] as const)
    if (typeof p[key] === 'boolean') defaults[key] = p[key];
  if (Number.isFinite(p.scale)) defaults.scale = Math.max(0.5, Math.min(2, p.scale));
  if (Number.isFinite(p.verticalPosition))
    defaults.verticalPosition = Math.max(-0.4, Math.min(0.4, p.verticalPosition));
  return defaults;
}
