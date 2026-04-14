# Prism

**Distributed Visual Operating System** built on a **Federated Object-Graph** using CRDTs.

Local-first, file-over-app. Applications are **Lenses** — disposable views over sovereign data. If the entire Prism ecosystem disappeared tomorrow, users retain 100% functional local files.

Every Prism app is an IDE. There is no wall between "using" and "building." The difference between a consumer app and a developer tool is a single toggle — the **"Glass Flip."**

> Active migration plan: [`docs/dev/clay-migration-plan.md`](docs/dev/clay-migration-plan.md)
> Decisions: [`docs/adr/`](docs/adr/)
> Per-package context: each package's `CLAUDE.md`

## Status

Prism is mid-migration from a React/TypeScript client onto Clay (immediate-mode UI) + Rust. The plan, phases, and risk ledger live in [`docs/dev/clay-migration-plan.md`](docs/dev/clay-migration-plan.md).

- **Client** — Rust. Cargo workspace at the repo root. WASM for web, Tauri 2 (no-webview) for desktop, Tauri Mobile for iOS/Android.
- **Daemon** — Rust. `prism-daemon` crate. Runs as a Tauri sidecar on desktop, an in-process tokio subsystem on mobile, and is accessed remotely over WebSocket on web.
- **Relay** — Hono JSX SSR. Out of scope for the Clay migration; slated for its own rewrite, so don't over-invest in its current shape.

## Repo layout

```
prism/
├── Cargo.toml              # Workspace root
├── packages/
│   ├── prism-core/         # (Rust) shared domain logic — mid-port from TS
│   ├── prism-builder/      # (Rust) Puck replacement — component registry + Puck JSON reader
│   ├── prism-shell/        # (Rust → WASM / native) the Clay-based client
│   ├── prism-studio/       # Tauri 2 desktop shell (Rust, src-tauri/)
│   ├── prism-daemon/       # (Rust) local physics engine — Loro, Luau, VFS
│   └── prism-relay/        # (Hono JSX SSR) federated relay — stays JS, pending rewrite
├── docs/
│   ├── adr/                # Architecture decision records
│   └── dev/                # Active development plan (clay-migration-plan.md)
├── scripts/hooks/          # Claude Code dev hooks
└── $legacy-inspiration-only/  # Reference material from the pre-Clay era
```

## Quick start

### Prerequisites

- **Rust** ≥ 1.75 (`rustup`)
- **Tauri 2 CLI** — `cargo install tauri-cli --version "^2"`
- **Trunk** — `cargo install trunk` (for the WASM dev server)
- **Node.js** ≥ 20 + **pnpm** ≥ 9 (relay only)

### Dev

```bash
# Client (native, fastest iteration)
pnpm dev            # → cargo run -p prism-shell

# Client (web, WASM via trunk)
pnpm dev:web        # → trunk serve --config packages/prism-shell/Trunk.toml

# Desktop shell (Tauri 2 no-webview)
pnpm dev:studio     # → cargo tauri dev

# Relay
pnpm dev:relay      # → pnpm --filter @prism/relay dev
```

### Build

```bash
pnpm build          # cargo build --workspace --release && relay build
pnpm build:web      # trunk build --release
```

### Test / lint / format

Every command below goes through the unified `prism` CLI (see
`packages/prism-cli/CLAUDE.md`); the `pnpm` form is a thin alias.

```bash
prism test           # cargo test --workspace
prism test --all     # Rust workspace + relay Playwright e2e
prism test --e2e     # relay Playwright suite only
prism lint           # cargo clippy --workspace --all-targets -- -D warnings
prism fmt            # cargo fmt --all
prism fmt --check    # cargo fmt --all -- --check
prism --dry-run <…>  # print the expanded argv instead of executing
```

Relay unit tests are currently unwired (vitest isn't installed) and
will come back alongside the relay rewrite.

## Philosophy

- **Local-First** — the user's disk is the source of truth.
- **Every App Is an IDE** — consumer and developer share the same app (Glass Flip).
- **Ownership Over Convenience** — MIT-only deps, no vendor lock-in.
- **Blockchain-Free** — W3C DIDs + E2EE CRDTs, not ledgers or tokens.
- **Loro Is the Hidden Constant** — all state lives in Loro; editors are projections.
- **One Toolchain** — post-migration, everything outside the relay is a single `cargo build`.

## License

MIT
