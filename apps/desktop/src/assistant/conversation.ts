export interface ConversationMessage {
  id: string;
  turnId?: string | null;
  role: 'user' | 'assistant';
  content: string;
  note?: string;
}

export interface ConversationRecord {
  id: string;
  title: string;
  modelProfileId: string;
  messages: ConversationMessage[];
  revision: number;
  createdAtMs: number;
  updatedAtMs: number;
}

export interface ConversationSummary {
  id: string;
  title: string;
  modelProfileId: string;
  messageCount: number;
  revision: number;
  createdAtMs: number;
  updatedAtMs: number;
}
