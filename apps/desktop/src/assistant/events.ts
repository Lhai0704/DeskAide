export type ResponseEvent =
  | { type: 'started'; requestId: string }
  | { type: 'delta'; requestId: string; text: string }
  | {
      type: 'completed';
      requestId: string;
      response: { content: string; finishReason: string };
    }
  | { type: 'failed'; requestId: string; message: string };

export interface ResponseState {
  requestId: string | null;
  content: string;
  status: 'idle' | 'streaming' | 'completed' | 'failed';
  error: string;
}

export const initialResponseState = (): ResponseState => ({
  requestId: null,
  content: '',
  status: 'idle',
  error: '',
});

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
        error: event.message,
      };
  }
}
