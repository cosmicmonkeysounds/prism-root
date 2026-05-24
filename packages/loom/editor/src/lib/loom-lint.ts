import { linter, type Diagnostic as CmDiagnostic } from '@codemirror/lint'
import type { Extension } from '@codemirror/state'

/**
 * CodeMirror linter backed by the real Rust loom-parser (compiled to
 * wasm via `loom-wasm`). On every doc change CM debounces and asks
 * for diagnostics; we hand the buffer to the wasm `diagnose()` export
 * and map byte spans → CM character offsets.
 *
 * The wasm module loads lazily on first request so non-Loom files
 * don't pay for it.
 */

type WasmDiagnostic = {
  code: string
  severity: 'error' | 'warning'
  message: string
  from: number
  to: number
  line: number
  column: number
}

type LoomWasm = {
  diagnose: (source: string) => WasmDiagnostic[]
}

let wasmPromise: Promise<LoomWasm> | null = null

async function loadWasm(): Promise<LoomWasm> {
  if (!wasmPromise) {
    wasmPromise = (async () => {
      // Vite resolves these at build time; the wasm file is fetched
      // and instantiated by the auto-generated init() the first time.
      const mod = await import('@/loom-wasm/loom_wasm')
      await mod.default()
      return {
        diagnose: (source: string) => mod.diagnose(source) as WasmDiagnostic[],
      }
    })().catch((err) => {
      // Don't permanently cache a failed import — let the user retry
      // by reopening the file.
      wasmPromise = null
      throw err
    })
  }
  return wasmPromise
}

function toCm(d: WasmDiagnostic): CmDiagnostic {
  return {
    from: d.from,
    to: Math.max(d.from + 1, d.to),
    severity: d.severity === 'error' ? 'error' : 'warning',
    source: d.code,
    message: d.message,
  }
}

export function loomLint(): Extension {
  return linter(
    async (view) => {
      let wasm: LoomWasm
      try {
        wasm = await loadWasm()
      } catch {
        return []
      }
      const text = view.state.doc.toString()
      const diagnostics = wasm.diagnose(text)
      // wasm spans are UTF-8 byte offsets; CodeMirror also stores
      // documents as a string indexed by char position. For pure
      // ASCII input these match. For files with multi-byte chars we
      // convert via a TextEncoder slice.
      const encoder = new TextEncoder()
      const hasMultibyte = encoder.encode(text).length !== text.length
      if (!hasMultibyte) {
        return diagnostics.map(toCm)
      }
      const byteToChar = buildByteIndex(text)
      return diagnostics.map((d) => {
        const from = byteToChar[Math.min(d.from, byteToChar.length - 1)] ?? d.from
        const to = byteToChar[Math.min(d.to, byteToChar.length - 1)] ?? d.to
        return toCm({ ...d, from, to })
      })
    },
    { delay: 200 },
  )
}

/** Build a byte-offset → char-offset lookup for the whole document. */
function buildByteIndex(text: string): number[] {
  const map: number[] = []
  let byte = 0
  for (let i = 0; i < text.length; i++) {
    map[byte] = i
    const code = text.charCodeAt(i)
    if (code < 0x80) byte += 1
    else if (code < 0x800) byte += 2
    else if (code >= 0xd800 && code <= 0xdbff) {
      // High surrogate; the matching low surrogate is the next code unit.
      byte += 4
      i++
    } else byte += 3
  }
  map[byte] = text.length
  return map
}
