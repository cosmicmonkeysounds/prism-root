import { linter, type Diagnostic as CmDiagnostic } from '@codemirror/lint'
import type { Extension } from '@codemirror/state'
import { parse } from '@loom/core/parser'
import { lspWorkspaceSync, uriFor } from '@/lib/lsp-client'
import { syncBuffer } from '@/lib/lsp-index'
import { lspToOffset } from '@/lib/lsp-nav'

/**
 * CodeMirror linter backed by the native TypeScript Loom parser
 * (`@loom/core`). On every doc change CM debounces and asks for
 * diagnostics; we parse the buffer and map the parser's spans straight
 * to CodeMirror offsets.
 *
 * The parser reports offsets as JS string (UTF-16) offsets — exactly
 * CodeMirror's document indexing — so no byte↔char conversion is needed
 * (the old wasm path returned UTF-8 byte offsets and had to remap).
 *
 * This parser-only variant is the fallback; `loomLintProject` (below) is
 * the richer, cross-file-aware default.
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

const SEVERITY: Record<number, CmDiagnostic['severity']> = {
  1: 'error',
  2: 'warning',
  3: 'info',
  4: 'hint',
}

/**
 * Project-aware linter: sources diagnostics from the LSP `Workspace`, which
 * merges parser diagnostics with the cross-file project diagnostics the
 * compile pass computes (`requiredSlotUnfilled`, `unresolvedTraitArg`,
 * `derivedBeatConflict`, `ambiguousSlot`, `requiredParamUnfilled`,
 * `unfilledDerivedSlot`, entry/main errors). These are invisible to the
 * parser-only linter above.
 *
 * The Workspace returns LSP `Range`s (line + UTF-16 character), so map them
 * back to CM offsets. Cross-file diagnostics are only complete once the whole
 * project is indexed (see `use-lsp-index`), but this degrades gracefully to
 * the current file's parser + own-file project diagnostics before that.
 */
export function loomLintProject(path: string): Extension {
  return linter(
    (view) => {
      syncBuffer(path, view.state.doc.toString())
      const params = lspWorkspaceSync().diagnosticsFor(uriFor(path))
      if (!params) return []
      return params.diagnostics.map((d): CmDiagnostic => {
        const from = lspToOffset(view.state, d.range.start)
        const to = Math.max(from + 1, lspToOffset(view.state, d.range.end))
        return {
          from,
          to,
          severity: (d.severity != null && SEVERITY[d.severity]) || 'error',
          source: d.code ?? d.source,
          message: d.message,
        }
      })
    },
    { delay: 200 },
  )
}
