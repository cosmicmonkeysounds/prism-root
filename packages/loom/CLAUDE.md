# packages/loom

All Loom v3 code lives here, split into four sibling crates so the
parser stays independently usable (LSP, codegen, future external
tools) without dragging the runtime + scheduler + Luau bridge along.

| Crate                  | Role                                                            |
|------------------------|-----------------------------------------------------------------|
| [`parser`](./parser)   | Lexer, AST, diagnostics, keyword table for the `.loom` surface  |
| [`runtime`](./runtime) | Bundle, resolver, playhead, ledger, reactive graph, scheduler, directive registry, Luau bridge |
| [`lsp`](./lsp)         | Stdio JSON-RPC server backed by `loom-parser` + a workspace-wide name index |
| [`syntax`](./syntax)   | TextMate grammar generator (driven by `loom-parser::keywords`) + Zed / VSCode extension shells |
| [`examples`](./examples) | Reference `.loom` projects used by `loom-runtime` integration tests and as authoring tutorials |

The canonical design lives in [`docs/dev/loom-v3.html`](../../docs/dev/loom-v3.html).
Per-crate `lib.rs` docstrings carry the module roadmap and the spec
section each module implements.

## Status

Phase 1 (this commit): V2 deleted; the four sibling crates are
scaffolded with module skeletons + spec cross-references. Nothing
parses or runs yet. Subsequent phases land the lexer → AST →
declarations → reactive graph → playhead → simulacra → meridian →
live layer → LSP request loop, in roughly that order.
