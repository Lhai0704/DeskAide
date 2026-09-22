export const AVATAR_PACK_STORAGE_KEY = 'deskaide.avatar-pack';
export const AVATAR_PACK_CHANGED_EVENT = 'avatar-pack-changed';
export const DEFAULT_AVATAR_PACK_ID = 'default-assistant' as const;

import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { loadAvatarManifest } from './manifest';
export interface AvatarPack {
  id: string;
  name: string;
  description: string;
  root: string;
  preview?: string;
}
export const AVATAR_PACKS: AvatarPack[] = [
  {
    id: 'default-assistant',
    name: '机器人助手',
    description: '静态图片占位，助手形象待后续规划',
    root: '/avatars/default-assistant',
  },
];

export type AvatarPackId = string;
export const localAssetUrl = (path: string) =>
  `${convertFileSrc('', 'avatar-local')}${path.split('/').map(encodeURIComponent).join('/')}`;
export async function discoverPacks() {
  const result = await invoke<{ directory: string; packs: string[]; runtimeReady: boolean }>(
    'list_local_avatar_packs',
  );
  const local: AvatarPack[] = [];
  for (const name of result.packs) {
    const root = localAssetUrl(`packs/${name}`);
    try {
      const m = await loadAvatarManifest(root);
      if (m.id === 'default-assistant' || local.some((p) => p.id === m.id)) continue;
      local.push({
        id: m.id,
        name: m.name,
        description: '本地形象',
        root,
        preview:
          m.renderer === 'live2d' ? `${root}/${m.preview}` : `${root}/${m.states.idle.asset}`,
      });
    } catch {
      /* Invalid packs cannot enter the catalog. */
    }
  }
  AVATAR_PACKS.splice(1, AVATAR_PACKS.length - 1, ...local);
  return result;
}
export type AvatarPackChangedPayload = { packId: AvatarPackId };

type AvatarStorage = Pick<Storage, 'getItem' | 'setItem'>;

export function isAvatarPackId(value: unknown): value is AvatarPackId {
  return AVATAR_PACKS.some((pack) => pack.id === value);
}

export function avatarPackById(packId: AvatarPackId) {
  return AVATAR_PACKS.find((pack) => pack.id === packId)!;
}

export function loadAvatarPackId(
  storage: AvatarStorage | undefined = browserStorage(),
): AvatarPackId {
  try {
    const stored = storage?.getItem(AVATAR_PACK_STORAGE_KEY);
    return isAvatarPackId(stored) ? stored : DEFAULT_AVATAR_PACK_ID;
  } catch {
    return DEFAULT_AVATAR_PACK_ID;
  }
}

export function saveAvatarPackId(
  packId: AvatarPackId,
  storage: AvatarStorage | undefined = browserStorage(),
) {
  try {
    storage?.setItem(AVATAR_PACK_STORAGE_KEY, packId);
  } catch {
    // Keep the selected avatar for this session when persistent storage is unavailable.
  }
}

function browserStorage(): AvatarStorage | undefined {
  if (typeof window === 'undefined') return undefined;
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}
