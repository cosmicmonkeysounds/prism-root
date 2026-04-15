# prism-core

Shared Rust foundations for the Slint-era Prism stack. Phase-2 target of
the Slint migration: the old `@prism/core` TypeScript package has been
deleted, and the modules that still matter land here leaf-first.

## Build & Test
- `cargo build -p prism-core` — default features (no `loro`).
- `cargo build -p prism-core --features crdt` — pulls in `loro` for hosts
  that actually run the CRDT layer.
- `cargo test -p prism-core` — unit + insta snapshot tests.

## Features
- `crdt` — adds the `loro` dependency. Off by default so pure-logic
  consumers (design tokens, shell mode, boot config, puck-JSON parsing)
  don't drag the CRDT engine into their dependency tree.

## Public surface
From `src/lib.rs`:
- `BootConfig`, `DEFAULT_BOOT_CONFIG` — the four-source boot-time
  resolver ported from `src/boot/load-boot-config.ts`.
- `DesignTokens` — color / spacing / typography constants. Leaf module.
- `Permission`, `ShellMode`, `ShellModeContext` — the runtime context
  Studio uses to decide which lenses and panels are reachable.
- `Store<S>`, `Action<S>`, `Subscription` — the zustand replacement
  from `kernel::store`. A single owning container for `S` with
  reducer-style dispatch, a synchronous subscription bus, and
  `snapshot` / `restore` via serde for §7 hot-reload.

## Module status
Tracked in the per-module `//!` docstrings; canonical list:

| Module | Status | Notes |
|---|---|---|
| `design_tokens` | ✅ ported | Leaf, no deps. |
| `shell_mode` | ✅ ported | Pure data + pure functions. |
| `boot_config` | ✅ ported | Uses `shell_mode`. |
| `foundation::batch` | ✅ ported | |
| `foundation::date` | ✅ ported | |
| `foundation::object_model` | ✅ ported | `case_str`, `context_engine`, `edge_model`, `nsid`, `query`, `registry`, `tree_model`, `types`, `weak_ref`. |
| `foundation::undo` | ✅ ported | Borrows manager via `SharedUndoManager` in bridge. |
| `foundation::vfs` | ✅ ported | |
| `foundation::clipboard` | ✅ ported | `TreeClipboard` borrows tree/edges/undo per-call. |
| `foundation::template` | ✅ ported | `TemplateRegistry` with `{{var}}` interpolation. |
| `foundation::persistence` | ✅ ported | Gated behind the `crdt` feature. `CollectionStore` wraps a `LoroDoc` with `objects` + `edges` maps (records stored as JSON strings so snapshots round-trip with the legacy TS runtime), exposes CRUD + `ObjectFilter` / `EdgeFilter` + synchronous `on_change` listeners + `export_snapshot` / `import`. `VaultManager<A>` orchestrates a `PrismManifest`'s collections against a `PersistenceAdapter` trait; `MemoryAdapter` ships for tests. Lazy-loads stores on first `open_collection`, tracks dirty state via per-store change listeners, saves snapshots to `data/collections/{id}.loro`. 56 unit tests. |
| `identity::did` | ✅ ported | Ed25519 identity, sign/verify, multisig, import/export. |
| `identity::encryption` | ✅ ported | AES-GCM-256 vault key manager, HKDF-derived keys. |
| `identity::manifest` | ✅ ported | Privilege sets + enforcer + `.prism.json` parse/serialise/validate. |
| `identity::trust` | ✅ ported | Sovereign immune system — Luau sandbox, schema poison-pill validator, hashcash proof-of-work, peer trust graph, Shamir secret sharing, encrypted escrow, PBKDF2 password auth. |
| `language::syntax` | 🚧 in progress | AST types (+ `SyntaxNode`/`RootNode`), scanner, token stream, case utils. |
| `language::expression` | 🚧 in progress | Tokens, parser, evaluator, field resolver. |
| `language::registry` | ✅ ported | Unified `LanguageContribution` registry (ADR-002 §A2) — `SurfaceMode`, `InlineTokenDef` + builder, wikilink token, `LanguageSurface`, compound-extension resolver. Generic over renderer (`R`) and editor-extension (`E`) types; `()` defaults keep it framework-free. Exposes `LanguageRegistry::resolve_file(&PrismFile)` for direct document→contribution lookup. |
| `language::document` | ✅ ported | ADR-002 §A1 `PrismFile` — unified file record with `FileBody::{Text, Graph, Binary}`, narrowing helpers, and keyword-struct builders (`TextFileParams` / `GraphFileParams` / `BinaryFileParams`). `schema` is now a typed `Option<DocumentSchema>` — the `serde_json::Value` placeholder came out when `language::forms` landed. |
| `language::forms` | ✅ ported | `field_schema`, `document_schema`, `form_schema`, `form_state`, `wiki_link`, `markdown` (Prism's narrow in-house dialect — not CommonMark). Pure data + pure functions; 48 unit tests. Used by `language::document::PrismFile::schema` and the `language::markdown` contribution below. |
| `language::luau` | 🚧 in progress | ADR-002 §A4 / Phase 4. `create_luau_contribution()` returns a `LanguageContribution<R,E>` with `parse` (stub — returns empty `RootNode` until full-moon port), `syntax_provider` (`LuauSyntaxProvider` stub), and a `code` + `preview` `LanguageSurface`. `mlua`-backed execution lives in `prism-daemon::modules::luau_module` rather than here. |
| `language::markdown` | ✅ ported | `create_markdown_contribution()` returns a `LanguageContribution<R,E>` whose `parse` runs `language::forms::markdown::parse_markdown` and projects each block into a child `SyntaxNode` (`hr` / `h1` / `p` / `oli` / `task` / `code` / …). Surface defaults to `preview`, exposes `code` + `preview`, ships the registry wikilink inline token. |
| `language::codegen` | ✅ ported | ADR-002 §A3 unified pipeline. `CodegenPipeline` dispatches heterogeneous emitters by an open `input_kind` string; `CodegenInputs` is a typed slot bundle backed by `Box<dyn Any>`. Symbol DSL: `SymbolDef` / `SymbolKind` / `SymbolParam` / `EnumValue` + `constant_namespace` / `fn_symbol` builders. Four concrete emitters — `SymbolTypeScriptEmitter`, `SymbolCSharpEmitter`, `SymbolEmmyDocEmitter`, `SymbolGDScriptEmitter` — plus `ts_name_transform` / `cs_name_transform` / `default_gdscript_name_transform`. `SourceBuilder` (line/indent/block/const_block) is the shared text buffer. `TextEmitter` trait rounds `RootNode` back to source text. 30 unit tests. |
| `kernel::store` | ✅ ported | `Store<S>` + `Action<S>` trait + `Subscription` handle. Replaces `zustand` per §6.1 of the migration plan and satisfies §7's hot-reload constraints (one root struct, no global mut, serde-backed `snapshot` / `restore`). Backs `prism_shell::Shell`. 16 unit tests. |
| `kernel::state_machine::machine` | ✅ ported | Flat, context-free finite state machine from `kernel/state-machine/machine.ts`. Generic over `State + Event` (`Eq + Hash + Clone`), `TransitionFrom::{One, Many, Any}` source matching, guards + actions, enter/exit hooks, terminal states, wildcard transitions, opaque `Subscription` handles. The xstate-backed `tool.machine.ts` is deferred to a `statig` rewrite and is not exported. 23 unit tests. |

When porting a new module, match the leaf-first order: port the
dependency-free pieces first, snapshot-test with `insta`, then layer
the higher-level modules on top. Keep per-module `//!` docs honest
about status — the table above is generated by hand and drifts.

## Style
- No multi-paragraph docstrings on items; `//!` module headers are
  allowed because they carry status and dependency notes.
- Types that need to survive hot-reload must live inside the module
  they're defined in — no global `lazy_static` / `OnceCell`.

## Downstream
`prism-builder`, `prism-shell`, and `prism-studio/src-tauri` all depend
on `prism-core`. Touch its public surface carefully and re-run
`cargo check --workspace` as the safety net.
