# prism-relay

Rust-native relay server — the **Sovereign Portal** host. Built on
`axum` + `tower` + `tokio`, it publishes portals as real server-rendered
HTML (SEO-friendly, no JS required for L1), rendered through the same
`prism-builder` component registry the Studio uses.

> **Status:** Rust rewrite landed 2026-04-15. The legacy Hono JSX SSR
> code (`@prism/relay`, the 17-module plugin system, vitest/playwright
> suites) was ripped out of this folder and replaced with the Rust
> crate documented below. The old 17-module feature surface (federation,
> ACME, webhooks, admin dashboard, …) is tracked as a follow-on
> checklist at the bottom — the skeleton only implements L1 portals +
> SEO today.

## Build & Test
- `cargo build -p prism-relay` — lib + `prism-relayd` bin.
- `cargo run -p prism-relay --bin prism-relayd -- --bind 127.0.0.1:1420`
  — start the server.
- `cargo test -p prism-relay` — 19 unit tests + 8 integration tests
  (HTTP requests driven through `tower::ServiceExt::oneshot`, no TCP
  listener needed) + 1 doctest.
- Everything is also reachable through the unified CLI:
  `cargo run -p prism-cli -- dev relay` and
  `cargo run -p prism-cli -- build --target relay`.

## Architecture

```
┌──────────────────────────────────────────────┐
│  AppState                                    │
│  ├── PortalStore  (RwLock<HashMap>)          │
│  ├── ComponentRegistry  (from prism-builder) │
│  └── DesignTokens       (from prism-core)    │
└──────────────────────────────────────────────┘
                    │
                    ▼
        build_router(Arc<AppState>)
                    │
                    ▼
   axum::Router ── tower::ServiceBuilder ── tokio::net::TcpListener
                    │
                    ▼
         prism_builder::render_document_html
                    │
                    ▼
            real semantic HTML
```

- **No webview, no JSX, no Hono.** The old relay rendered portals with
  Hono's `jsxImportSource: "hono/jsx"`. The new one calls
  `prism_builder::render_document_html(doc, registry, tokens)` — the
  same `Component::render_html` trait the Studio will eventually share
  for its WASM web target.
- **Clay is not involved on the server side.** Clay is an immediate-mode
  pixel-oriented layout engine; its upstream `web/html` renderer is a
  client-side WASM DOM-as-canvas thing that produces only `<div>`/`<a>`/
  `<img>` with absolute-positioned CSS transforms. Search engines can't
  read that. The Sovereign Portal path is a completely separate
  semantic-HTML render target living in `prism_builder::html` +
  `prism_builder::render` + each component's `render_html` impl.
- **Component trait is two-target.** `prism_builder::Component` now
  carries `render_clay` (stub → `Value`, filled in later by Studio) and
  `render_html` (live — default impl wraps children in
  `<div data-component="…">`). Five built-in components ship with the
  relay: `heading`, `text`, `link`, `image`, `container`.
- **Portal levels.** Only L1 (static read-only HTML) is implemented in
  the skeleton. L2–L4 (live patching / forms / hydration) land as
  follow-on work — the routing + render layer already has the seams.

## Routes

| Method | Path | Status | Description |
|---|---|---|---|
| GET | `/healthz` | ✅ | 200 `OK`, for load balancers |
| GET | `/` | ✅ | Portal index (lists public portals) |
| GET | `/portals` | ✅ | Same as `/` — canonical alias |
| GET | `/portals/:id` | ✅ | Renders a single portal as HTML. 404 if private or missing |
| GET | `/sitemap.xml` | ✅ | Auto-generated XML sitemap from public portals |
| GET | `/robots.txt` | ✅ | Allows `/portals/`, disallows `/api/` |

All handlers render through `render_document_html`, which walks the
portal's `Node` tree and dispatches each `Node::Component` into the
registered component's `render_html` method. Unknown component ids
return `RenderError::UnknownComponent(id)` — propagated to the HTTP
layer as `500 Internal Server Error` so crawlers and admins both see
the failure loud.

## Modules

- `components` — five built-in `Component` impls: `HeadingComponent`
  (maps `level 1..=6` → `<h1>`..`<h6>`), `TextComponent` (`<p>`),
  `LinkComponent` (`<a href>` with attribute escape), `ImageComponent`
  (`<img src alt>`, self-closing), `ContainerComponent`
  (`<div class="prism-container">` with child walk). `register_builtins`
  installs all five.
- `portal` — `PortalLevel` enum (L1–L4), `PortalMeta`, `Portal`,
  `PortalStore` (a `RwLock<HashMap<PortalId, Portal>>`). Methods:
  `upsert`, `get`, `list`, `list_public` (filtered + sorted by id).
- `routes` — `build_router(Arc<AppState>) -> Router`. All page builders
  (`portal_index_page`, `portal_detail_page`, `sitemap_xml`) are
  free functions with their own unit tests so the SSR layer can be
  exercised without a running HTTP stack.
- `state` — `AppState { portals, registry, tokens }`. `AppState::new`
  wires the registry with `components::register_builtins`;
  `AppState::with_sample_portals` additionally seeds a public
  `welcome` portal (container → heading + text + link) and a private
  `draft` portal so tests have real content to crawl.
- `bin/prism_relayd.rs` — clap CLI with `--bind <addr>` (default
  `127.0.0.1:1420`), `tokio::main`, `tracing_subscriber::fmt`, and
  `axum::serve` with `with_sample_portals`.

## SEO

- `<title>` + `<meta name="description">` pulled from `PortalMeta`.
- OpenGraph and Twitter Card metadata get added as the render layer
  grows — the portal skeleton exposes `PortalMeta { title, description,
  og_image, published_at }` already.
- `/sitemap.xml` iterates `PortalStore::list_public()` and emits one
  `<url>` per public portal.
- `/robots.txt` is static text — `User-agent: *`, `Allow: /portals/`,
  `Disallow: /api/`, `Sitemap: /sitemap.xml`.

## Tests

- **Unit** — 19 tests across `portal.rs` (4: construction, visibility
  filter, upsert overwrite, deterministic ordering), `components.rs`
  (7: each component's render_html output, container recursion, attr
  escape on link), `state.rs` (2: default boot, sample seed),
  `routes.rs` (6: each page builder in isolation — index empty, index
  with public portals, detail happy-path, detail 404 on private, detail
  404 on missing, sitemap shape).
- **Integration** — 8 tests in `tests/routes.rs` drive the real axum
  router via `tower::ServiceExt::oneshot` + `http_body_util::BodyExt`,
  asserting on status codes, `content-type`, and rendered HTML bodies.
  No port binding needed, so CI runs green under sandboxing.
- **Docs** — the one doctest on `AppState::with_sample_portals` seeds
  the default state and asserts both portals round-trip through
  `PortalStore::get`.

## Follow-on modules (legacy Hono relay surface)

The old Hono relay shipped 17 plug-in modules. Rust ports are scoped as
follow-on work so the skeleton stays small and reviewable. Legend: ✅
shipped in skeleton, ⏳ next-up, ❌ not started.

| Module | Status | Notes |
|---|---|---|
| sovereign-portals (L1 static) | ✅ | Present in skeleton |
| seo (sitemap + robots) | ✅ | Present in skeleton |
| sovereign-portals (L2 live patch) | ❌ | Needs WS upgrade + snapshot diff |
| sovereign-portals (L3 form submit) | ❌ | Needs ephemeral-DID auth |
| sovereign-portals (L4 hydration) | ❌ | Needs `window.__PRISM_PORTAL__` JS shim |
| blind-mailbox | ❌ | Port from TS |
| relay-router | ❌ | Port from TS |
| relay-timestamp | ❌ | Port from TS |
| blind-ping | ❌ | APNs/FCM push transports |
| capability-tokens | ❌ | Shared trait with `prism-core` |
| webhooks | ❌ | Needs `reqwest` + timeout |
| collection-host | ❌ | Needs `loro` integration |
| hashcash | ❌ | Pure compute |
| peer-trust | ❌ | Federation dep |
| escrow | ❌ | PBKDF2 via `ring` or `argon2` |
| federation | ❌ | Peer discovery + gossip |
| acme-certificates | ❌ | `instant-acme` candidate |
| portal-templates | ❌ | Trivial CRUD |
| webrtc-signaling | ❌ | SFU rooms |
| vault-host | ❌ | Owner-authed blob store |
| password-auth | ❌ | PBKDF2-SHA256 |
| admin dashboard | ❌ | Separate `prism-admin-kit` port |
| metrics (`/metrics`) | ❌ | `metrics-exporter-prometheus` |

No relay code is gated behind cargo features yet — add per-module
features when the surface grows past what CI can type-check in one
pass.

## Migration notes

- The old folder contained ~30k lines of TypeScript across `src/`,
  `tests/`, `e2e/`, Docker/compose files, and the 17-module plugin
  system. Every one of those files was deleted as part of this
  rewrite; `pnpm-workspace.yaml` no longer lists `packages/prism-relay`.
- The `@prism/relay` package name / `prism-relay` CLI / port 1420 all
  carry over so downstream tooling (Studio sidecar spawner, docs,
  operator runbooks) keeps working once the feature surface is back.
- The root `package.json` no longer installs vitest/playwright for
  this subtree; `prism test` is Rust-only now (`cargo test --workspace`
  picks up `prism-relay`'s unit + integration tests automatically).
