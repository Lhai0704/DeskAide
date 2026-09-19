// Reparse the accumulated source, withholding incomplete constructs so the spoken
// prefix never changes when a Markdown delimiter arrives in a later model delta.
export function spokenText(source: string, final = false): string {
  let output = '';
  let i = 0;
  while (i < source.length) {
    const rest = source.slice(i);
    if (i === 0 || source[i - 1] === '\n') {
      const prefix = /^(?: {0,3})(?:#{1,6}\s+|>\s*|[-+*]\s+|\d+\.\s+)/.exec(rest);
      if (prefix) {
        i += prefix[0].length;
        continue;
      }
      if (!final && /^(?: {0,3})(?:#{1,6}|>|[-+*]|\d+\.?)$/.test(rest)) break;
    }
    if (rest.startsWith('```') || rest.startsWith('~~~')) {
      const fence = rest.slice(0, 3);
      const end = source.indexOf(fence, i + 3);
      if (end < 0) break;
      output += '\n';
      i = end + 3;
      continue;
    }
    if (!final && /^(?:`{1,2}|~{1,2})$/.test(rest)) break;
    if (source[i] === '`') {
      const end = source.indexOf('`', i + 1);
      if (end < 0) {
        if (final) output += source.slice(i + 1);
        break;
      }
      output += source.slice(i + 1, end);
      i = end + 1;
      continue;
    }
    if (rest.startsWith('![') || source[i] === '[') {
      const image = rest.startsWith('![');
      const begin = i + (image ? 2 : 1);
      const close = source.indexOf(']', begin);
      if (close < 0 || (!final && close === source.length - 1)) break;
      if (source[close + 1] === '(') {
        let end = close + 2,
          depth = 1;
        for (; end < source.length && depth; end++) {
          if (source[end] === '(') depth++;
          if (source[end] === ')') depth--;
        }
        if (depth) break;
        if (!image) output += source.slice(begin, close);
        i = end;
        continue;
      }
      output += source.slice(begin, close);
      i = close + 1;
      continue;
    }
    if (/^(?:https?:\/\/|www\.)/i.test(rest)) {
      const end = rest.search(/[\s。！？；，、<>]/);
      if (end < 0) break;
      i += end;
      continue;
    }
    if (!final && ['http://', 'https://', 'www.', '!['].some((p) => p.startsWith(rest))) break;
    if (!/[*_~#<>]/.test(source[i])) output += source[i];
    i++;
  }
  return output;
}

export class SpeechText {
  private source = '';
  private consumed = 0;
  append(delta: string): string[] {
    this.source += delta;
    return this.take(false);
  }
  finish(content: string): string[] {
    // Providers normally return the accumulated content; never replay its prefix.
    if (content.startsWith(this.source)) this.source = content;
    else if (!this.source) this.source = content;
    return this.take(true);
  }
  private take(final: boolean): string[] {
    const text = spokenText(this.source, final);
    const result: string[] = [];
    while (this.consumed < text.length) {
      const rest = text.slice(this.consumed);
      const chars = Array.from(rest);
      let count = chars.findIndex(
        (c, i) =>
          /[。！？!?\n；;]/.test(c) ||
          (c === '.' &&
            !/\d/.test(chars[i - 1] ?? '') &&
            (i + 1 < chars.length ? /\s/.test(chars[i + 1]) : final)),
      );
      count = count < 0 ? 0 : count + 1;
      if (!count || count > 120) {
        if (chars.length >= 120) {
          count = 120;
          for (let j = 119; j >= 0; j--)
            if (/[,，、\s]/.test(chars[j])) {
              count = j + 1;
              break;
            }
        } else if (final) count = chars.length;
        else break;
      }
      const part = chars.slice(0, count).join('');
      this.consumed += part.length;
      if (part.trim()) result.push(part.trim());
    }
    return result;
  }
}
