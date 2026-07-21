export type AvatarRenderer = 'static';
export type AvatarStateName = 'idle' | 'activated';

export interface AvatarState {
  asset: string;
  alt: string;
}

export interface AvatarPackManifest {
  schemaVersion: 1;
  id: string;
  name: string;
  version: string;
  renderer: AvatarRenderer;
  defaultWidth: number;
  defaultHeight: number;
  states: Record<AvatarStateName, AvatarState>;
}
