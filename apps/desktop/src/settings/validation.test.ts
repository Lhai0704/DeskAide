import { describe, expect, it } from 'vitest';
import {
  newProfileDraft,
  parseCustomHeaders,
  toProfilePayload,
  validateModelProfile,
} from './validation';

describe('model profile validation', () => {
  it('normalizes a valid OpenAI-compatible profile', () => {
    const draft = newProfileDraft();
    Object.assign(draft, {
      name: 'LongCat',
      baseUrl: 'https://api.longcat.chat/openai/',
      modelId: 'LongCat-2.0',
      customHeadersText: 'X-App: DeskAide',
    });
    const payload = toProfilePayload(draft);
    expect(payload.baseUrl).toBe('https://api.longcat.chat/openai');
    expect(payload.customHeaders).toEqual({ 'X-App': 'DeskAide' });
    expect(payload.apiKey).toBeNull();
  });

  it('rejects invalid URLs, token limits and timeouts', () => {
    const draft = newProfileDraft();
    draft.name = 'Bad';
    draft.modelId = 'model';
    draft.baseUrl = 'file:///tmp/api';
    draft.maxOutputTokens = 0;
    draft.timeoutSeconds = 0;
    expect(validateModelProfile(draft)).toHaveLength(3);
  });

  it('rejects credential-bearing custom headers', () => {
    expect(() => parseCustomHeaders('Authorization: Bearer secret')).toThrow('敏感字段');
    expect(() => parseCustomHeaders('malformed')).toThrow('Name: Value');
  });

  it('rejects credentials embedded in the base URL', () => {
    const draft = newProfileDraft();
    Object.assign(draft, {
      name: 'unsafe',
      modelId: 'model',
      baseUrl: 'https://user:secret@example.com/v1',
    });
    expect(validateModelProfile(draft)).toContain('Base URL 不能包含凭据、查询参数或片段');
  });
});
