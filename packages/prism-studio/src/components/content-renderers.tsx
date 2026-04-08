/**
 * Content renderers — markdown and iframe widgets.
 *
 * MarkdownWidget provides a simple markdown preview block that users can
 * drop onto pages for rich content without a text-block wrapper. It uses
 * a minimal dependency-free markdown → HTML conversion covering the most
 * common inline elements (bold/italic/code/links) and block elements
 * (headings, paragraphs, lists, code blocks, horizontal rules, quotes).
 *
 * IframeWidget embeds an external URL with sandbox attributes.
 */

// ── Markdown helpers ───────────────────────────────────────────────────────

const escapeHtml = (s: string): string =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Convert inline markdown syntax to HTML. Escapes unsafe characters first. */
export function renderInlineMarkdown(src: string): string {
  let out = escapeHtml(src);
  out = out.replace(/`([^`]+)`/g, (_m, code: string) => `<code>${code}</code>`);
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  out = out.replace(/__([^_]+)__/g, "<strong>$1</strong>");
  out = out.replace(/\*([^*]+)\*/g, "<em>$1</em>");
  out = out.replace(/_([^_]+)_/g, "<em>$1</em>");
  out = out.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_m, label: string, href: string) => {
    const safe = href.replace(/"/g, "&quot;");
    return `<a href="${safe}" target="_blank" rel="noopener noreferrer">${label}</a>`;
  });
  return out;
}

/** Convert a markdown document into HTML using a small block grammar. */
export function renderMarkdown(src: string): string {
  if (!src) return "";
  const lines = src.split(/\r?\n/);
  const out: string[] = [];
  let i = 0;
  let inList: "ul" | "ol" | null = null;
  let inCode = false;
  let codeLines: string[] = [];

  const closeList = (): void => {
    if (inList) {
      out.push(`</${inList}>`);
      inList = null;
    }
  };

  while (i < lines.length) {
    const line = lines[i] ?? "";
    if (inCode) {
      if (/^```/.test(line)) {
        out.push(`<pre><code>${escapeHtml(codeLines.join("\n"))}</code></pre>`);
        codeLines = [];
        inCode = false;
      } else {
        codeLines.push(line);
      }
      i += 1;
      continue;
    }
    if (/^```/.test(line)) {
      closeList();
      inCode = true;
      i += 1;
      continue;
    }
    if (/^\s*$/.test(line)) {
      closeList();
      i += 1;
      continue;
    }
    if (/^---+$/.test(line.trim())) {
      closeList();
      out.push("<hr/>");
      i += 1;
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      closeList();
      const level = heading[1]?.length ?? 1;
      out.push(`<h${level}>${renderInlineMarkdown(heading[2] ?? "")}</h${level}>`);
      i += 1;
      continue;
    }
    const quote = /^>\s?(.*)$/.exec(line);
    if (quote) {
      closeList();
      out.push(`<blockquote>${renderInlineMarkdown(quote[1] ?? "")}</blockquote>`);
      i += 1;
      continue;
    }
    const ul = /^[-*+]\s+(.*)$/.exec(line);
    if (ul) {
      if (inList !== "ul") {
        closeList();
        out.push("<ul>");
        inList = "ul";
      }
      out.push(`<li>${renderInlineMarkdown(ul[1] ?? "")}</li>`);
      i += 1;
      continue;
    }
    const ol = /^\d+\.\s+(.*)$/.exec(line);
    if (ol) {
      if (inList !== "ol") {
        closeList();
        out.push("<ol>");
        inList = "ol";
      }
      out.push(`<li>${renderInlineMarkdown(ol[1] ?? "")}</li>`);
      i += 1;
      continue;
    }
    closeList();
    out.push(`<p>${renderInlineMarkdown(line)}</p>`);
    i += 1;
  }
  if (inCode && codeLines.length > 0) {
    out.push(`<pre><code>${escapeHtml(codeLines.join("\n"))}</code></pre>`);
  }
  closeList();
  return out.join("\n");
}

// ── Markdown Widget ────────────────────────────────────────────────────────

export interface MarkdownWidgetProps {
  source?: string;
}

export function MarkdownWidgetRenderer(props: MarkdownWidgetProps) {
  const { source = "" } = props;
  const html = renderMarkdown(source);
  return (
    <div
      data-testid="markdown-widget"
      className="prism-markdown"
      style={{
        padding: 12,
        borderRadius: 6,
        border: "1px solid #e2e8f0",
        background: "#ffffff",
        color: "#0f172a",
        fontSize: 14,
        lineHeight: 1.55,
        margin: "0 0 8px 0",
      }}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

// ── Iframe Widget ──────────────────────────────────────────────────────────

export interface IframeWidgetProps {
  src: string;
  title?: string;
  height?: number;
  allowFullscreen?: boolean;
}

/** Validate a URL is safe for iframe embedding (http/https only). */
export function isSafeIframeUrl(src: string): boolean {
  const trimmed = src.trim();
  if (!trimmed) return false;
  try {
    const url = new URL(trimmed);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export function IframeWidgetRenderer(props: IframeWidgetProps) {
  const { src, title = "Embedded content", height = 360, allowFullscreen = true } = props;
  if (!isSafeIframeUrl(src)) {
    return (
      <div
        data-testid="iframe-widget-empty"
        style={{
          padding: 16,
          border: "1px dashed #cbd5e1",
          borderRadius: 6,
          background: "#f8fafc",
          color: "#64748b",
          fontSize: 12,
          textAlign: "center",
          margin: "0 0 8px 0",
        }}
      >
        Set an http(s) URL to embed content.
      </div>
    );
  }
  return (
    <div data-testid="iframe-widget" style={{ margin: "0 0 8px 0" }}>
      <iframe
        src={src}
        title={title}
        height={height}
        width="100%"
        loading="lazy"
        referrerPolicy="no-referrer"
        sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
        allowFullScreen={allowFullscreen}
        style={{ border: "1px solid #cbd5e1", borderRadius: 6, background: "#ffffff" }}
      />
    </div>
  );
}
