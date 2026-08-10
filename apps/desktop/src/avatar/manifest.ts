import type { AvatarPackManifest } from './types';

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
  manifest: AvatarPackManifest,
  state: keyof AvatarPackManifest['states'],
  packRoot = DEFAULT_AVATAR_PACK_ROOT,
): string {
  return `${packRoot}/${manifest.states[state].asset}`;
}

export function assertManifest(value: unknown): asserts value is AvatarPackManifest {
  if (!isObject(value)) throw new Error('助手形象资源包 manifest 必须是对象');
  const isStaticManifest = value.schemaVersion === 1 && value.renderer === 'static';
  const isVideoManifest = value.schemaVersion === 2 && value.renderer === 'video';
  if (!isStaticManifest && !isVideoManifest) {
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
  if (!isObject(value.states)) throw new Error('助手形象资源包 states 无效');

  for (const stateName of ['idle', 'activated'] as const) {
    const state = value.states[stateName];
    if (!isObject(state) || typeof state.asset !== 'string' || typeof state.alt !== 'string') {
      throw new Error(`助手形象资源包状态 ${stateName} 无效`);
    }
    if (state.asset.includes('..') || state.asset.startsWith('/')) {
      throw new Error(`助手形象资源包状态 ${stateName} 使用了不安全的资源路径`);
    }
  }
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isPositiveNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0;
}
