# Loom — Multi-User Backbone

Status: **Phase 1 in progress.** Co-authoring (v1) is the scope; co-playing
plus richer transcript/session features land in v2.

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

### Phase 3 — WebSocket sync

- `/ws` handler with `auth → subscribe → update` loop.
- One subscriber set per workspace + a tokio broadcast channel for
  fan-out.
- Server reconstructs the doc on import via `LoroDoc::import`.
- Backpressure + reconnection are deferred to Phase 4 once we have
  real numbers.

### Phase 4 — Client wasm + TS glue

- Extend `loom-wasm`:
  - `LoomDoc` — wraps `LoroDoc`, exposes `import_snapshot`,
    `export_update_since`, `apply_update`, `subscribe`, accessors for
    `LoroText`.
- TypeScript WebSocket client (`packages/loom/editor/src/lib/sync.ts`)
  that knows the envelopes.
- Swap the Zustand workspace store: reads come from `doc.getText(path)`,
  writes go through CRDT ops, CodeMirror gets a Loro binding extension.

### Phase 5 — Presence

- Client publishes `{ userId, cursor: { file, line, col } }` on every
  selection change (debounced).
- Server fans out via `presence` envelope.
- CodeMirror shows remote carets via a decoration set.

### Phase 6 — File System Access fallback

- Keep FSA as **export**: download a tarball / write workspace to
  local folder. Not the canonical store.

### Phase 7+ — v2

- `loom-runtime` execution server-side, presented as a new relay module
  hosting `Playhead`s with player events flowing over the same WS.
- `prism-core::network::session::transcript` integration for replay.

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
