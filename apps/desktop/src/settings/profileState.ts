import type { ModelProfile } from '../assistant/model';

export function upsertProfile(profiles: ModelProfile[], saved: ModelProfile): ModelProfile[] {
  const index = profiles.findIndex((profile) => profile.id === saved.id);
  if (index < 0) return [...profiles, saved];
  return profiles.map((profile) => (profile.id === saved.id ? saved : profile));
}

export function removeProfile(profiles: ModelProfile[], profileId: string): ModelProfile[] {
  return profiles.filter((profile) => profile.id !== profileId);
}

export function canDeleteProfile(profile: ModelProfile, activeProfileId: string): boolean {
  return profile.providerType !== 'mock' && profile.id !== activeProfileId;
}
