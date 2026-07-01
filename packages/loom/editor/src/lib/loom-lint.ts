import { linter, type Diagnostic as CmDiagnostic } from '@codemirror/lint'
import type { Extension } from '@codemirror/state'
import { parse } from '@loom/core/parser'

/**
 * CodeMirror linter backed by the native TypeScript Loom parser
 * (`@loom/core`). On every doc change CM debounces and asks for
 * diagnostics; we parse the buffer and map the parser's spans straight
 * to CodeMirror offsets.
 *
 * The parser reports offsets as JS string (UTF-16) offsets — exactly
 * CodeMirror's document indexing — so no byte↔char conversion is needed
 * (the old wasm path returned UTF-8 byte offsets and had to remap).
 */
export function loomLint(): Extension {
  return linter(
    (view) => {
      const text = view.state.doc.toString()
      const [, diagnostics] = parse(text)
      return diagnostics.map((d): CmDiagnostic => {
        const from = d.span.start.offset
        const to = Math.max(from + 1, d.span.end.offset)
        return {
          from,
          to,
          severity: d.severity === 'error' ? 'error' : 'warning',
          source: d.code,
          message: d.message,
        }
      })
    },
    { delay: 200 },
  )
}
