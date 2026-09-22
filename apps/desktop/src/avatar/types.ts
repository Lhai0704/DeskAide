export type AvatarRenderer = 'static' | 'video' | 'live2d' | 'vrm';
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

export type SemanticState = 'idle' | 'activated' | 'thinking' | 'responding' | 'speaking' | 'error';
export interface MotionRef {
  group: string;
  index: number;
}
export interface AvatarPreferences {
  mouseTracking: boolean;
  idleAnimation: boolean;
  autoBlink: boolean;
  motions: boolean;
  scale: number;
  verticalPosition: number;
}
export interface Live2DAvatarPackManifest {
  schemaVersion: 3;
  renderer: 'live2d';
  id: string;
  name: string;
  version: string;
  alt: string;
  preview: string;
  defaultWidth: number;
  defaultHeight: number;
  model: string;
  layout?: {
    scale?: number;
    anchor?: { x: number; y: number };
    position?: { x: number; y: number };
  };
  motions?: Partial<Record<SemanticState, MotionRef>>;
  expressions?: Partial<Record<'neutral' | 'thinking' | 'tap', string>>;
  taps?: Record<string, { motion?: MotionRef; expression?: string }>;
  behavior?: {
    mouseTracking?: boolean;
    idleAnimation?: boolean;
    motions?: boolean;
    blink?: 'auto' | 'model' | 'fallback' | 'off';
  };
  metadata?: { author?: string; license?: string };
}
export interface VrmAvatarPackManifest {
  schemaVersion: 4;
  renderer: 'vrm';
  id: string;
  name: string;
  version: string;
  alt: string;
  preview: string;
  defaultWidth: number;
  defaultHeight: number;
  model: string;
  layout?: { scale?: number };
  motions?: Partial<Record<SemanticState, string>>;
  expressions?: Partial<Record<'neutral' | 'thinking' | 'tap', string>>;
  behavior?: {
    mouseTracking?: boolean;
    idleAnimation?: boolean;
    motions?: boolean;
    blink?: 'auto' | 'model' | 'fallback' | 'off';
  };
  metadata?: { author?: string; license?: string };
}
export interface RendererInput {
  state: SemanticState;
  speakingLevel: number;
  interaction: number;
  cursorFocus: { x: number; y: number } | null;
  preferences: AvatarPreferences;
}
export type MediaAvatarPackManifest = StaticAvatarPackManifest | VideoAvatarPackManifest;
export type AvatarPackManifest =
  MediaAvatarPackManifest | Live2DAvatarPackManifest | VrmAvatarPackManifest;

function posedPack(
  pack?: AvatarPackManifest,
): pack is Live2DAvatarPackManifest | VrmAvatarPackManifest {
  return pack?.renderer === 'live2d' || pack?.renderer === 'vrm';
}

export function avatarUsesPointerMask(renderer: AvatarRenderer | undefined) {
  return renderer === 'live2d' || renderer === 'vrm';
}

export const avatarDefaults = (pack?: AvatarPackManifest): AvatarPreferences => ({
  mouseTracking: posedPack(pack) ? (pack.behavior?.mouseTracking ?? true) : true,
  idleAnimation: posedPack(pack) ? (pack.behavior?.idleAnimation ?? true) : true,
  autoBlink: !posedPack(pack) || pack.behavior?.blink !== 'off',
  motions: posedPack(pack) ? (pack.behavior?.motions ?? true) : true,
  scale: 1,
  verticalPosition: 0,
});
