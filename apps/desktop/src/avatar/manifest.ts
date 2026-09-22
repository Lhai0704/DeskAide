import type { AvatarPackManifest, MediaAvatarPackManifest } from './types';

export const DEFAULT_AVATAR_PACK_ROOT = '/avatars/default-assistant';

export async function loadAvatarManifest(
  packRoot = DEFAULT_AVATAR_PACK_ROOT,
  fetcher: typeof fetch = fetch,
): Promise<AvatarPackManifest> {
  const response = await fetcher(`${packRoot}/manifest.json`);
  if (!response.ok) {
    throw new Error(`助手形象资源包 manifest 加载失败（HTTP ${response.status}）`);
  }

  const manifest: unknown = await response.json();
  assertManifest(manifest);
  return manifest;
}

export function avatarAssetUrl(
  manifest: MediaAvatarPackManifest,
  state: keyof MediaAvatarPackManifest['states'],
  packRoot = DEFAULT_AVATAR_PACK_ROOT,
): string {
  return `${packRoot}/${manifest.states[state].asset}`;
}

export function assertManifest(value: unknown): asserts value is AvatarPackManifest {
  if (!isObject(value)) throw new Error('助手形象资源包 manifest 必须是对象');
  const isStaticManifest = value.schemaVersion === 1 && value.renderer === 'static';
  const isVideoManifest = value.schemaVersion === 2 && value.renderer === 'video';
  const isLive2D = value.schemaVersion === 3 && value.renderer === 'live2d';
  const isVrm = value.schemaVersion === 4 && value.renderer === 'vrm';
  if (!isStaticManifest && !isVideoManifest && !isLive2D && !isVrm) {
    throw new Error('不支持的助手形象资源包 schemaVersion 或 renderer');
  }

  for (const key of ['id', 'name', 'version'] as const) {
    if (typeof value[key] !== 'string' || value[key].trim() === '') {
      throw new Error(`助手形象资源包字段 ${key} 无效`);
    }
  }

  if (!isPositiveNumber(value.defaultWidth) || !isPositiveNumber(value.defaultHeight)) {
    throw new Error('助手形象资源包默认尺寸无效');
  }
  if (isLive2D) {
    assertLive2D(value);
    return;
  }
  if (isVrm) {
    assertVrm(value);
    return;
  }
  if (!isObject(value.states)) throw new Error('助手形象资源包 states 无效');

  for (const stateName of ['idle', 'activated'] as const) {
    const state = value.states[stateName];
    if (!isObject(state) || typeof state.asset !== 'string' || typeof state.alt !== 'string') {
      throw new Error(`助手形象资源包状态 ${stateName} 无效`);
    }
    safeAssetPath(state.asset);
  }
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function safeAssetPath(value: unknown): asserts value is string {
  if (
    typeof value !== 'string' ||
    !value ||
    /[\\\\:%?#]/.test(value) ||
    [...value].some((c) => c.charCodeAt(0) < 32) ||
    value.startsWith('/') ||
    value.split('/').some((p) => !p || p === '.' || p === '..')
  )
    throw new Error('不安全的资源路径');
}
function motion(value: unknown) {
  if (
    !isObject(value) ||
    typeof value.group !== 'string' ||
    !value.group.trim() ||
    !Number.isInteger(value.index) ||
    (value.index as number) < 0
  )
    throw new Error('无效 motion mapping');
}
function assertLive2D(v: Record<string, unknown>) {
  safeAssetPath(v.model);
  safeAssetPath(v.preview);
  if (!v.model.endsWith('.model3.json') || typeof v.alt !== 'string')
    throw new Error('无效 Live2D model/alt');
  if ((v.defaultWidth as number) > 4096 || (v.defaultHeight as number) > 4096)
    throw new Error('无效 Live2D 尺寸');
  if (v.layout !== undefined) {
    if (!isObject(v.layout)) throw new Error('无效 layout');
    if (v.layout.scale !== undefined && (!isPositiveNumber(v.layout.scale) || v.layout.scale > 3))
      throw new Error('无效 scale');
    for (const key of ['anchor', 'position'])
      if (v.layout[key] !== undefined) {
        const p = v.layout[key];
        if (
          !isObject(p) ||
          ![p.x, p.y].every((n) => typeof n === 'number' && Number.isFinite(n) && n >= 0 && n <= 1)
        )
          throw new Error('无效布局坐标');
      }
  }
  if (v.motions !== undefined) {
    if (!isObject(v.motions)) throw new Error('无效 motions');
    for (const [key, val] of Object.entries(v.motions)) {
      if (!['idle', 'activated', 'thinking', 'responding', 'speaking', 'error'].includes(key))
        throw new Error('无效语义状态');
      motion(val);
    }
  }
  if (v.expressions !== undefined) {
    if (!isObject(v.expressions)) throw new Error('无效 expressions');
    for (const [key, val] of Object.entries(v.expressions))
      if (!['neutral', 'thinking', 'tap'].includes(key) || typeof val !== 'string' || !val)
        throw new Error('无效 expression mapping');
  }
  if (v.behavior !== undefined) {
    if (!isObject(v.behavior)) throw new Error('无效 behavior');
    for (const key of ['mouseTracking', 'idleAnimation', 'motions'])
      if (v.behavior[key] !== undefined && typeof v.behavior[key] !== 'boolean')
        throw new Error('无效 behavior 开关');
    if (
      v.behavior.blink !== undefined &&
      !['auto', 'model', 'fallback', 'off'].includes(v.behavior.blink as string)
    )
      throw new Error('无效 blink');
  }
  if (v.taps !== undefined) {
    if (!isObject(v.taps)) throw new Error('无效 taps');
    for (const item of Object.values(v.taps)) {
      if (!isObject(item)) throw new Error('无效 tap');
      if (item.motion !== undefined) motion(item.motion);
      if (item.expression !== undefined && typeof item.expression !== 'string')
        throw new Error('无效 tap expression');
    }
  }
  if (
    v.metadata !== undefined &&
    (!isObject(v.metadata) || Object.values(v.metadata).some((x) => typeof x !== 'string'))
  )
    throw new Error('无效 metadata');
}

function hasExtension(value: string, extension: string) {
  return value.toLowerCase().endsWith(extension);
}

function assertVrm(v: Record<string, unknown>) {
  safeAssetPath(v.model);
  safeAssetPath(v.preview);
  if (typeof v.model !== 'string' || !hasExtension(v.model, '.vrm') || typeof v.alt !== 'string')
    throw new Error('无效 VRM model/alt');
  if ((v.defaultWidth as number) > 4096 || (v.defaultHeight as number) > 4096)
    throw new Error('无效 VRM 尺寸');
  if (v.layout !== undefined) {
    if (!isObject(v.layout) || Object.keys(v.layout).some((key) => key !== 'scale'))
      throw new Error('VRM layout 只支持 scale');
    if (v.layout.scale !== undefined && (!isPositiveNumber(v.layout.scale) || v.layout.scale > 3))
      throw new Error('无效 scale');
  }
  if (v.motions !== undefined) {
    if (!isObject(v.motions)) throw new Error('无效 motions');
    for (const [key, val] of Object.entries(v.motions)) {
      if (!['idle', 'activated', 'thinking', 'responding', 'speaking', 'error'].includes(key))
        throw new Error('无效语义状态');
      safeAssetPath(val);
      if (!hasExtension(val, '.vrma')) throw new Error('VRM 动作必须是 .vrma');
    }
  }
  if (v.expressions !== undefined) {
    if (!isObject(v.expressions)) throw new Error('无效 expressions');
    for (const [key, val] of Object.entries(v.expressions))
      if (!['neutral', 'thinking', 'tap'].includes(key) || typeof val !== 'string' || !val.trim())
        throw new Error('无效 expression mapping');
  }
  if (v.behavior !== undefined) {
    if (!isObject(v.behavior)) throw new Error('无效 behavior');
    for (const key of ['mouseTracking', 'idleAnimation', 'motions'])
      if (v.behavior[key] !== undefined && typeof v.behavior[key] !== 'boolean')
        throw new Error('无效 behavior 开关');
    if (
      v.behavior.blink !== undefined &&
      !['auto', 'model', 'fallback', 'off'].includes(v.behavior.blink as string)
    )
      throw new Error('无效 blink');
  }
  if (
    v.metadata !== undefined &&
    (!isObject(v.metadata) || Object.values(v.metadata).some((x) => typeof x !== 'string'))
  )
    throw new Error('无效 metadata');
}

function isPositiveNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0;
}
