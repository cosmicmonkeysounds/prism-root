// Author-time AST access for the Properties tray + static story views.
// Backed by the native TypeScript Loom parser (`@loom/core`) — no wasm,
// fully synchronous.
//
// The parser hands us real TypeScript objects: top-level items are
// tagged unions (`{ kind: 'beat', value }`), and the header contract /
// properties are JS `Map`s. The shape-agnostic `field` / `entries`
// accessors below work over both Maps and plain objects so callers stay
// representation-independent.

import { parse } from '@loom/core/parser'
import {
  applyBeatProperty,
  applyInsertBeat,
  applyMoveBeat,
  applyRemoveBeat,
  type LoomFile,
} from '@loom/core/parser'

export type LoomPos = { line: number; column: number; offset: number }
export type LoomSpan = { start: LoomPos; end: LoomPos }
export type LoomFileAst = LoomFile
export type LoomParseResult = { ast: LoomFileAst; diagnostics: unknown[] }
export type ItemKind = 'beat' | 'declaration' | 'let'

// ---------------------------------------------------------------------------
// Shape-agnostic accessors (Map | plain object)
// ---------------------------------------------------------------------------

export function field(obj: unknown, key: string): unknown {
  if (obj instanceof Map) return (obj as Map<string, unknown>).get(key)
  if (obj && typeof obj === 'object') return (obj as Record<string, unknown>)[key]
  return undefined
}

export function entries(obj: unknown): [string, unknown][] {
  if (obj instanceof Map) return [...(obj as Map<string, unknown>).entries()]
  if (obj && typeof obj === 'object') return Object.entries(obj as Record<string, unknown>)
  return []
}

// ---------------------------------------------------------------------------
// AST queries
// ---------------------------------------------------------------------------

export function itemKind(item: unknown): ItemKind | null {
  const k = field(item, 'kind')
  if (k === 'beat') return 'beat'
  if (k === 'declaration') return 'declaration'
  if (k === 'letBinding') return 'let'
  return null
}

export function itemPayload(item: unknown): unknown {
  return field(item, 'value')
}

export function itemSpan(item: unknown): LoomSpan | null {
  return (field(itemPayload(item), 'span') as LoomSpan) ?? null
}

export function itemName(item: unknown): string {
  return String(field(itemPayload(item), 'name') ?? '')
}

/** The item (beat / declaration / let) whose span covers `line0` (0-based). */
export function itemAtLine(ast: LoomFileAst, line0: number): unknown | null {
  for (const it of ast.items) {
    const span = itemSpan(it)
    if (span && line0 >= span.start.line && line0 <= span.end.line) return it
  }
  return null
}

export function findBeat(ast: LoomFileAst, name: string): unknown | null {
  return ast.items.find((it) => itemKind(it) === 'beat' && itemName(it) === name) ?? null
}

export function findDeclaration(ast: LoomFileAst, name: string): unknown | null {
  return ast.items.find((it) => itemKind(it) === 'declaration' && itemName(it) === name) ?? null
}

/** Text of a `PropertyValue` ({ value, span }). */
export function propText(pv: unknown): string {
  const v = field(pv, 'value')
  return v == null ? '' : String(v)
}

export function declKindLabel(kind: unknown): string {
  return typeof kind === 'string' ? kind : 'declaration'
}

/** Count beat body items by their variant tag (dialogue / choice / …). */
export function bodyBreakdown(body: unknown): { tag: string; count: number }[] {
  const arr = Array.isArray(body) ? body : []
  const counts = new Map<string, number>()
  for (const bi of arr) {
    const tag = String(field(bi, 'kind') ?? 'item')
    counts.set(tag, (counts.get(tag) ?? 0) + 1)
  }
  return [...counts.entries()].map(([tag, count]) => ({ tag, count }))
}

export type FileSummaryData = {
  beats: number
  lets: number
  declarations: [string, number][]
}

export function summarize(ast: LoomFileAst): FileSummaryData {
  let beats = 0
  let lets = 0
  const decls = new Map<string, number>()
  for (const it of ast.items) {
    const k = itemKind(it)
    if (k === 'beat') beats++
    else if (k === 'let') lets++
    else if (k === 'declaration') {
      const label = declKindLabel(field(itemPayload(it), 'kind'))
      decls.set(label, (decls.get(label) ?? 0) + 1)
    }
  }
  return { beats, lets, declarations: [...decls.entries()] }
}

// ---------------------------------------------------------------------------
// Parser + structural-edit access — synchronous now (no wasm load).
// The hook shape is retained so callers don't change.
// ---------------------------------------------------------------------------

type ParseFn = (source: string) => LoomParseResult

const PARSE_FN: ParseFn = (source) => {
  const [ast, diagnostics] = parse(source)
  return { ast, diagnostics }
}

/** The Loom `parse` fn. Always available (engine is in-process TS). */
export function useLoomParser(): ParseFn | null {
  return PARSE_FN
}

export type Anchor = 'before' | 'after' | 'start' | 'end'

export type LoomEditApi = {
  /** Set / insert a beat contract property; returns the new source. */
  setBeatProperty: (source: string, beat: string, key: string, value: string) => string
  /** Move a beat relative to `anchorName`; returns the new source. */
  moveBeat: (source: string, beat: string, anchor: Anchor, anchorName: string) => string
  /** Insert a new empty beat at `anchor`; returns the new source. */
  insertBeat: (source: string, name: string, anchor: Anchor, anchorName: string) => string
  /** Delete a beat; returns the new source. */
  removeBeat: (source: string, beat: string) => string
}

const EDIT_API: LoomEditApi = {
  setBeatProperty: (s, b, k, v) => applyBeatProperty(s, b, k, v),
  moveBeat: (s, b, a, n) => applyMoveBeat(s, b, a, n),
  insertBeat: (s, nm, a, n) => applyInsertBeat(s, nm, a, n),
  removeBeat: (s, b) => applyRemoveBeat(s, b),
}

/** The structural-edit API. Always available (engine is in-process TS). */
export function useLoomEdit(): LoomEditApi | null {
  return EDIT_API
}
