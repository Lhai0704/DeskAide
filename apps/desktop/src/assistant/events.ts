export type ResponseEvent =
  | { type: 'started'; requestId: string }
  | { type: 'delta'; requestId: string; text: string }
  | {
      type: 'completed';
      requestId: string;
      response: { content: string; finishReason: string };
    }
  | { type: 'failed'; requestId: string; code: string; message: string }
  | { type: 'cancelled'; requestId: string };

export interface ResponseState {
  requestId: string | null;
  content: string;
  status: 'idle' | 'streaming' | 'completed' | 'failed' | 'cancelled';
  error: string;
}

export const initialResponseState = (): ResponseState => ({
  requestId: null,
  content: '',
  status: 'idle',
  error: '',
});

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

export function reduceResponseEvent(state: ResponseState, event: ResponseEvent): ResponseState {
  if (state.requestId && event.requestId !== state.requestId) return state;

  switch (event.type) {
    case 'started':
      return { requestId: event.requestId, content: '', status: 'streaming', error: '' };
    case 'delta':
      return {
        ...state,
        requestId: event.requestId,
        content: state.content + event.text,
        status: 'streaming',
      };
    case 'completed':
      return {
        requestId: event.requestId,
        content: event.response.content,
        status: 'completed',
        error: '',
      };
    case 'failed':
      return {
        ...state,
        requestId: event.requestId,
        status: 'failed',
        error: providerErrorLabel(event.code, event.message),
      };
    case 'cancelled':
      return {
        ...state,
        requestId: event.requestId,
        status: 'cancelled',
        error: '',
      };
  }
}
