export type WikiToken =
  | { kind: "text"; text: string }
  | { kind: "link"; id: string; display: string; raw: string };

const WIKI_LINK_RE = /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g;

export function parseWikiLinks(text: string): WikiToken[] {
  const tokens: WikiToken[] = [];
  let last = 0;

  for (const match of text.matchAll(WIKI_LINK_RE)) {
    const start = match.index;
    if (start > last) {
      tokens.push({ kind: "text", text: text.slice(last, start) });
    }
    const id = match[1]!.trim();
    const display = match[2]?.trim() ?? id;
    tokens.push({ kind: "link", id, display, raw: match[0] });
    last = start + match[0].length;
  }

  if (last < text.length) {
    tokens.push({ kind: "text", text: text.slice(last) });
  }

  return tokens;
}

export function extractLinkedIds(text: string): string[] {
  const ids: string[] = [];
  for (const match of text.matchAll(WIKI_LINK_RE)) {
    ids.push(match[1]!.trim());
  }
  return ids;
}

export function renderWikiLinks(text: string, resolver: (id: string) => string): string {
  return text.replace(WIKI_LINK_RE, (_match, id: string, display: string | undefined) => {
    return display?.trim() ?? resolver(id.trim());
  });
}

export function buildWikiLink(id: string, display?: string): string {
  return display && display !== id ? `[[${id}|${display}]]` : `[[${id}]]`;
}

export function detectInlineLink(text: string, cursorPos: number): string | null {
  const before = text.slice(0, cursorPos);
  const openIdx = before.lastIndexOf("[[");
  if (openIdx < 0) return null;
  const between = before.slice(openIdx + 2);
  if (between.includes("]]")) return null;
  return between;
}
