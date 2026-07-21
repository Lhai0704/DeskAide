export interface ConversationMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  note?: string;
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
