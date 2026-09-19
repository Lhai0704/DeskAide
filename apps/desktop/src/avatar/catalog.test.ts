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
    expect(isAvatarPackId('legacy-pack-a')).toBe(false);
    expect(isAvatarPackId('legacy-pack-b')).toBe(false);
    expect(isAvatarPackId('unknown')).toBe(false);
  });

  it('loads a stored avatar and otherwise uses the default', () => {
    expect(loadAvatarPackId(storageWith('default-assistant'))).toBe('default-assistant');
    expect(loadAvatarPackId(storageWith('unknown'))).toBe('default-assistant');
    expect(loadAvatarPackId(storageWith(null))).toBe('default-assistant');
  });

  it.each(['legacy-pack-a', 'legacy-pack-b'])(
    'falls back to the static placeholder for the old %s preference',
    (previousPackId) => {
      expect(loadAvatarPackId(storageWith(previousPackId))).toBe('default-assistant');
    },
  );

  it('uses the placeholder when storage is unavailable', () => {
    expect(
      loadAvatarPackId({
        getItem: () => {
          throw new Error('Storage unavailable');
        },
        setItem: vi.fn(),
      }),
    ).toBe('default-assistant');
  });

  it('persists the selected avatar', () => {
    const storage = storageWith(null);

    saveAvatarPackId('default-assistant', storage);

    expect(storage.setItem).toHaveBeenCalledWith(AVATAR_PACK_STORAGE_KEY, 'default-assistant');
  });

  it('resolves catalog metadata', () => {
    expect(avatarPackById('default-assistant').root).toBe('/avatars/default-assistant');
  });
});
