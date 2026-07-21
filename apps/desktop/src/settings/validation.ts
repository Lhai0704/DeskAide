import type { ModelCapabilities, ModelProfile, ProviderType } from '../assistant/model';

export interface ModelProfileDraft {
  id: string | null;
  name: string;
  providerType: ProviderType;
  baseUrl: string;
  modelId: string;
  capabilities: ModelCapabilities;
  maxOutputTokens: number | null;
  timeoutSeconds: number;
  customHeadersText: string;
  apiKey: string;
}

export interface ModelProfilePayload {
  id: string | null;
  name: string;
  providerType: ProviderType;
  baseUrl: string;
  modelId: string;
  capabilities: ModelCapabilities;
  maxOutputTokens: number | null;
  timeoutSeconds: number;
  customHeaders: Record<string, string>;
  apiKey: string | null;
}

function isSensitiveHeader(name: string): boolean {
  const normalized = name.toLowerCase();
  return ['authorization', 'api-key', 'apikey', 'token', 'secret', 'cookie'].some((part) =>
    normalized.includes(part),
  );
}

export function newProfileDraft(): ModelProfileDraft {
  return {
    id: null,
    name: '',
    providerType: 'openai_compatible',
    baseUrl: '',
    modelId: '',
    capabilities: {
      supportsText: true,
      supportsImages: false,
      supportsStreaming: true,
      supportsSystemMessage: true,
      maxImages: null,
      contextWindow: 32768,
    },
    maxOutputTokens: 4096,
    timeoutSeconds: 60,
    customHeadersText: '',
    apiKey: '',
  };
}

export function profileToDraft(profile: ModelProfile): ModelProfileDraft {
  return {
    id: profile.id,
    name: profile.name,
    providerType: profile.providerType,
    baseUrl: profile.baseUrl,
    modelId: profile.modelId,
    capabilities: structuredClone(profile.capabilities),
    maxOutputTokens: profile.maxOutputTokens,
    timeoutSeconds: profile.timeoutSeconds,
    customHeadersText: Object.entries(profile.customHeaders)
      .map(([name, value]) => `${name}: ${value}`)
      .join('\n'),
    apiKey: '',
  };
}

export function parseCustomHeaders(value: string): Record<string, string> {
  const headers: Record<string, string> = {};
  for (const [index, rawLine] of value.split(/\r?\n/).entries()) {
    const line = rawLine.trim();
    if (!line) continue;
    const separator = line.indexOf(':');
    if (separator < 1) throw new Error(`自定义 Header 第 ${index + 1} 行格式应为 Name: Value`);
    const name = line.slice(0, separator).trim();
    const headerValue = line.slice(separator + 1).trim();
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name) || !headerValue) {
      throw new Error(`自定义 Header 第 ${index + 1} 行无效`);
    }
    if (isSensitiveHeader(name)) {
      throw new Error(`自定义 Header 不能包含敏感字段 ${name}`);
    }
    headers[name] = headerValue;
  }
  return headers;
}

export function validateModelProfile(draft: ModelProfileDraft): string[] {
  const errors: string[] = [];
  if (!draft.name.trim()) errors.push('请输入 Profile 名称');
  if (!draft.modelId.trim()) errors.push('请输入 Model ID');
  try {
    const url = new URL(draft.baseUrl.trim());
    if (!['http:', 'https:'].includes(url.protocol)) errors.push('Base URL 必须使用 HTTP 或 HTTPS');
    if (url.username || url.password || url.search || url.hash)
      errors.push('Base URL 不能包含凭据、查询参数或片段');
  } catch {
    errors.push('请输入有效的 Base URL');
  }
  if (
    !Number.isInteger(draft.timeoutSeconds) ||
    draft.timeoutSeconds < 1 ||
    draft.timeoutSeconds > 600
  )
    errors.push('超时时间必须是 1–600 秒');
  const contextWindow = draft.capabilities.contextWindow;
  if (contextWindow !== null && (!Number.isInteger(contextWindow) || contextWindow <= 0))
    errors.push('上下文长度必须大于 0');
  if (
    draft.maxOutputTokens !== null &&
    (!Number.isInteger(draft.maxOutputTokens) || draft.maxOutputTokens <= 0)
  )
    errors.push('最大输出长度必须大于 0');
  try {
    parseCustomHeaders(draft.customHeadersText);
  } catch (cause) {
    errors.push(cause instanceof Error ? cause.message : String(cause));
  }
  return errors;
}

export function toProfilePayload(draft: ModelProfileDraft): ModelProfilePayload {
  const errors = validateModelProfile(draft);
  if (errors.length) throw new Error(errors[0]);
  return {
    id: draft.id,
    name: draft.name.trim(),
    providerType: draft.providerType,
    baseUrl: draft.baseUrl.trim().replace(/\/+$/, ''),
    modelId: draft.modelId.trim(),
    capabilities: draft.capabilities,
    maxOutputTokens: draft.maxOutputTokens,
    timeoutSeconds: draft.timeoutSeconds,
    customHeaders: parseCustomHeaders(draft.customHeadersText),
    apiKey: draft.apiKey.trim() || null,
  };
}
