import type { ContextCollectionResult } from './model';
export interface ToolCall {
  id: string;
  name: string;
  arguments: string;
}
export interface ToolDefinition {
  displayName?: string;
  id: string;
  name: string;
  description: string;
  parameters: unknown;
  source: { type: 'internal' } | { type: 'mcp'; serverId: string; serverName: string };
  risk: string;
  revision: number;
}
export interface ToolApproval {
  approvalId: string;
  call: ToolCall;
  definition: ToolDefinition;
  sensitiveContext: boolean;
}
export type AssistantEvent = {
  version: 1;
  conversationId: string;
  turnId: string;
  sequence: number;
} & (
  | { type: 'turnStarted' }
  | { type: 'contextPrepared'; results: ContextCollectionResult[] }
  | { type: 'messageStarted'; messageId: string; modelStep: number }
  | { type: 'textDelta' | 'reasoningDelta'; messageId: string; text: string }
  | { type: 'messageCompleted'; messageId: string; content: string }
  | { type: 'toolProposed'; call: ToolCall }
  | { type: 'toolApprovalRequired'; approval: ToolApproval }
  | { type: 'toolStarted' | 'toolCompleted'; toolCallId: string }
  | { type: 'toolFailed'; toolCallId: string; code: string }
  | {
      type: 'usage';
      modelStep: number;
      usage: {
        inputTokens: number | null;
        outputTokens: number | null;
        totalTokens: number | null;
      };
    }
  | { type: 'turnCompleted' | 'turnCancelled'; revision: number }
  | { type: 'turnFailed'; revision: number; code: string; message: string }
  | { type: 'warning'; code: string; message: string }
);
export interface ResponseState {
  turnId: string | null;
  conversationId: string | null;
  sequence: number;
  content: string;
  status: 'idle' | 'streaming' | 'approval' | 'tool' | 'completed' | 'failed' | 'cancelled';
  error: string;
  approval: ToolApproval | null;
  activity: string;
  revision: number;
}
export interface TurnSnapshot {
  phase?: 'preparing' | 'generating' | 'responding' | 'tool' | 'approval' | 'terminal';
  conversationId: string;
  turnId: string;
  sequence: number;
  content: string;
  status: 'running' | 'completed' | 'failed' | 'cancelled';
  approval: ToolApproval | null;
  revision: number;
  error: { code: string; message: string } | null;
}
export const initialResponseState = (
  conversationId: string | null = null,
  turnId: string | null = null,
): ResponseState => ({
  conversationId,
  turnId,
  sequence: 0,
  content: '',
  status: turnId ? 'streaming' : 'idle',
  error: '',
  approval: null,
  activity: '',
  revision: 0,
});
export function isTerminal(status: ResponseState['status']): boolean {
  return ['completed', 'failed', 'cancelled'].includes(status);
}
export function providerErrorLabel(code: string, fallback: string): string {
  const labels: Record<string, string> = {
    api_key_missing: 'API Key 未配置',
    invalid_base_url: 'Base URL 无效',
    model_not_found: '模型不存在',
    authentication_error: 'API Key 无效或无权访问（401）',
    permission_error: 'API Key 权限或额度不足（402/403）',
    rate_limited: '请求过于频繁，请稍后重试（429）',
    provider_server_error: 'Provider 服务暂时不可用',
    network_error: '无法连接到 Provider',
    timeout: '请求超时',
    incompatible_response: 'Provider 响应格式不兼容',
    stream_interrupted: '流式响应意外中断',
  };
  return labels[code] ? `${labels[code]}：${fallback}` : fallback;
}

export function reduceResponseEvent(state: ResponseState, event: AssistantEvent): ResponseState {
  if (
    event.version !== 1 ||
    !state.turnId ||
    state.turnId !== event.turnId ||
    state.conversationId !== event.conversationId ||
    event.sequence <= state.sequence ||
    isTerminal(state.status)
  )
    return state;
  const next = { ...state, sequence: event.sequence };
  switch (event.type) {
    case 'turnStarted':
      return { ...next, status: 'streaming', activity: '正在准备' };
    case 'textDelta':
      return { ...next, content: next.content + event.text, status: 'streaming', activity: '' };
    case 'messageStarted':
      return { ...next, status: 'streaming', activity: '正在生成' };
    case 'toolProposed':
      return { ...next, activity: `工具：${event.call.name}` };
    case 'toolApprovalRequired':
      return { ...next, status: 'approval', approval: event.approval, activity: '等待你的批准' };
    case 'toolStarted':
      return { ...next, status: 'tool', approval: null, activity: '正在执行工具' };
    case 'toolCompleted':
      return { ...next, status: 'streaming', approval: null, activity: '工具执行完成，正在继续' };
    case 'toolFailed':
      return {
        ...next,
        status: 'streaming',
        approval: null,
        activity: `工具未完成：${event.code}`,
      };
    case 'turnCompleted':
      return {
        ...next,
        status: 'completed',
        approval: null,
        revision: event.revision,
        activity: '',
      };
    case 'turnCancelled':
      return {
        ...next,
        status: 'cancelled',
        approval: null,
        revision: event.revision,
        activity: '',
      };
    case 'turnFailed':
      return {
        ...next,
        status: 'failed',
        approval: null,
        revision: event.revision,
        error: providerErrorLabel(event.code, event.message),
        activity: '',
      };
    case 'warning':
      return { ...next, activity: event.message };
    default:
      return next;
  }
}
export function restoreSnapshot(state: ResponseState, snapshot: TurnSnapshot): ResponseState {
  if (
    state.turnId !== snapshot.turnId ||
    state.conversationId !== snapshot.conversationId ||
    snapshot.sequence <= state.sequence ||
    isTerminal(state.status)
  )
    return state;
  return {
    ...state,
    sequence: snapshot.sequence,
    content: snapshot.content,
    approval: snapshot.approval,
    revision: snapshot.revision,
    status:
      snapshot.status === 'running'
        ? snapshot.approval
          ? 'approval'
          : 'streaming'
        : snapshot.status,
    error: snapshot.error?.message ?? '',
  };
}
