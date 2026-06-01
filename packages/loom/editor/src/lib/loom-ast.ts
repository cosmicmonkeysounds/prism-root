// Author-time AST access for the Properties tray (IDE redesign v2,
// Phase 2). The runtime `DetailFor` registry reads a live play head;
// author modes (Writing / Editing) have no play session, so they read
// the parsed AST of the active file instead — via the wasm `parse`
// export.
//
// `serde_wasm_bindgen` serialises Rust structs as JS objects but maps
// (the `IndexMap` contract / header properties, and externally-tagged
// enum wrappers) as JS `Map`s. The `field` / `entries` accessors below
// are deliberately robust to both shapes so we don't depend on that
// representation detail.

import { useEffect, useState } from 'react'

export type LoomPos = { line: number; column: number; byte: number }
export type LoomSpan = { start: LoomPos; end: LoomPos }
export type LoomFileAst = { header: unknown; items: unknown[] }
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

function firstKey(obj: unknown): string | null {
  if (typeof obj === 'string') return obj
  const e = entries(obj)
  return e.length ? e[0][0] : null
}

function has(obj: unknown, key: string): boolean {
  if (obj instanceof Map) return (obj as Map<string, unknown>).has(key)
  return !!obj && typeof obj === 'object' && key in (obj as object)
}

// ---------------------------------------------------------------------------
// AST queries
// ---------------------------------------------------------------------------

export function itemKind(item: unknown): ItemKind | null {
  if (has(item, 'Beat')) return 'beat'
  if (has(item, 'Declaration')) return 'declaration'
  if (has(item, 'LetBinding')) return 'let'
  return null
}

export function itemPayload(item: unknown): unknown {
  return field(item, 'Beat') ?? field(item, 'Declaration') ?? field(item, 'LetBinding')
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
  if (typeof kind === 'string') return kind
  return firstKey(kind) ?? 'Declaration'
}

/** Count beat body items by their variant tag (Dialogue / Choice / …). */
export function bodyBreakdown(body: unknown): { tag: string; count: number }[] {
  const arr = Array.isArray(body) ? body : []
  const counts = new Map<string, number>()
  for (const bi of arr) {
    const tag = firstKey(bi) ?? 'item'
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
// wasm parser loader + hook
// ---------------------------------------------------------------------------

type ParseFn = (source: string) => LoomParseResult

let parserPromise: Promise<ParseFn> | null = null

async function loadParser(): Promise<ParseFn> {
  if (!parserPromise) {
    parserPromise = (async () => {
      const mod = await import('@/loom-wasm/loom_wasm')
      await mod.default()
      return (source: string) => mod.parse(source) as LoomParseResult
    })().catch((err) => {
      parserPromise = null
      throw err
    })
  }
  return parserPromise
}

/** Returns the wasm `parse` fn once loaded, or `null` while loading. */
export function useLoomParser(): ParseFn | null {
  const [fn, setFn] = useState<ParseFn | null>(null)
  useEffect(() => {
    let alive = true
    loadParser()
      .then((f) => {
        if (alive) setFn(() => f)
      })
      .catch(() => {
        /* parser unavailable — tray falls back to a loading state */
      })
    return () => {
      alive = false
    }
  }, [])
  return fn
}
