/**
 * Markdown — the renderer for the in-panel developer guides.
 *
 * The guides are markdown written for developers: headings, prose, lists,
 * fenced code, inline code, bold, links, pipe tables and blockquotes. The panel
 * needs to read them without a markdown dependency (no new packages), so this
 * file renders exactly that subset and nothing else.
 *
 * Deliberate limits:
 *   - no `dangerouslySetInnerHTML` anywhere: every character is text, so a
 *     guide cannot inject markup into the panel;
 *   - unknown syntax falls through as plain text rather than being dropped, so
 *     a construct this subset does not know still reaches the reader;
 *   - a fenced block keeps its own scroll container and its own copy button —
 *     that is the one interaction a code sample needs in a panel.
 *
 * The visual rules live in `pages/developers.css` (`dev-md-*`), tokens only.
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import Icon from '../Icon';

interface MarkdownProps {
  markdown: string;
  /** Copy label of the code blocks, supplied by the page's catalog. */
  copyLabel: string;
  copiedLabel: string;
  copyBlockedLabel: string;
  copyFailedLabel: string;
}

/* --- inline --------------------------------------------------------------- */

/** One inline token: code, bold or a link. Everything else stays literal. */
type InlineToken =
  | { kind: 'text'; text: string }
  | { kind: 'code'; text: string }
  | { kind: 'bold'; text: string }
  | { kind: 'link'; text: string; href: string };

const INLINE = /(`[^`\n]+`|\*\*[^*\n]+\*\*|\[[^\]\n]+\]\([^)\s]+\))/g;

function tokenizeInline(text: string): InlineToken[] {
  const tokens: InlineToken[] = [];
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    const index = match.index ?? 0;
    if (index > last) tokens.push({ kind: 'text', text: text.slice(last, index) });
    const raw = match[0];
    if (raw.startsWith('`')) {
      tokens.push({ kind: 'code', text: raw.slice(1, -1) });
    } else if (raw.startsWith('**')) {
      tokens.push({ kind: 'bold', text: raw.slice(2, -2) });
    } else {
      const label = raw.slice(1, raw.indexOf(']'));
      const href = raw.slice(raw.indexOf('](') + 2, -1);
      tokens.push({ kind: 'link', text: label, href });
    }
    last = index + raw.length;
  }
  if (last < text.length) tokens.push({ kind: 'text', text: text.slice(last) });
  return tokens;
}

/**
 * Only schemes a guide can legitimately link to are rendered as links.
 * `javascript:` and `data:` are rendered as plain text instead — a guide is
 * content from the server, not a script surface.
 */
function safeHref(href: string): string | null {
  if (href.startsWith('/') || href.startsWith('#')) return href;
  if (/^https?:\/\//i.test(href)) return href;
  return null;
}

function renderInline(text: string, keyPrefix: string): ReactNode[] {
  return tokenizeInline(text).map((token, index) => {
    const key = `${keyPrefix}-${index}`;
    if (token.kind === 'code') return <code key={key} className="dev-md-code-inline">{token.text}</code>;
    if (token.kind === 'bold') return <strong key={key}>{token.text}</strong>;
    if (token.kind === 'link') {
      const href = safeHref(token.href);
      if (!href) return <span key={key}>{token.text}</span>;
      const external = /^https?:\/\//i.test(href);
      return (
        <a
          key={key}
          href={href}
          target={external ? '_blank' : undefined}
          rel={external ? 'noreferrer noopener' : undefined}
        >
          {token.text}
        </a>
      );
    }
    return <span key={key}>{token.text}</span>;
  });
}

/* --- block parsing --------------------------------------------------------- */

type Block =
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'paragraph'; text: string }
  | { kind: 'code'; language: string; text: string }
  | { kind: 'list'; ordered: boolean; items: string[] }
  | { kind: 'quote'; text: string }
  | { kind: 'table'; head: string[]; rows: string[][] }
  | { kind: 'rule' };

const HEADING = /^(#{1,6})\s+(.*)$/;
const FENCE = /^```([\w+#.-]*)\s*$/;
const ORDERED = /^\d+[.)]\s+/;
const UNORDERED = /^[-*+]\s+/;
const RULE = /^(---|\*\*\*|___)\s*$/;

function splitRow(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((cell) => cell.trim());
}

const TABLE_DIVIDER = /^\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?$/;

function parseMarkdown(markdown: string): Block[] {
  const lines = markdown.replace(/\r\n/g, '\n').split('\n');
  const blocks: Block[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index] ?? '';

    if (line.trim() === '') {
      index += 1;
      continue;
    }

    // Fenced code: consume until the closing fence or end of document.
    const fence = line.match(FENCE);
    if (fence) {
      const language = fence[1] ?? '';
      const body: string[] = [];
      index += 1;
      while (index < lines.length && !(lines[index] ?? '').startsWith('```')) {
        body.push(lines[index] ?? '');
        index += 1;
      }
      index += 1; // closing fence
      blocks.push({ kind: 'code', language, text: body.join('\n') });
      continue;
    }

    const heading = line.match(HEADING);
    if (heading) {
      blocks.push({ kind: 'heading', level: heading[1]?.length ?? 1, text: heading[2]?.trim() ?? '' });
      index += 1;
      continue;
    }

    if (RULE.test(line.trim())) {
      blocks.push({ kind: 'rule' });
      index += 1;
      continue;
    }

    if (line.trimStart().startsWith('> ')) {
      const quote: string[] = [];
      while (index < lines.length && (lines[index] ?? '').trimStart().startsWith('> ')) {
        quote.push((lines[index] ?? '').trimStart().slice(2));
        index += 1;
      }
      blocks.push({ kind: 'quote', text: quote.join(' ') });
      continue;
    }

    // Pipe table: a header row followed by a divider row.
    if (line.trim().startsWith('|') && TABLE_DIVIDER.test((lines[index + 1] ?? '').trim())) {
      const head = splitRow(line);
      index += 2;
      const rows: string[][] = [];
      while (index < lines.length && (lines[index] ?? '').trim().startsWith('|')) {
        rows.push(splitRow(lines[index] ?? ''));
        index += 1;
      }
      blocks.push({ kind: 'table', head, rows });
      continue;
    }

    const ordered = ORDERED.test(line.trimStart());
    const unordered = UNORDERED.test(line.trimStart());
    if (ordered || unordered) {
      const items: string[] = [];
      while (index < lines.length) {
        const current = (lines[index] ?? '').trimStart();
        const isItem = ordered ? ORDERED.test(current) : UNORDERED.test(current);
        if (!isItem) break;
        items.push(current.replace(ordered ? ORDERED : UNORDERED, ''));
        index += 1;
      }
      blocks.push({ kind: 'list', ordered, items });
      continue;
    }

    // Paragraph: consecutive plain lines until the next block-level start.
    const paragraph: string[] = [line.trim()];
    index += 1;
    while (index < lines.length) {
      const next = lines[index] ?? '';
      if (
        next.trim() === ''
        || FENCE.test(next)
        || HEADING.test(next)
        || next.trimStart().startsWith('> ')
        || ORDERED.test(next.trimStart())
        || UNORDERED.test(next.trimStart())
        || RULE.test(next.trim())
      ) break;
      paragraph.push(next.trim());
      index += 1;
    }
    blocks.push({ kind: 'paragraph', text: paragraph.join(' ') });
  }

  return blocks;
}

/* --- code block ------------------------------------------------------------ */

function CodeBlock({
  code,
  language,
  copyLabel,
  copiedLabel,
  copyBlockedLabel,
  copyFailedLabel,
}: {
  code: string;
  language: string;
  copyLabel: string;
  copiedLabel: string;
  copyBlockedLabel: string;
  copyFailedLabel: string;
}) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const copy = async () => {
    setError(null);
    if (!navigator.clipboard) {
      setCopied(false);
      setError(copyBlockedLabel);
      return;
    }
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 4000);
    } catch {
      setCopied(false);
      setError(copyFailedLabel);
    }
  };

  return (
    <div className="dev-md-pre">
      <div className="dev-md-pre-head">
        <span className="dev-md-pre-lang">{language || 'text'}</span>
        <button type="button" className="btn btn-quiet btn-sm" onClick={() => { void copy(); }}>
          <Icon name="lucide:copy" size={14} />
          {copied ? copiedLabel : copyLabel}
        </button>
      </div>
      <pre className="dev-md-pre-body"><code>{code}</code></pre>
      {error ? <p className="dev-md-pre-error" role="alert">{error}</p> : null}
    </div>
  );
}

/* --- scrollable table ------------------------------------------------------ */

/**
 * A markdown table is a document, not a record list: stacking its cells would
 * delete the column/row meaning the prose depends on. It keeps its grid and
 * scrolls sideways inside this wrapper instead.
 *
 * The wrapper is focusable so a keyboard can reach the overflow, and it states
 * that it scrolls with a small edge hint — but only while the table really is
 * wider than the column, which is why the overflow is measured rather than
 * assumed. A table that fits gets no hint and no extra padding.
 */
function TableScroller({ label, children }: { label: string; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const [scrollable, setScrollable] = useState(false);

  useEffect(() => {
    const node = ref.current;
    if (!node) return undefined;
    const measure = () => setScrollable(node.scrollWidth > node.clientWidth + 1);
    measure();
    if (typeof ResizeObserver === 'undefined') return undefined;
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  return (
    <div className={`dev-md-table-wrap${scrollable ? ' is-scrollable' : ''}`}>
      <div
        ref={ref}
        className="dev-md-table-scroll"
        tabIndex={0}
        role="group"
        aria-label={label}
      >
        {children}
      </div>
      {scrollable ? <span className="dev-md-table-hint" aria-hidden="true">→</span> : null}
    </div>
  );
}

/* --- renderer -------------------------------------------------------------- */

export default function Markdown({
  markdown,
  copyLabel,
  copiedLabel,
  copyBlockedLabel,
  copyFailedLabel,
}: MarkdownProps) {
  const blocks = useMemo(() => parseMarkdown(markdown), [markdown]);

  return (
    <div className="dev-md">
      {blocks.map((block, index) => {
        const key = `block-${index}`;
        switch (block.kind) {
          case 'heading': {
            const level = Math.min(4, Math.max(2, block.level + 1));
            const Tag = (`h${level}` as 'h2' | 'h3' | 'h4');
            return <Tag key={key} className={`dev-md-h dev-md-h${level}`}>{renderInline(block.text, key)}</Tag>;
          }
          case 'code':
            return (
              <CodeBlock
                key={key}
                code={block.text}
                language={block.language}
                copyLabel={copyLabel}
                copiedLabel={copiedLabel}
                copyBlockedLabel={copyBlockedLabel}
                copyFailedLabel={copyFailedLabel}
              />
            );
          case 'list': {
            const items = block.items.map((item, itemIndex) => (
              <li key={`${key}-${itemIndex}`}>{renderInline(item, `${key}-${itemIndex}`)}</li>
            ));
            return block.ordered
              ? <ol key={key} className="dev-md-list">{items}</ol>
              : <ul key={key} className="dev-md-list">{items}</ul>;
          }
          case 'quote':
            return <blockquote key={key} className="dev-md-quote">{renderInline(block.text, key)}</blockquote>;
          case 'table':
            return (
              <TableScroller key={key} label={block.head.join(' · ')}>
                <table className="data-table dev-md-table" data-mobile="scroll">
                  <thead>
                    <tr>
                      {block.head.map((cell, cellIndex) => (
                        <th key={`${key}-h-${cellIndex}`} scope="col">{renderInline(cell, `${key}-h-${cellIndex}`)}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {block.rows.map((row, rowIndex) => (
                      <tr key={`${key}-r-${rowIndex}`}>
                        {row.map((cell, cellIndex) => (
                          <td key={`${key}-r-${rowIndex}-${cellIndex}`}>{renderInline(cell, `${key}-r-${rowIndex}-${cellIndex}`)}</td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </TableScroller>
            );
          case 'rule':
            return <hr key={key} className="dev-md-rule" />;
          case 'paragraph':
          default:
            return <p key={key} className="dev-md-p">{renderInline(block.text, key)}</p>;
        }
      })}
    </div>
  );
}