export const AVATAR_PACK_STORAGE_KEY = 'deskaide.avatar-pack';
export const AVATAR_PACK_CHANGED_EVENT = 'avatar-pack-changed';
export const DEFAULT_AVATAR_PACK_ID = 'default-assistant' as const;

export const AVATAR_PACKS = [
  {
    id: 'default-assistant',
    name: '机器人助手',
    description: '静态图片占位，助手形象待后续规划',
    root: '/avatars/default-assistant',
  },
] as const;

export type AvatarPackId = (typeof AVATAR_PACKS)[number]['id'];
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
