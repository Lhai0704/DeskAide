export interface ModelCapabilities {
  supportsText: boolean;
  supportsImages: boolean;
  supportsStreaming: boolean;
  supportsSystemMessage: boolean;
  maxImages: number | null;
  contextWindow: number | null;
}

export type ProviderType = 'mock' | 'openai_compatible';

export interface ModelProfile {
  id: string;
  name: string;
  providerType: ProviderType;
  baseUrl: string;
  modelId: string;
  capabilities: ModelCapabilities;
  maxOutputTokens: number | null;
  timeoutSeconds: number;
  customHeaders: Record<string, string>;
  hasApiKey: boolean;
}

export interface AssistantBootstrap {
  activeModelProfileId: string;
  modelProfiles: ModelProfile[];
}

export type ContextSourceId =
  | 'selectedText'
  | 'webPage'
  | 'activeWindowText'
  | 'activeWindowScreenshot'
  | 'regionScreenshot'
  | 'screenScreenshot';

export interface ContextOption {
  id: ContextSourceId;
  label: string;
  image: boolean;
}

export const CONTEXT_OPTIONS: ContextOption[] = [
  { id: 'selectedText', label: '当前选中文字', image: false },
  { id: 'webPage', label: '当前网页', image: false },
  { id: 'activeWindowText', label: '当前窗口文字', image: false },
  { id: 'activeWindowScreenshot', label: '当前窗口截图', image: true },
  { id: 'regionScreenshot', label: '框选区域截图', image: true },
  { id: 'screenScreenshot', label: '当前屏幕截图', image: true },
];

export function activeModel(
  profiles: ModelProfile[],
  activeModelProfileId: string,
): ModelProfile | null {
  return profiles.find((profile) => profile.id === activeModelProfileId) ?? null;
}

export function contextUnavailableReason(
  option: ContextOption,
  capabilities: ModelCapabilities,
): string {
  if (option.image && !capabilities.supportsImages) return '当前模型不支持图片';
  if (option.image) return '图片采集将在后续阶段接入';
  return '上下文采集将在后续阶段接入';
}
