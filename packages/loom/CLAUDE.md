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

Phase 4 in progress: parser stitches headers / declarations (raw
bodies) / beats / dialogue / choices / diverts / fences / conditional
chains (`<if:>/<else if:>/<else>`) / block-opening directives. Runtime
plays the §16 worked example end-to-end with `Bundle` + `Playhead`,
resolves cross-file diverts, dispatches directive calls
(`sfx`/`cue`/`pause`/`anchor`/`fire`/`set`), evaluates reactive `let`
bindings against a `World` scope, runs `<if:>` arms, expands inline
`{expr}` substitutions inside action / dialogue text, tracks sticky
vs. once-only choice consumption, and answers `played(name)` /
`visits(name)` / `since(name)` ledger queries from expressions.

Still to come: Simulacra (CHARACTER bodies — disposition, knowledge,
goals), Meridian (stats / axes / pools / trees), SCENE / GENERATOR
coroutines, tiered scheduler, live-performance layer, Luau bridge,
and the LSP request loop.
