import { describe, expect, it } from 'vitest';
import { SpeechText, spokenText } from './text';

describe('incremental speech text', () => {
  it('speaks sentences early and does not repeat the final response', () => {
    const text = new SpeechText();
    expect(text.append('你好。下一')).toEqual(['你好。']);
    expect(text.append('句！')).toEqual(['下一句！']);
    expect(text.finish('你好。下一句！')).toEqual([]);
  });
  it('handles non-streaming responses and an unfinished final sentence', () => {
    expect(new SpeechText().finish('Hello. 最后的话')).toEqual(['Hello.', '最后的话']);
  });
  it('never exposes code fences or links split across deltas', () => {
    const input =
      '# 标题\n**你好**。\n```js\nsecret();\n```\n[链接文字](https://example.org)！ https://example.org/private\n结束。';
    const text = new SpeechText();
    const output = [
      ...Array.from(input).flatMap((c) => text.append(c)),
      ...text.finish(input),
    ].join('');
    expect(output).toBe('标题你好。链接文字！结束。');
  });
  it('limits long segments by Unicode characters without splitting surrogate pairs', () => {
    const input = '好😀'.repeat(140);
    const text = new SpeechText();
    const output = [...text.append(input), ...text.finish(input)];
    expect(output.join('')).toBe(input);
    expect(output.every((s) => Array.from(s).length <= 120)).toBe(true);
  });
  it('removes numbered lists and keeps inline code readable', () => {
    expect(spokenText('1. 使用 `npm test`。', true)).toBe('使用 npm test。');
  });
  it('does not read an unfinished code block', () => {
    expect(new SpeechText().finish('可读。\n```secret')).toEqual(['可读。']);
  });
  it('preserves Chinese prose after a bare URL without whitespace', () => {
    expect(new SpeechText().finish('访问 https://example.org。然后继续。')).toEqual([
      '访问 。',
      '然后继续。',
    ]);
  });
});
