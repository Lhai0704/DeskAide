import type { ResponseState } from './events';

export interface ConversationMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  note?: string;
}

export interface ConversationRecord {
  id: string;
  title: string;
  modelProfileId: string;
  messages: ConversationMessage[];
  createdAtMs: number;
  updatedAtMs: number;
}

export interface ConversationSummary {
  id: string;
  title: string;
  modelProfileId: string;
  messageCount: number;
  createdAtMs: number;
  updatedAtMs: number;
}

export interface SaveConversationInput {
  id: string;
  title: string | null;
  modelProfileId: string;
  messages: ConversationMessage[];
}

export interface ModelMessage {
  role: 'user' | 'assistant';
  content: Array<{ type: 'text'; text: string }>;
}

export function buildModelMessages(messages: ConversationMessage[]): ModelMessage[] {
  return messages
    .filter((message) => message.content.trim().length > 0)
    .map((message) => ({
      role: message.role,
      content: [{ type: 'text' as const, text: message.content }],
    }));
}

export function hasSavableConversation(messages: ConversationMessage[]): boolean {
  return messages.some((message) => message.role === 'user' && message.content.trim().length > 0);
}

export function responseToConversationMessage(
  response: ResponseState,
  id: string,
): ConversationMessage | null {
  const note =
    response.status === 'cancelled'
      ? '已停止'
      : response.status === 'failed'
        ? `生成失败：${response.error}`
        : undefined;
  if (!response.content && !note) return null;
  return {
    id,
    role: 'assistant',
    content: response.content,
    note,
  };
}
