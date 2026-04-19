# Prism

**Distributed Visual Operating System** built on a **Federated Object-Graph** using CRDTs.

Local-first, file-over-app. Applications are **Lenses** — disposable views over sovereign data. If the entire Prism ecosystem disappeared tomorrow, users retain 100% functional local files.

Every Prism app is an IDE. There is no wall between "using" and "building." The difference between a consumer app and a developer tool is a single toggle — the **"Glass Flip."**

> Active migration plan: [`docs/dev/slint-migration-plan.md`](docs/dev/slint-migration-plan.md)
> Decisions: [`docs/adr/`](docs/adr/)
> Per-package context: each package's `CLAUDE.md`

## Status

All-Rust Cargo monorepo on [Slint](https://github.com/slint-ui/slint). The Slint migration (from React/TypeScript) is tracked in [`docs/dev/slint-migration-plan.md`](docs/dev/slint-migration-plan.md). Phases 0–3 are closed; Phase 4+ is pending.

- **Shell** — Rust. `prism-shell` crate. Slint UI compiled at build time (`ui/app.slint`). Native binary + WASM (`wasm32-unknown-unknown` + `wasm-bindgen`) for web.
- **Builder** — Rust. `prism-builder` crate. Component registry + `BuilderDocument` tree with two render targets: Slint DSL (via `slint-interpreter`) and HTML SSR.
- **Daemon** — Rust. `prism-daemon` crate. Transport-agnostic kernel. Runs as a sidecar on desktop (IPC via `interprocess` + `postcard`), in-process on mobile, remote over WebSocket on web.
- **Relay** — Rust. `prism-relay` crate. Axum-based Sovereign Portal SSR server rendering `BuilderDocument` trees to semantic HTML.
- **Studio** — Rust. `prism-studio/src-tauri`. Desktop shell: spawns daemon sidecar, hands control to `prism_shell::Shell`. Slint owns windowing end-to-end.

## Repo layout

```
prism/
├── Cargo.toml              # Workspace root
├── packages/
│   ├── prism-core/         # (Rust) shared domain logic
│   ├── prism-builder/      # (Rust) component registry + Slint/HTML render
│   ├── prism-shell/        # (Rust → WASM / native) the Slint-based client
│   ├── prism-studio/       # Desktop shell (Rust, src-tauri/)
│   ├── prism-daemon/       # (Rust) local physics engine — Loro, Luau, VFS
│   ├── prism-relay/        # (Rust) Axum SSR — federated relay
│   └── prism-cli/          # (Rust) unified `prism` CLI
├── docs/
│   ├── adr/                # Architecture decision records
│   └── dev/                # Active development plans
├── scripts/hooks/          # Claude Code dev hooks
└── $legacy-inspiration-only/  # Reference material from the pre-Slint era
```

## Quick start

### Prerequisites

- **Rust** ≥ 1.75 (`rustup`)
- **wasm-bindgen-cli** — `cargo install wasm-bindgen-cli` (version must match the `wasm-bindgen` crate in the workspace manifest)
- **Node.js** ≥ 20 + **pnpm** ≥ 9 (optional — `pnpm` scripts are thin aliases over the `prism` CLI)

### Dev

```bash
# Build the prism CLI first
cargo build -p prism-cli

# Shell (native, fastest iteration)
prism dev shell

# Shell (web, WASM via wasm-bindgen)
prism dev web

# Desktop shell (Studio)
prism dev studio

# Everything (process supervisor with colored logs)
prism dev all
```

### Build

```bash
prism build                   # all targets (release)
prism build --target web      # WASM build only
prism build --target desktop  # native binary only
prism build --debug           # debug profile
```

### Test / lint / format

```bash
prism test                    # cargo test --workspace
prism lint                    # cargo clippy --workspace --all-targets -- -D warnings
prism fmt                     # cargo fmt --all
prism fmt --check             # cargo fmt --all -- --check
prism --dry-run <...>         # print the expanded argv instead of executing
```

## Philosophy

- **Local-First** — the user's disk is the source of truth.
- **Every App Is an IDE** — consumer and developer share the same app (Glass Flip).
- **Ownership Over Convenience** — no vendor lock-in.
- **Blockchain-Free** — W3C DIDs + E2EE CRDTs, not ledgers or tokens.
- **Loro Is the Hidden Constant** — all state lives in Loro; editors are projections.
- **One Toolchain** — everything is a single `cargo build`.

## License

GPL-3.0-or-later
