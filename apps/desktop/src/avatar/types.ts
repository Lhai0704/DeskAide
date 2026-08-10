export type AvatarRenderer = 'static' | 'video';
export type AvatarStateName = 'idle' | 'activated';

export interface AvatarState {
  asset: string;
  alt: string;
}

interface AvatarPackManifestBase {
  id: string;
  name: string;
  version: string;
  defaultWidth: number;
  defaultHeight: number;
  states: Record<AvatarStateName, AvatarState>;
}

export interface StaticAvatarPackManifest extends AvatarPackManifestBase {
  schemaVersion: 1;
  renderer: 'static';
}

export interface VideoAvatarPackManifest extends AvatarPackManifestBase {
  schemaVersion: 2;
  renderer: 'video';
}

export type AvatarPackManifest = StaticAvatarPackManifest | VideoAvatarPackManifest;
