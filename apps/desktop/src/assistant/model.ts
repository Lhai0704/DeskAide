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

export interface TargetWindow {
  id: string;
  applicationName: string | null;
  processName: string | null;
  title: string | null;
}

export interface AssistantShownPayload {
  target: TargetWindow | null;
  warning: string | null;
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

export type ContextCollectionStatus = 'added' | 'unavailable' | 'failed';

export interface ContextCollectionResult {
  source: ContextSourceId;
  status: ContextCollectionStatus;
  characterCount: number;
  truncated: boolean;
  message: string;
}

export interface SubmitModelRequestResult {
  requestId: string;
  contextResults: ContextCollectionResult[];
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
  target: TargetWindow | null,
): string | null {
  if (!option.image && !capabilities.supportsText) return '当前模型不支持文字';
  if (option.image && !capabilities.supportsImages) return '当前模型不支持图片';
  if (option.image) return '图片采集将在后续阶段接入';
  if (option.id === 'webPage') return '浏览器扩展将在后续阶段接入';
  if (!target) return '未记录到本次激活前的外部窗口';
  return null;
}

export function contextSourceLabel(source: ContextSourceId): string {
  return CONTEXT_OPTIONS.find((option) => option.id === source)?.label ?? source;
}

export function contextResultNote(results: ContextCollectionResult[]): string | undefined {
  if (results.length === 0) return undefined;
  return results
    .map((result) => {
      const label = contextSourceLabel(result.source);
      if (result.status === 'added') {
        return `${label}：已添加 ${result.characterCount} 字${result.truncated ? '（已截断）' : ''}`;
      }
      return `${label}：${result.message}`;
    })
    .join('；');
}
