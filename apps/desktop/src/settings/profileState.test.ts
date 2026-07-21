import { describe, expect, it } from 'vitest';
import type { ModelProfile } from '../assistant/model';
import { canDeleteProfile, removeProfile, upsertProfile } from './profileState';

const profile = (id: string, providerType: ModelProfile['providerType'] = 'openai_compatible') =>
  ({ id, providerType, name: id }) as ModelProfile;

describe('profile save and delete state', () => {
  it('adds, updates, and removes profiles without mutating the input', () => {
    const initial = [profile('mock', 'mock')];
    const added = upsertProfile(initial, profile('remote'));
    const updated = upsertProfile(added, { ...profile('remote'), name: 'updated' });
    expect(initial).toHaveLength(1);
    expect(updated.find((item) => item.id === 'remote')?.name).toBe('updated');
    expect(removeProfile(updated, 'remote')).toEqual(initial);
  });

  it('protects the Mock and active profiles from deletion', () => {
    expect(canDeleteProfile(profile('mock', 'mock'), 'remote')).toBe(false);
    expect(canDeleteProfile(profile('remote'), 'remote')).toBe(false);
    expect(canDeleteProfile(profile('other'), 'remote')).toBe(true);
  });
});
