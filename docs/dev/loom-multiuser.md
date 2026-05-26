# Loom — Multi-User Backbone

Status: **Phases 1–8 landed.** WebSocket sync + presence are live on
`/ws`; the client wasm (`loom-wasm::LoomDoc`) and TS glue
(`editor/src/lib/{sync,auth,presence,cm-loro,workspaces,export,project}.ts`)
round-trip edits against the server. The React editor ships Cloud,
Remote, and Play sidebar panels driving the multi-user + co-playing
flows. The local FSA + remote workspace models are unified by
`LoomFolderBridge` (an Obsidian-style folder ↔ LoomDoc bridge) +
`.loom-workspace.json` manifest. Phase 7 hosts `loom_runtime::Playhead`
server-side and broadcasts the transcript over `/ws`. **Phase 8** turns
`loom-relayd` into a single-binary deployment: it now ships the static
React editor itself, defaults the client to same-origin, and the
`prism loom build` / `prism loom serve` CLI commands wrap the build +
boot loop.

## Goal

Keep the existing React/Vite/CodeMirror Loom editor as the front-end. Add
the smallest slice of the Prism stack alongside it that lets multiple
authors collaborate on `.loom` projects in real time, with multi-workspace
storage and authenticated sessions.

This is **not** an effort to rebuild the editor on top of `prism-shell`
or `prism-ui-runtime`. UI stays in React; the server side and the wire
layer come from Prism.

## What we take from Prism

### Client (extend `packages/loom/wasm`)

- **`prism-core` with `crdt` feature** — Loro `LoroDoc` / `LoroText` /
  `LoroMap`. Each workspace is a single `LoroDoc`; each `.loom` file is
  a `LoroText` child. Snapshots round-trip through the wire.
- **`prism-core::network::presence::types`** — `PresenceState` for cursor
  / selection broadcasts. Wire shape only; the manager lives server-side.
- **`prism-core::network::relay::message`** — envelope types so the TS
  WebSocket client decodes exactly what the server encodes.
- **`loom-parser` + `loom-syntax`** — already wired through
  `loom-wasm::parse`/`diagnose`/`emit_tmgrammar`.
- **`loom-runtime`** (v1 stretch / v2 must) — wasm bridge for local
  preview play. Not on the multi-user path in v1.

We do **not** take `prism-ui-runtime`, `prism-shell`, `prism-builder`,
`prism-dock`, `prism-studio`, `prism-ui-build`, or `prism-daemon`.

### Server (new crate `packages/loom/server`)

A thin axum server using just the relay machinery in `prism-core`. It
does **not** depend on `prism-relay` — the full relay drags
`prism-builder` (SSR portals) and 14 modules Loom doesn't need.

Module set:

| Module | Used for |
|---|---|
| `collection_host` (capability `relay:collections`) | per-workspace CRDT snapshot store |
| `password_auth` (capability `relay:password-auth`) | username/password registration + login |
| `capability_tokens` (capability `relay:tokens`) | scoped share-links into workspaces |

Modules we explicitly skip in v1: `federation`, `escrow`, `vault_host`,
`sovereign_portals`, `portal_templates`, `signaling`, `webhooks`,
`oauth`, `acme`, `blind_*`, `hashcash`, `peer_trust`, `timestamper`,
`relay_router`. They can be opted back in with one `use_module` line
each when a feature wants them.

## Resolved decisions

1. **v1 scope:** co-authoring. v2 adds co-playing on the same wire +
   workspace model.
2. **Auth:** start with `prism-core::network::relay::modules::password_auth`
   (PBKDF2-SHA256, already implemented). The client uses a hand-rolled
   TS wrapper hitting `/api/auth/{register,login,change}` — no
   BetterAuth dependency. Reason: BetterAuth is a TS stack designed
   around a TS server runtime; bolting it onto a Rust backend would
   either require running BetterAuth as a Node sidecar or duplicating
   its endpoint surface in Rust. Neither is worth it for a username +
   password flow. DID-based auth via `prism-core::identity` is a later
   upgrade once federation becomes interesting.
3. **Workspaces:** multi-workspace. The model: one workspace per
   `loro::LoroDoc`. Workspaces are listed/created via REST; access is
   gated by capability tokens.

## Workspace model

```
workspace (LoroDoc)
├── meta:   LoroMap   { name, createdAt, owners, ... }
├── files:  LoroMap   { "<path>" -> LoroText }
└── assets: LoroMap   { "<path>" -> LoroMap { hash, mime, ... } }
```

Renames are `move` ops on the `files` map; deletes are `remove`. The
file body is the `LoroText` itself — no separate "file content" key.

Asset blobs are out of band (uploaded over a future `/api/assets/*`
endpoint, content-addressed by sha256). The `assets` map only carries
metadata.

## Wire protocol

All envelopes share a `{ kind, payload }` shape. v1 uses the existing
`prism-core::network::relay::message` types where they fit and adds
Loom-specific ones for workspace lifecycle.

**HTTP** (`/api/*`):

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/health` | Liveness + module list |
| POST | `/api/auth/register` | username + password registration |
| POST | `/api/auth/login` | login → session token |
| POST | `/api/auth/change` | password change |
| GET | `/api/workspaces` | list workspaces visible to caller |
| POST | `/api/workspaces` | create workspace (returns id) |
| GET | `/api/workspaces/{id}` | workspace metadata |
| DELETE | `/api/workspaces/{id}` | delete workspace |
| GET | `/api/workspaces/{id}/snapshot` | export full Loro snapshot |
| POST | `/api/workspaces/{id}/snapshot` | import Loro snapshot |
| POST | `/api/tokens/issue` | mint scoped capability token |
| POST | `/api/tokens/verify` | check token validity |

**WebSocket** (`/ws`):

| In | Out | Purpose |
|---|---|---|
| `auth { token }` | `auth-ok { did }` / `error` | session upgrade |
| `subscribe { workspace }` | `snapshot { bytes }` | join, receive base |
| `update { workspace, bytes }` | `update { workspace, bytes }` | CRDT delta fan-out |
| `presence { workspace, state }` | `presence { workspace, peers }` | cursors |
| `unsubscribe { workspace }` | (none) | leave |
| `ping` | `pong` | keepalive |

CRDT deltas ride as opaque byte blobs from `LoroDoc::export_updates_since`.
The server doesn't introspect them; it appends them to the canonical
snapshot and rebroadcasts to other subscribers.

## Phased roadmap

### Phase 1 — Server skeleton *(current)*

- New crate `packages/loom/server` with `loom-relayd` bin.
- `LoomRelayState` carrying a `RelayInstance` with the three modules
  above + a `WorkspaceRegistry` (the multi-workspace map, see Phase 2
  for shape).
- `/api/health` returns `{ relayDid, modules, startedAt }`.
- Auth + workspace REST endpoints stubbed; full impl in Phase 2.
- Integration test via `tower::ServiceExt::oneshot` for health.

### Phase 2 — Workspace REST + auth

- `WorkspaceRegistry`: `HashMap<WorkspaceId, Workspace>` where each
  `Workspace` owns a `loro::LoroDoc` + ACL set.
- Wire `/api/auth/*` to the `RelayPasswordAuth` capability.
- Wire `/api/workspaces/*` to the registry.
- Wire `/api/tokens/*` to the `CapabilityTokenManager` capability.
- Sessions: signed token in `Authorization: Bearer ...`.

### Phase 3 — WebSocket sync *(landed)*

- `/ws` handler with `auth → subscribe → update` loop (`packages/loom/server/src/ws.rs`).
- One `WorkspaceHub` per workspace inside `WsHub`, each holding a
  `tokio::sync::broadcast::Sender<WsBroadcast>` (cap 1024).
- Auth accepts both session tokens (scope `session`) and capability
  tokens whose `scope` matches the subscribed workspace — share-link
  guests join the same socket the owner does.
- The opaque-blob `CollectionHost::import_snapshot` stays the server's
  canonical store; clients merge locally once Phase 4 lands `LoroDoc`.
- Backpressure + reconnection are deferred to Phase 4 once we have
  real numbers.

### Phase 4 — Client wasm + TS glue *(landed; store swap deferred)*

- `loom-wasm::LoomDoc` (`packages/loom/wasm/src/loom_doc.rs`) — wraps
  `loro::LoroDoc`, exposes `import_snapshot` / `export_snapshot` /
  `export_updates_since(vv)` / `current_version` / `apply_update` /
  `subscribe`, plus file CRUD on the `files: LoroMap` child
  (`get_text` / `set_text` / `splice_text` / `delete_file` /
  `rename_file` / `list_files`).
- `editor/src/lib/sync.ts` — `LoomSyncClient` over the native
  `WebSocket`. Sends `auth → subscribe`, debounces local commits
  (50ms default) into `update` envelopes via `export_updates_since`,
  applies remote `snapshot` / `update` / `presence` envelopes.
- `editor/src/lib/auth.ts` — REST wrapper for `/api/auth/{register,
  login,change}`; persists the session token under `loom.sessionToken`.
- `editor/src/lib/presence.ts` — `PresenceTracker` (framework-free
  observable) that the sync client feeds with server presence
  envelopes.
- `editor/src/lib/cm-loro.ts` — minimal CodeMirror 6 extension that
  splices change-set deltas into the `LoomDoc` and replays remote
  commits back into the editor. Cursor stability across remote edits
  is out of scope for this round.
- **Parallel cloud flow (landed)**: rather than swap the existing
  Zustand `workspace.ts` store wholesale, the editor now ships a
  sibling `session.ts` Zustand store + a Cloud sidebar panel
  (`components/cloud/CloudPanel.tsx`) + a Remote editor panel
  (`components/cloud/RemoteEditor.tsx`). The local FSA flow stays
  intact; signing in through Cloud, picking a workspace, and
  switching to the Remote panel routes the editor through `loomBinding`
  + `LoomDoc`. The dock's `panels.tsx` was split into a pure-component
  module plus `panel-registry.ts` to satisfy
  `react-refresh/only-export-components`.
- **Still deferred**: collapsing the two stores into one. Today users
  pick "local file root" or "remote workspace" at the panel level;
  cross-flow operations (drag a remote workspace's file out to the
  filesystem, or vice versa) are out of scope until we know how the
  divergent ACL/persistence rules should merge.

### Phase 5 — Presence *(server landed)*

- Server side rides the same `/ws` connection: each `WorkspaceHub`
  carries a `RwLock<HashMap<peer_id, PresenceState>>` keyed by a
  uuid-per-connection. Every `presence` envelope replaces that peer's
  entry and broadcasts the full peer list (originator included) so
  the joiner sees their own caret reflected.
- Departures are emitted on `unsubscribe` and on socket teardown.
- Client side (debounced selection → envelope, CodeMirror remote
  carets) lands with Phase 4.

### Phase 6 — File System Access fallback *(landed)*

- `editor/src/lib/export.ts` — `exportWorkspaceToFolder(doc, dir)`
  walks `doc.listFiles()` and materialises every path under a
  user-picked FSA directory, creating intermediate folders.
  `importFolderIntoWorkspace(doc, dir)` is the inverse: a recursive
  FSA walk that calls `doc.setText(path, body)` per leaf, with
  configurable prefix skips (`.git/`, `node_modules/`, `dist/`,
  `target/` by default).
- The Remote toolbar now hosts "Import…" / "Export…" buttons that
  drive these helpers via `pickDirectory()`. Status messages surface
  inline; the in-flight button disables to prevent overlapping runs.
- The relay's CRDT stays the canonical store — FSA is strictly a
  copy-out / copy-in seam, mirroring the spec's "FSA becomes
  export-only" intent.

### Phase 7 — co-playing *(landed)*

- `loom-server/src/play.rs` — `PlayHub` registers one `PlaySession`
  per workspace, owning a `loom_runtime::Playhead` advanced via
  `play-choice`. `play-start` carries the workspace's current file
  sources (`Vec<(path, source)>`), so the server doesn't need to
  introspect the opaque CRDT snapshot — clients ship their materialised
  view at session start.
- Wire envelopes added to `ws.rs`: `play-start`, `play-choice`,
  `play-stop` (incoming), `play-state` (outgoing — broadcasts the full
  transcript + pending choices to every workspace subscriber).
- `editor/src/components/cloud/PlayPanel.tsx` renders the transcript
  (Action, Dialogue, Scene, ChoiceTaken, Ended) plus the current
  choice prompt; `useSession.{startPlay,sendChoice,stopPlay}` drives
  the WS verbs.
- Dock surfaces the Play panel under ⌘⇧P alongside Cloud / Remote.

### Unification — folder ↔ workspace bridge *(landed)*

- `editor/src/lib/project.ts`:
  - **Manifest.** `.loom-workspace.json` carries the workspace id +
    relay URL so opening the folder later re-anchors the same
    multi-user session.
  - **`seedDocFromFolder`** reads every file on disk into the LoomDoc
    on first link (subsequent re-binds skip the seed and let the
    bridge reconcile from both sides).
  - **`LoomFolderBridge`** is the bidirectional sync:
    - LoomDoc commit → write changed paths to disk (debounced) + clean
      up files removed in the doc.
    - `FileSystemObserver` change → splice the new content back into
      the LoomDoc (recursive watch, with a `writing` counter that
      suppresses our own write echoes).
- `useSession.bindFolder` / `unbindFolder` + a Cloud-panel "Local
  folder" section drive the lifecycle. The folder stays canonical-
  feeling (Obsidian); the relay stays the wire layer (git-like remote).
- This was modelled on Prism's [Project Vault](./project-vault.md) but
  simplified — files on disk **are** the data (no `.loom-data/` blob
  store, no GraphObject layer). The CRDT only mirrors the folder for
  sync; persistence is the folder itself.

### Phase 8 — self-hosted single-binary editor *(landed)*

The Loom editor can now be hosted in full from one process. `loom-relayd`
serves the React build alongside the JSON / WebSocket API; the React
editor learns to default the relay URL to its own origin so a fresh
deploy needs zero client-side configuration.

**Server changes** (`packages/loom/server`):

- `tower-http` gains the `fs` + `cors` features. `build_router_with`
  accepts a `LoomServeConfig { editor_dist: Option<PathBuf>, cors:
  CorsMode }`:
  - When `editor_dist` is `Some(path)` the router gains a fallback
    handler that wraps `ServeDir::new(path)` and, on 404, re-reads
    `index.html` from disk. The custom fallback is necessary because
    `tower-http`'s `ServeFile` derives the served path from the
    request URI, which would 404 on nested SPA links like
    `/workspace/abc`.
  - `CorsMode::Permissive` (used by `--cors permissive` or the dev
    server case) attaches `tower_http::cors::CorsLayer::permissive()`
    so the Vite dev server on `:5173` can hit the relay on `:7878`.
  - `CorsMode::SameOrigin` (the default once the editor is served
    same-origin) keeps cross-origin off.
- The `/api/*` and `/ws` routes stay exactly as in Phases 1–7; only
  the fallback changes.

**Binary changes** (`packages/loom/server/src/bin/loom_relayd.rs`):

- New flags on `loom-relayd`:
  - `--editor-dist <path>` — directory of `vite build` output to serve
    (also reads `LOOM_EDITOR_DIST`).
  - `--cors permissive|same-origin` — opt in to cross-origin requests.
  - `--bind <addr:port>` already exists; the default is still
    `127.0.0.1:7878`. Operators who want LAN access pass
    `--bind 0.0.0.0:7878`.

**Editor changes** (`packages/loom/editor`):

- `useSession` defaults `relayUrl` to `window.location.origin` when no
  override is persisted *and* `location.origin` is not a Vite dev URL
  (`http://localhost:5173`, `:4173`, etc.). The "Relay URL" field on
  the Cloud panel still lets the user point a same-origin deploy at
  a different relay if they want to.
- The Vite dev experience is unchanged: hitting `:5173` continues to
  talk to `:7878` (with CORS now provided by the relay's permissive
  mode when launched via the dev recipe).

**CLI** (`packages/prism-cli`):

- `prism loom build` — runs `pnpm build` in `packages/loom/editor` to
  produce `editor/dist/`, then `cargo build -p loom-server` to produce
  the `loom-relayd` binary. `--ship` adds `--release` to the cargo
  step. `--skip-editor` / `--skip-server` cut the build in half when
  iterating.
- `prism loom serve [--bind <addr>] [--port <port>] [--editor-dist
  <path>] [--cors <mode>] [--build]` — execs `loom-relayd` with the
  editor dist pre-wired. `--build` runs the full build first; without
  it, the command auto-builds the editor when `dist/` is missing
  (suppress with `--no-auto-build`) and auto-builds the server binary
  when `target/<profile>/loom-relayd` is missing.

**What "host and serve the Loom editor in full" looks like**:

```
prism loom build           # one-time build of editor + relay
prism loom serve --bind 0.0.0.0:7878
# → open http://your.host:7878 from any browser, sign up, start authoring
```

A single TCP listener carries the static editor, the REST API, and the
WebSocket sync channel. No reverse proxy required.

### Phase 9+ — future

- Persistent server-side workspaces (today's `CollectionHost` is
  in-memory; first restart drops state).
- Transcript replay via `prism-core::network::session::transcript`.
- DID-based auth + federation.
- TLS termination (today the assumption is a reverse proxy sits in
  front for prod, or the network is trusted).

## What "active" means at each phase

- **End of Phase 1**: `cargo run -p loom-server` binds 127.0.0.1:7878,
  `curl /api/health` returns a JSON body with `relayDid` + module
  list. No multi-user yet.
- **End of Phase 3**: two `wscat` sessions, after subscribing to the
  same workspace, see each other's CRDT updates round-trip.
- **End of Phase 4**: the React editor, with the relay running locally,
  shows live edits from a second tab editing the same file.
- **End of Phase 5**: live cursors visible across tabs.

## Non-goals

- Replacing the React/Vite editor.
- Running `loom-runtime` on the server in v1.
- Federation, OAuth, escrow, signaling, SSR portals.
- File-system parity with the FSA flow (FSA becomes export-only).
- Live preview-play of stories across multiple users (v2).
