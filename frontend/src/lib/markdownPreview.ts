export type Inline = { text: string; bold: boolean; italic: boolean };

export type PreviewBlock =
  | { kind: 'heading'; level: 1 | 2 | 3 | 4 | 5 | 6; inlines: Inline[] }
  | { kind: 'paragraph'; inlines: Inline[] }
  | {
      kind: 'list';
      ordered: boolean;
      items: { level: number; n?: number; inlines: Inline[] }[];
    }
  | { kind: 'code'; text: string }
  | { kind: 'hr' };

const ESCAPABLE = '\\`*_{}[]()#+-.!|<>~"\'&:$%=^/,;?@';

function pushText(out: Inline[], text: string, bold: boolean, italic: boolean): void {
  if (!text) return;
  const last = out[out.length - 1];
  if (last && last.bold === bold && last.italic === italic) {
    last.text += text;
  } else {
    out.push({ text, bold, italic });
  }
}

/** Index of the next unescaped `marker` at or after `from`, or -1. */
function findClose(s: string, marker: string, from: number): number {
  for (let i = from; i < s.length; i++) {
    if (s[i] === '\\') {
      i++;
      continue;
    }
    if (s.startsWith(marker, i)) {
      // For single `*`, skip a `**` run.
      if (marker === '*' && s[i + 1] === '*') {
        i++;
        continue;
      }
      return i > from ? i : -1;
    }
  }
  return -1;
}

function parseInto(s: string, bold: boolean, italic: boolean, out: Inline[]): void {
  let buf = '';
  let i = 0;
  const flush = () => {
    pushText(out, buf, bold, italic);
    buf = '';
  };
  while (i < s.length) {
    const ch = s[i];
    if (ch === '\\' && i + 1 < s.length && ESCAPABLE.includes(s[i + 1])) {
      buf += s[i + 1];
      i += 2;
      continue;
    }
    if (ch === '*') {
      if (s.startsWith('**', i)) {
        const end = findClose(s, '**', i + 2);
        if (end !== -1) {
          flush();
          parseInto(s.slice(i + 2, end), true, italic, out);
          i = end + 2;
          continue;
        }
      } else {
        const end = findClose(s, '*', i + 1);
        if (end !== -1) {
          flush();
          parseInto(s.slice(i + 1, end), bold, true, out);
          i = end + 1;
          continue;
        }
      }
    }
    buf += ch;
    i++;
  }
  flush();
}

function parseInlines(s: string): Inline[] {
  const out: Inline[] = [];
  parseInto(s, false, false, out);
  return out;
}

const HEADING = /^(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$/;
const HR = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const FENCE = /^ {0,3}(```+|~~~+)/;
const BULLET = /^( *)[-*+][ \t]+(.*)$/;
const ORDERED = /^( *)(\d+)[.)][ \t]+(.*)$/;

export function parseMarkdownPreview(md: string): PreviewBlock[] {
  const lines = md.replace(/\r\n?/g, '\n').split('\n');
  const blocks: PreviewBlock[] = [];
  let para: string[] = [];

  const flushPara = () => {
    if (para.length) {
      blocks.push({ kind: 'paragraph', inlines: parseInlines(para.join(' ')) });
      para = [];
    }
  };

  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    const fence = FENCE.exec(line);
    if (fence) {
      flushPara();
      const marker = fence[1];
      const body: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith(marker)) {
        body.push(lines[i]);
        i++;
      }
      i++; // closing fence (or end of input)
      blocks.push({ kind: 'code', text: body.join('\n') });
      continue;
    }

    if (line.trim() === '') {
      flushPara();
      i++;
      continue;
    }

    const h = HEADING.exec(line);
    if (h) {
      flushPara();
      blocks.push({
        kind: 'heading',
        level: h[1].length as 1 | 2 | 3 | 4 | 5 | 6,
        inlines: parseInlines(h[2]),
      });
      i++;
      continue;
    }

    if (HR.test(line)) {
      flushPara();
      blocks.push({ kind: 'hr' });
      i++;
      continue;
    }

    const bm = BULLET.exec(line);
    const om = bm ? null : ORDERED.exec(line);
    if (bm || om) {
      flushPara();
      const ordered = !!om;
      const items: { level: number; n?: number; inlines: Inline[] }[] = [];
      while (i < lines.length) {
        const b = BULLET.exec(lines[i]);
        const o = b ? null : ORDERED.exec(lines[i]);
        if (ordered ? !o : !b) break;
        if (o) {
          items.push({
            level: Math.floor(o[1].length / 2),
            n: Number(o[2]),
            inlines: parseInlines(o[3]),
          });
        } else if (b) {
          items.push({ level: Math.floor(b[1].length / 2), inlines: parseInlines(b[2]) });
        }
        i++;
      }
      blocks.push({ kind: 'list', ordered, items });
      continue;
    }

    para.push(line.trim());
    i++;
  }
  flushPara();
  return blocks;
}
