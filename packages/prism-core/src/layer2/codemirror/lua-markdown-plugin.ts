/**
 * lua-markdown-plugin — scans markdown text for ```lua fenced blocks,
 * executes each via an injected runner, and replaces the block with a
 * new fenced block containing both the source and its output.
 *
 * Runner is injected so the module stays testable and does not pull in
 * wasmoon at the type level. In the default browser wiring, pass
 * executeLua from `@prism/core/lua`.
 *
 * Output format per block:
 *
 *     ```lua
 *     print("hi")
 *     ```
 *     ```
 *     output: hi
 *     ```
 */

// ── Types ───────────────────────────────────────────────────────────────────

export interface LuaRunnerResult {
  success: boolean;
  value: unknown;
  error?: string | undefined;
  stdout?: string | undefined;
}

export type LuaRunner = (script: string) => Promise<LuaRunnerResult>;

export interface LuaMarkdownBlock {
  /** Character offset of the opening fence. */
  start: number;
  /** Character offset just past the closing fence. */
  end: number;
  /** The raw Lua source code inside the fence. */
  source: string;
}

// ── Block extraction ────────────────────────────────────────────────────────

/**
 * Find every ```lua ... ``` block in the markdown text.
 * Only matches fenced code blocks whose info string is exactly `lua`
 * (case-insensitive), followed by end-of-line.
 */
export function findLuaBlocks(markdown: string): LuaMarkdownBlock[] {
  const blocks: LuaMarkdownBlock[] = [];
  const lines = markdown.split("\n");
  let offset = 0;
  let i = 0;

  while (i < lines.length) {
    const line = lines[i] ?? "";
    const match = /^```lua\s*$/i.exec(line);
    if (!match) {
      offset += line.length + 1; // +1 for newline
      i++;
      continue;
    }

    const blockStart = offset;
    const openFenceLen = line.length + 1;
    offset += openFenceLen;
    i++;

    const sourceLines: string[] = [];
    let closed = false;
    while (i < lines.length) {
      const inner = lines[i] ?? "";
      if (/^```\s*$/.test(inner)) {
        offset += inner.length + 1;
        i++;
        closed = true;
        break;
      }
      sourceLines.push(inner);
      offset += inner.length + 1;
      i++;
    }

    if (closed) {
      blocks.push({
        start: blockStart,
        end: offset,
        source: sourceLines.join("\n"),
      });
    }
  }

  return blocks;
}

// ── Block replacement ───────────────────────────────────────────────────────

/**
 * Format a LuaRunnerResult as a fenced output block.
 */
export function formatBlockResult(
  source: string,
  result: LuaRunnerResult,
): string {
  const parts: string[] = [];
  parts.push("```lua");
  parts.push(source);
  parts.push("```");

  if (result.success) {
    const output: string[] = [];
    if (result.stdout) output.push(result.stdout.trimEnd());
    if (result.value !== undefined && result.value !== null) {
      output.push(`=> ${formatValue(result.value)}`);
    }
    if (output.length > 0) {
      parts.push("```");
      parts.push(output.join("\n"));
      parts.push("```");
    }
  } else {
    parts.push("```");
    parts.push(`error: ${result.error ?? "unknown error"}`);
    parts.push("```");
  }

  return parts.join("\n");
}

function formatValue(value: unknown): string {
  if (value === null) return "nil";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

/**
 * Execute every ```lua block in the markdown via the runner and return
 * the markdown with each block replaced by its source + output.
 */
export async function processLuaBlocks(
  markdown: string,
  runner: LuaRunner,
): Promise<string> {
  const blocks = findLuaBlocks(markdown);
  if (blocks.length === 0) return markdown;

  // Execute all blocks (sequentially — Lua runtimes may share state)
  const replacements: { start: number; end: number; text: string }[] = [];
  for (const block of blocks) {
    const result = await runner(block.source);
    replacements.push({
      start: block.start,
      end: block.end,
      text: formatBlockResult(block.source, result),
    });
  }

  // Splice replacements back into the markdown from end to start
  // so offsets don't shift.
  let out = markdown;
  for (let i = replacements.length - 1; i >= 0; i--) {
    const r = replacements[i];
    if (!r) continue;
    out = out.slice(0, r.start) + r.text + out.slice(r.end);
  }
  return out;
}
