import { Fragment, type ReactNode } from "react";
import { isExternalUrl } from "../../lib/external";
import { ExternalLink } from "../ExternalLink";

/**
 * Minimal, safe Markdown renderer for release notes. It builds React elements only: raw HTML in
 * the notes is shown as literal text, links are limited to https (opened in the system browser) and images are dropped.
 */

const INLINE =
  /(`[^`\n]+`)|(\*\*[^*\n]+\*\*|__[^_\n]+__)|(\*[^*\s][^*\n]*\*|_[^_\s][^_\n]*_)|(!?\[[^\]\n]*\]\([^)\s]*\))|(https?:\/\/[^\s<>()]+)/g;

export function renderInline(text: string, keyBase = "i"): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  let n = 0;
  for (const m of text.matchAll(INLINE)) {
    const at = m.index ?? 0;
    if (at > last) out.push(text.slice(last, at));
    const key = `${keyBase}-${n++}`;
    const tok = m[0];
    if (m[1]) {
      out.push(<code key={key}>{tok.slice(1, -1)}</code>);
    } else if (m[2]) {
      out.push(<strong key={key}>{renderInline(tok.slice(2, -2), key)}</strong>);
    } else if (m[3]) {
      out.push(<em key={key}>{renderInline(tok.slice(1, -1), key)}</em>);
    } else if (m[4]) {
      const isImage = tok.startsWith("!");
      const label = tok.slice(tok.indexOf("[") + 1, tok.indexOf("]("));
      const url = tok.slice(tok.indexOf("](") + 2, -1);
      if (isImage || !isExternalUrl(url)) {
        out.push(label); // never load remote images or run javascript: links
      } else {
        out.push(
          <ExternalLink key={key} href={url}>
            {renderInline(label, key)}
          </ExternalLink>,
        );
      }
    } else if (m[5]) {
      const url = tok.replace(/[.,;:!?]+$/, "");
      out.push(<ExternalLink key={key} href={url} />);
      if (url.length < tok.length) out.push(tok.slice(url.length));
    }
    last = at + tok.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

type Block =
  | { t: "h"; level: number; text: string }
  | { t: "p"; text: string }
  | { t: "ul" | "ol"; items: string[] }
  | { t: "code"; text: string }
  | { t: "quote"; text: string }
  | { t: "hr" };

export function parseBlocks(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let para: string[] = [];
  const flush = () => {
    if (para.length) blocks.push({ t: "p", text: para.join(" ") });
    para = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (/^\s*```/.test(line)) {
      flush();
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i]!)) body.push(lines[i++]!);
      blocks.push({ t: "code", text: body.join("\n") });
      continue;
    }
    if (!line.trim()) {
      flush();
      continue;
    }
    const h = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line);
    if (h) {
      flush();
      blocks.push({ t: "h", level: h[1]!.length, text: h[2]! });
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flush();
      blocks.push({ t: "hr" });
      continue;
    }
    const li = /^\s*(?:([-*+])|(\d+)[.)])\s+(.*)$/.exec(line);
    if (li) {
      flush();
      const kind = li[1] ? "ul" : "ol";
      const prev = blocks[blocks.length - 1];
      if (prev && prev.t === kind) prev.items.push(li[3]!);
      else blocks.push({ t: kind, items: [li[3]!] });
      continue;
    }
    const q = /^\s*>\s?(.*)$/.exec(line);
    if (q) {
      flush();
      const prev = blocks[blocks.length - 1];
      if (prev && prev.t === "quote") prev.text += ` ${q[1]}`;
      else blocks.push({ t: "quote", text: q[1]! });
      continue;
    }
    // Continuation of the previous list item (indented text).
    const prev = blocks[blocks.length - 1];
    if (!para.length && prev && (prev.t === "ul" || prev.t === "ol") && /^\s{2,}\S/.test(line)) {
      prev.items[prev.items.length - 1] += ` ${line.trim()}`;
      continue;
    }
    para.push(line.trim());
  }
  flush();
  return blocks;
}

export function Markdown({ source, className }: { source: string; className?: string }) {
  const blocks = parseBlocks(source);
  return (
    <div className={className ?? "md"}>
      {blocks.map((b, i) => {
        const key = `b${i}`;
        switch (b.t) {
          case "h": {
            // The modal title is the h1: release headings start at h3.
            const Tag = `h${Math.min(6, Math.max(3, b.level + 1))}` as "h3" | "h4" | "h5" | "h6";
            return <Tag key={key}>{renderInline(b.text, key)}</Tag>;
          }
          case "p":
            return <p key={key}>{renderInline(b.text, key)}</p>;
          case "ul":
          case "ol": {
            const List = b.t;
            return (
              <List key={key}>
                {b.items.map((it, j) => (
                  <li key={j}>{renderInline(it, `${key}-${j}`)}</li>
                ))}
              </List>
            );
          }
          case "code":
            return (
              <pre key={key}>
                <code>{b.text}</code>
              </pre>
            );
          case "quote":
            return <blockquote key={key}>{renderInline(b.text, key)}</blockquote>;
          case "hr":
            return <hr key={key} />;
          default:
            return <Fragment key={key} />;
        }
      })}
    </div>
  );
}
