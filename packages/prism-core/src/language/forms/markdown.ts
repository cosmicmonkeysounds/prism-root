export type BlockToken =
  | { kind: "empty" }
  | { kind: "hr" }
  | { kind: "h1"; text: string }
  | { kind: "h2"; text: string }
  | { kind: "h3"; text: string }
  | { kind: "p"; text: string }
  | { kind: "blockquote"; text: string }
  | { kind: "li"; text: string }
  | { kind: "oli"; text: string; n: number }
  | { kind: "task"; text: string; checked: boolean }
  | { kind: "code"; text: string; lang?: string };

export type InlineToken =
  | { kind: "text"; text: string }
  | { kind: "bold"; children: InlineToken[] }
  | { kind: "italic"; children: InlineToken[] }
  | { kind: "code"; text: string }
  | { kind: "link"; text: string; href: string }
  | { kind: "wiki"; id: string; display: string };

const INLINE_RE_SOURCE =
  /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]|\*\*(.+?)\*\*|\*(.+?)\*|`([^`]+)`|\[([^\]]+)\]\(([^)]+)\)/;

export function parseMarkdown(md: string): BlockToken[] {
  const tokens: BlockToken[] = [];
  const lines = md.split("\n");
  let inCode = false;
  let codeLang = "";
  let codeLines: string[] = [];

  for (const raw of lines) {
    if (raw.startsWith("```")) {
      if (inCode) {
        const codeToken: BlockToken = { kind: "code", text: codeLines.join("\n") };
        if (codeLang) codeToken.lang = codeLang;
        tokens.push(codeToken);
        codeLines = [];
        codeLang = "";
        inCode = false;
      } else {
        inCode = true;
        codeLang = raw.slice(3).trim();
      }
      continue;
    }
    if (inCode) {
      codeLines.push(raw);
      continue;
    }

    if (raw.trim() === "") {
      tokens.push({ kind: "empty" });
      continue;
    }

    if (raw === "---" || raw === "***" || raw === "___") {
      tokens.push({ kind: "hr" });
      continue;
    }

    if (raw.startsWith("### ")) {
      tokens.push({ kind: "h3", text: raw.slice(4) });
      continue;
    }
    if (raw.startsWith("## ")) {
      tokens.push({ kind: "h2", text: raw.slice(3) });
      continue;
    }
    if (raw.startsWith("# ")) {
      tokens.push({ kind: "h1", text: raw.slice(2) });
      continue;
    }

    if (raw.startsWith("> ")) {
      tokens.push({ kind: "blockquote", text: raw.slice(2) });
      continue;
    }

    const taskMatch = raw.match(/^[-*] \[([ x])\] (.*)$/);
    if (taskMatch) {
      tokens.push({ kind: "task", text: taskMatch[2] as string, checked: taskMatch[1] === "x" });
      continue;
    }

    if (raw.startsWith("- ") || raw.startsWith("* ")) {
      tokens.push({ kind: "li", text: raw.slice(2) });
      continue;
    }

    const olMatch = raw.match(/^(\d+)\. (.*)$/);
    if (olMatch) {
      tokens.push({ kind: "oli", text: olMatch[2] as string, n: Number(olMatch[1]) });
      continue;
    }

    tokens.push({ kind: "p", text: raw });
  }

  if (inCode && codeLines.length > 0) {
    const codeToken: BlockToken = { kind: "code", text: codeLines.join("\n") };
    if (codeLang) codeToken.lang = codeLang;
    tokens.push(codeToken);
  }

  return tokens;
}

export function parseInline(text: string): InlineToken[] {
  const re = new RegExp(INLINE_RE_SOURCE.source, "g");
  const tokens: InlineToken[] = [];
  let last = 0;
  let m: RegExpExecArray | null;

  while ((m = re.exec(text)) !== null) {
    if (m.index > last) {
      tokens.push({ kind: "text", text: text.slice(last, m.index) });
    }

    if (m[1] !== undefined) {
      const id = m[1].trim();
      const display = m[2]?.trim() ?? id;
      tokens.push({ kind: "wiki", id, display });
    } else if (m[3] !== undefined) {
      tokens.push({ kind: "bold", children: parseInline(m[3]) });
    } else if (m[4] !== undefined) {
      tokens.push({ kind: "italic", children: parseInline(m[4]) });
    } else if (m[5] !== undefined) {
      tokens.push({ kind: "code", text: m[5] });
    } else if (m[6] !== undefined && m[7] !== undefined) {
      tokens.push({ kind: "link", text: m[6], href: m[7] });
    }

    last = m.index + m[0].length;
  }

  if (last < text.length) {
    tokens.push({ kind: "text", text: text.slice(last) });
  }

  return tokens;
}

export function inlineToPlainText(tokens: InlineToken[]): string {
  return tokens
    .map((t) => {
      switch (t.kind) {
        case "text":
          return t.text;
        case "bold":
          return inlineToPlainText(t.children);
        case "italic":
          return inlineToPlainText(t.children);
        case "code":
          return t.text;
        case "link":
          return t.text;
        case "wiki":
          return t.display;
      }
    })
    .join("");
}

export function extractWikiIds(blocks: BlockToken[]): string[] {
  const ids: string[] = [];
  for (const block of blocks) {
    if ("text" in block && block.text) {
      for (const t of parseInline(block.text)) {
        if (t.kind === "wiki") ids.push(t.id);
      }
    }
  }
  return ids;
}
