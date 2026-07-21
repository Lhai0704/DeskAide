import { describe, expect, it } from 'vitest';
import {
  CONTEXT_OPTIONS,
  activeModel,
  contextUnavailableReason,
  type ModelCapabilities,
  type ModelProfile,
} from './model';

const textOnly: ModelCapabilities = {
  supportsText: true,
  supportsImages: false,
  supportsStreaming: true,
  supportsSystemMessage: true,
  maxImages: 0,
  contextWindow: 4096,
};

describe('context capability presentation', () => {
  it('explains that image context is disabled by the model', () => {
    const screenshot = CONTEXT_OPTIONS.find((option) => option.id === 'screenScreenshot');
    expect(screenshot).toBeDefined();
    expect(contextUnavailableReason(screenshot!, textOnly)).toBe('当前模型不支持图片');
  });

  it('does not imply that unimplemented text collection is available', () => {
    const selectedText = CONTEXT_OPTIONS.find((option) => option.id === 'selectedText');
    expect(selectedText).toBeDefined();
    expect(contextUnavailableReason(selectedText!, textOnly)).toBe('上下文采集将在后续阶段接入');
  });

  it('updates the presented capabilities when the active profile changes', () => {
    const profiles = [
      { id: 'text', capabilities: textOnly },
      { id: 'vision', capabilities: { ...textOnly, supportsImages: true } },
    ] as ModelProfile[];
    expect(activeModel(profiles, 'text')?.capabilities.supportsImages).toBe(false);
    expect(activeModel(profiles, 'vision')?.capabilities.supportsImages).toBe(true);
  });
});
