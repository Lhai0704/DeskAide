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

describe('Avatar Pack manifest', () => {
  it('accepts the phase-one schema', () => {
    expect(() => assertManifest(validManifest)).not.toThrow();
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
