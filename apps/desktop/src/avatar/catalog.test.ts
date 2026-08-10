import { describe, expect, it, vi } from 'vitest';
import {
  AVATAR_PACK_STORAGE_KEY,
  avatarPackById,
  isAvatarPackId,
  loadAvatarPackId,
  saveAvatarPackId,
} from './catalog';

function storageWith(value: string | null) {
  return {
    getItem: vi.fn(() => value),
    setItem: vi.fn(),
  };
}

describe('avatar pack preferences', () => {
  it('accepts only avatars in the catalog', () => {
    expect(isAvatarPackId('default-assistant')).toBe(true);
    expect(isAvatarPackId('legacy-pack-a')).toBe(true);
    expect(isAvatarPackId('legacy-pack-b')).toBe(true);
    expect(isAvatarPackId('unknown')).toBe(false);
  });

  it('loads a stored avatar and otherwise uses the default', () => {
    expect(loadAvatarPackId(storageWith('legacy-pack-a'))).toBe('legacy-pack-a');
    expect(loadAvatarPackId(storageWith('unknown'))).toBe('legacy-pack-b');
    expect(loadAvatarPackId(storageWith(null))).toBe('legacy-pack-b');
  });

  it('persists the selected avatar', () => {
    const storage = storageWith(null);

    saveAvatarPackId('legacy-pack-a', storage);

    expect(storage.setItem).toHaveBeenCalledWith(AVATAR_PACK_STORAGE_KEY, 'legacy-pack-a');
  });

  it('resolves catalog metadata', () => {
    expect(avatarPackById('legacy-pack-a').root).toBe('/avatars/legacy-pack-a');
    expect(avatarPackById('legacy-pack-b').root).toBe(
      '/avatars/legacy-pack-b',
    );
  });
});
