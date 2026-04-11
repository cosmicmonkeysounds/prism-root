/**
 * High-level Luau AST helpers — the surface that panels, the debugger,
 * and the kernel consume. Thin async wrappers around the full-moon WASM
 * module, normalising the raw wasm-bindgen return values into the shapes
 * the rest of Prism expects.
 *
 * None of these helpers hold state. Initialization is managed by
 * `wasm-loader.ts`; every call cheaply awaits `ensureLuauParserLoaded`.
 */

import type {
  Diagnostic,
  TextRange,
  RootNode,
  SyntaxNode,
} from "@prism/core/syntax";
import { ensureLuauParserLoaded, getLuauParserSync } from "./wasm-loader.js";

// ── Public types ─────────────────────────────────────────────────────────────

/**
 * One parsed `ui.<kind>(...)` call. Mirrors the Rust `UiCall` struct
 * exported from `native/luau-parser/src/lib.rs`. Replaces the ad-hoc
 * `UINode` format the hand-rolled parser in `luau-facet-panel.tsx` used —
 * callers map this into their own renderer props.
 */
export interface LuauUiCall {
  /** The element kind after `ui.`, e.g. "label", "button", "section". */
  kind: string;
  /** Positional and named args in the order they appear in source. */
  args: LuauUiArg[];
  /** 1-based line number where the call begins. */
  line: number;
  /** 0-based column where the call begins. */
  column: number;
  /** Nested `ui.*(...)` calls (section/column/row bodies). */
  children: LuauUiCall[];
}

export interface LuauUiArg {
  /** `undefined` for positional args, string for `{ key = value }` entries. */
  key?: string;
  /** The literal value as it appears in source (strings are unquoted). */
  value: string;
  /** Which branch of the Rust parser produced this arg. */
  valueKind: "string" | "number" | "bool" | "identifier" | "other";
}

/**
 * `{ kind, args, line, column, children }` result from `findUiCalls` with
 * a top-level `error` field for parser failures. Callers render a fallback
 * when `error` is set.
 */
export interface LuauUiParseResult {
  calls: LuauUiCall[];
  error: string | null;
}

// ── Async entry points (panels, debugger) ───────────────────────────────────

/**
 * Parse Luau source into a Unist-compatible `RootNode`. Returns the root
 * node on success; throws with the full-moon error string on failure.
 */
export async function parseLuau(source: string): Promise<RootNode> {
  const mod = await ensureLuauParserLoaded();
  const raw = mod.parse(source);
  return normalizeRoot(raw);
}

/**
 * Extract every top-level `ui.<kind>(...)` call from the source. Never
 * throws — parser errors are surfaced on the `error` field so panels can
 * render a fallback without wrapping in try/catch.
 */
export async function findUiCalls(
  source: string,
): Promise<LuauUiParseResult> {
  if (source.trim().length === 0) {
    return { calls: [], error: null };
  }
  const mod = await ensureLuauParserLoaded();
  try {
    const raw = mod.findUiCalls(source);
    return { calls: normalizeUiCalls(raw), error: null };
  } catch (err) {
    return { calls: [], error: errorMessage(err) };
  }
}

/**
 * Return the 1-based line number where every statement in the source
 * begins. Used by `luau-debugger.ts` to inject `__prism_trace(n)` at
 * statement boundaries — correct on multi-line strings and statements
 * (the regex line scanner it replaced was not).
 */
export async function findStatementLines(source: string): Promise<number[]> {
  if (source.trim().length === 0) return [];
  const mod = await ensureLuauParserLoaded();
  const raw = mod.findStatementLines(source);
  // Return a plain Array so callers can `.includes` / `.map` without
  // worrying about the TypedArray API surface.
  return Array.from(raw);
}

/**
 * Parse-only diagnostics for the source. Returns an empty array on success,
 * a single diagnostic for the first parser error otherwise. Severity is
 * always `"error"` for parser failures.
 */
export async function validateLuau(source: string): Promise<Diagnostic[]> {
  const mod = await ensureLuauParserLoaded();
  const raw = mod.validate(source);
  return normalizeDiagnostics(raw, source);
}

// ── Sync accessors (post-init LanguageDefinition / SyntaxProvider) ──────────

/** Sync variant of `parseLuau`. Requires `ensureLuauParserLoaded()` first. */
export function parseLuauSync(source: string): RootNode {
  const mod = getLuauParserSync();
  const raw = mod.parse(source);
  return normalizeRoot(raw);
}

/** Sync variant of `findUiCalls`. Requires `ensureLuauParserLoaded()` first. */
export function findUiCallsSync(source: string): LuauUiParseResult {
  if (source.trim().length === 0) return { calls: [], error: null };
  const mod = getLuauParserSync();
  try {
    const raw = mod.findUiCalls(source);
    return { calls: normalizeUiCalls(raw), error: null };
  } catch (err) {
    return { calls: [], error: errorMessage(err) };
  }
}

/** Sync variant of `findStatementLines`. */
export function findStatementLinesSync(source: string): number[] {
  if (source.trim().length === 0) return [];
  const mod = getLuauParserSync();
  return Array.from(mod.findStatementLines(source));
}

/** Sync variant of `validateLuau`. */
export function validateLuauSync(source: string): Diagnostic[] {
  const mod = getLuauParserSync();
  const raw = mod.validate(source);
  return normalizeDiagnostics(raw, source);
}

// ── Normalizers ──────────────────────────────────────────────────────────────

function normalizeRoot(raw: unknown): RootNode {
  if (!raw || typeof raw !== "object") {
    return { type: "root", children: [] };
  }
  const obj = raw as Record<string, unknown>;
  const children = Array.isArray(obj["children"])
    ? (obj["children"] as unknown[]).map(normalizeSyntaxNode)
    : [];
  const root: RootNode = { type: "root", children };
  const position = normalizePosition(obj["position"]);
  if (position) root.position = position;
  return root;
}

function normalizeSyntaxNode(raw: unknown): SyntaxNode {
  if (!raw || typeof raw !== "object") {
    return { type: "unknown" };
  }
  const obj = raw as Record<string, unknown>;
  const node: SyntaxNode = {
    type: typeof obj["type"] === "string" ? (obj["type"] as string) : "unknown",
  };
  const position = normalizePosition(obj["position"]);
  if (position) node.position = position;
  if (typeof obj["value"] === "string") node.value = obj["value"] as string;
  if (Array.isArray(obj["children"])) {
    node.children = (obj["children"] as unknown[]).map(normalizeSyntaxNode);
  }
  return node;
}

function normalizePosition(raw: unknown): SyntaxNode["position"] {
  if (!raw || typeof raw !== "object") return undefined;
  const obj = raw as Record<string, unknown>;
  const start = raw2Pos(obj["start"]);
  const end = raw2Pos(obj["end"]);
  if (!start || !end) return undefined;
  return { start, end };
}

function raw2Pos(raw: unknown): { offset: number; line: number; column: number } | null {
  if (!raw || typeof raw !== "object") return null;
  const obj = raw as Record<string, unknown>;
  const offset = typeof obj["offset"] === "number" ? (obj["offset"] as number) : 0;
  const line = typeof obj["line"] === "number" ? (obj["line"] as number) : 1;
  const column =
    typeof obj["column"] === "number" ? (obj["column"] as number) : 0;
  return { offset, line, column };
}

function normalizeUiCalls(raw: unknown): LuauUiCall[] {
  if (!Array.isArray(raw)) return [];
  const out: LuauUiCall[] = [];
  for (const entry of raw) {
    const call = normalizeUiCall(entry);
    if (call) out.push(call);
  }
  return out;
}

function normalizeUiCall(raw: unknown): LuauUiCall | null {
  if (!raw || typeof raw !== "object") return null;
  const obj = raw as Record<string, unknown>;
  const kind = typeof obj["kind"] === "string" ? (obj["kind"] as string) : "";
  if (!kind) return null;
  const args = Array.isArray(obj["args"])
    ? (obj["args"] as unknown[])
        .map(normalizeUiArg)
        .filter((a): a is LuauUiArg => a !== null)
    : [];
  const line = typeof obj["line"] === "number" ? (obj["line"] as number) : 0;
  const column =
    typeof obj["column"] === "number" ? (obj["column"] as number) : 0;
  const children = Array.isArray(obj["children"])
    ? normalizeUiCalls(obj["children"])
    : [];
  return { kind, args, line, column, children };
}

function normalizeUiArg(raw: unknown): LuauUiArg | null {
  if (!raw || typeof raw !== "object") return null;
  const obj = raw as Record<string, unknown>;
  const value = typeof obj["value"] === "string" ? (obj["value"] as string) : "";
  const valueKindRaw = obj["valueKind"];
  const valueKind: LuauUiArg["valueKind"] =
    valueKindRaw === "string" ||
    valueKindRaw === "number" ||
    valueKindRaw === "bool" ||
    valueKindRaw === "identifier" ||
    valueKindRaw === "other"
      ? valueKindRaw
      : "other";
  const arg: LuauUiArg = { value, valueKind };
  if (typeof obj["key"] === "string") arg.key = obj["key"] as string;
  return arg;
}

function normalizeDiagnostics(raw: unknown, source: string): Diagnostic[] {
  if (!Array.isArray(raw)) return [];
  const out: Diagnostic[] = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== "object") continue;
    const obj = entry as Record<string, unknown>;
    const message =
      typeof obj["message"] === "string"
        ? (obj["message"] as string)
        : "Parse error";
    const line = typeof obj["line"] === "number" ? (obj["line"] as number) : 1;
    const column =
      typeof obj["column"] === "number" ? (obj["column"] as number) : 0;
    out.push({
      message,
      severity: "error",
      range: lineColToRange(source, line, column),
    });
  }
  return out;
}

function lineColToRange(
  source: string,
  line: number,
  column: number,
): TextRange {
  // Convert 1-based line + 0-based column to byte offset in source.
  let offset = 0;
  let currentLine = 1;
  for (let i = 0; i < source.length && currentLine < line; i++) {
    if (source[i] === "\n") currentLine++;
    offset = i + 1;
  }
  const start = offset + column;
  return { start, end: start + 1 };
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return String(err);
}
