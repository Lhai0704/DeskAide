import { describe, expect, it } from 'vitest';
import { assertManifest, avatarAssetUrl } from './manifest';
import type { AvatarPackManifest } from './types';

const validManifest: AvatarPackManifest = {
  schemaVersion: 1,
  id: 'default-assistant',
  name: 'Default Assistant',
  version: '1.0.0',
  renderer: 'static',
  defaultWidth: 160,
  defaultHeight: 160,
  states: {
    idle: { asset: 'idle.png', alt: 'Idle' },
    activated: { asset: 'activated.png', alt: 'Activated' },
  },
};

const validVideoManifest: AvatarPackManifest = {
  schemaVersion: 2,
  id: 'video-assistant',
  name: 'Video Assistant',
  version: '1.0.0',
  renderer: 'video',
  defaultWidth: 160,
  defaultHeight: 160,
  states: {
    idle: { asset: 'idle.webm', alt: 'Idle' },
    activated: { asset: 'idle.webm', alt: 'Activated' },
  },
};

describe('Avatar Pack manifest', () => {
  it('accepts the phase-one schema', () => {
    expect(() => assertManifest(validManifest)).not.toThrow();
  });

  it('accepts the video schema', () => {
    expect(() => assertManifest(validVideoManifest)).not.toThrow();
  });

  it('requires renderer and schema version to match', () => {
    const invalid = { ...validVideoManifest, renderer: 'static' };
    expect(() => assertManifest(invalid)).toThrow(/schemaVersion/);
  });

  it('rejects path traversal', () => {
    const invalid = structuredClone(validManifest) as unknown as Record<string, unknown>;
    (invalid.states as Record<string, { asset: string }>).idle.asset = '../secret.png';
    expect(() => assertManifest(invalid)).toThrow(/不安全/);
  });

  it('resolves state assets from the pack root', () => {
    expect(avatarAssetUrl(validManifest, 'activated')).toBe(
      '/avatars/default-assistant/activated.png',
    );
  });
});
