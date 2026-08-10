import { describe, expect, it } from 'vitest';
import {
  CONTEXT_OPTIONS,
  activeModel,
  contextExcerpt,
  contextResultNote,
  contextUnavailableReason,
  windowLabel,
  type ModelCapabilities,
  type ModelProfile,
  type TargetWindow,
} from './model';

const textOnly: ModelCapabilities = {
  supportsText: true,
  supportsImages: false,
  supportsStreaming: true,
  supportsSystemMessage: true,
  maxImages: 0,
  contextWindow: 4096,
};

const target: TargetWindow = {
  id: 'window-1',
  applicationName: 'Editor',
  processName: 'editor.exe',
  title: 'Document',
};

describe('context capability presentation', () => {
  it('explains image capability and implementation limits', () => {
    const screenshot = CONTEXT_OPTIONS.find((option) => option.id === 'screenScreenshot');
    expect(contextUnavailableReason(screenshot!, textOnly, target)).toBe('当前模型不支持图片');
    expect(
      contextUnavailableReason(screenshot!, { ...textOnly, supportsImages: true }, target),
    ).toBe('图片采集将在后续阶段接入');
  });

  it('enables implemented text context only when an external target exists', () => {
    const selectedText = CONTEXT_OPTIONS.find((option) => option.id === 'selectedText');
    const webPage = CONTEXT_OPTIONS.find((option) => option.id === 'webPage');
    expect(contextUnavailableReason(selectedText!, textOnly, target)).toBeNull();
    expect(contextUnavailableReason(selectedText!, textOnly, null)).toBe(
      '未记录到本次激活前的外部窗口',
    );
    expect(contextUnavailableReason(webPage!, textOnly, target)).toBe('浏览器扩展将在后续阶段接入');
  });

  it('updates the presented capabilities when the active profile changes', () => {
    const profiles = [
      { id: 'text', capabilities: textOnly },
      { id: 'vision', capabilities: { ...textOnly, supportsImages: true } },
    ] as ModelProfile[];
    expect(activeModel(profiles, 'text')?.capabilities.supportsImages).toBe(false);
    expect(activeModel(profiles, 'vision')?.capabilities.supportsImages).toBe(true);
  });

  it('formats successful and unavailable collection results for a message note', () => {
    expect(
      contextResultNote([
        {
          source: 'selectedText',
          status: 'added',
          characterCount: 12,
          truncated: true,
          message: '已添加',
        },
        {
          source: 'activeWindowText',
          status: 'unavailable',
          characterCount: 0,
          truncated: false,
          message: '目标未公开文字',
        },
      ]),
    ).toBe('当前选中文字：已添加 12 字（已截断）；当前窗口文字：目标未公开文字');
  });

  it('creates stable labels and compact one-line context excerpts', () => {
    expect(windowLabel(target)).toBe('Document');
    expect(contextExcerpt(' first\n\nsecond   third ', 14)).toBe('first second t…');
  });
});
