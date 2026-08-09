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
    expect(isAvatarPackId('female-assistant')).toBe(true);
    expect(isAvatarPackId('unknown')).toBe(false);
  });

  it('loads a stored avatar and otherwise uses the default', () => {
    expect(loadAvatarPackId(storageWith('female-assistant'))).toBe('female-assistant');
    expect(loadAvatarPackId(storageWith('unknown'))).toBe('female-assistant');
    expect(loadAvatarPackId(storageWith(null))).toBe('female-assistant');
  });

  it('persists the selected avatar', () => {
    const storage = storageWith(null);

    saveAvatarPackId('female-assistant', storage);

    expect(storage.setItem).toHaveBeenCalledWith(AVATAR_PACK_STORAGE_KEY, 'female-assistant');
  });

  it('resolves catalog metadata', () => {
    expect(avatarPackById('female-assistant').root).toBe('/avatars/female-assistant');
  });
});
