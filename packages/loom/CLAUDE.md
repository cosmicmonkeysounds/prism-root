# packages/loom

All Loom v3 code lives here, split into sibling crates so the
parser stays independently usable (LSP, codegen, future external
tools) without dragging the runtime + scheduler + Luau bridge along.

| Crate                  | Role                                                            |
|------------------------|-----------------------------------------------------------------|
| [`parser`](./parser)   | Lexer, AST, diagnostics, keyword table for the `.loom` surface  |
| [`runtime`](./runtime) | Bundle, resolver, playhead, ledger, reactive graph, scheduler, directive registry, Luau bridge |
| [`lsp`](./lsp)         | Stdio JSON-RPC server backed by `loom-parser` + a workspace-wide name index |
| [`syntax`](./syntax)   | TextMate grammar generator (driven by `loom-parser::keywords`) + Zed / VSCode extension shells |
| [`wasm`](./wasm)       | `wasm-bindgen` surface for the parser — `parse` / `diagnose` / `emit_tmgrammar` consumed by the React editor |
| [`server`](./server)   | Multi-user backbone — `loom-relayd` axum server hosting per-workspace Loro CRDTs over `prism-core::network::relay`. See [`docs/dev/loom-multiuser.md`](../../docs/dev/loom-multiuser.md). |
| [`editor`](./editor)   | React/Vite/CodeMirror web IDE — the user-facing front end |
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

## Multi-user (Phase 1 landed)

`loom-server` is the new sibling crate that backs collaborative
authoring of `.loom` projects. It depends only on `prism-core` (not
`prism-relay`) so the binary stays small. Phase 1 — module wiring +
`/api/health` — is in. Subsequent phases add auth, multi-workspace
REST, WebSocket CRDT sync, and a `LoomDoc` client wasm surface. Full
roadmap: [`docs/dev/loom-multiuser.md`](../../docs/dev/loom-multiuser.md).

Run locally: `cargo run -p loom-server --bin loom-relayd` (defaults to
`127.0.0.1:7878`).
