# ADR-003: Dynamic Data Providers — external streams as first-class records

**Status**: Proposed
**Date**: 2026-04-13

## Context

The Puck visual builder in `@prism/studio` ships with ten dynamic widgets
(tasks, reminders, contacts, events, notes, goals, habits, bookmarks, timer
sessions, captures). Today each widget renders *local* records — objects
stored in the kernel's Loro CRDT. This is Tier 1, and it is already landed.

Tier 2 is the natural next step: the same widgets should be able to surface
records that live in **external systems** — Google Calendar events, Gmail
messages, iCloud reminders, Apple/Google contacts, OS notification center
items, RSS feeds, weather, GitHub issues, Linear tasks, a local filesystem
inbox, and so on. The user should compose an events widget that pulls from
their real calendar, or a tasks widget that mirrors their Todoist — without
each widget re-implementing auth, sync, normalisation, and error handling.

Helm (the legacy project at `$legacy-inspiration-only/helm/`) solved this
with a `ProviderRegistry` + per-provider normalisers + a background sync
engine driven by `syncToken`/`deltaLink` cursors. That pattern worked well
but was tightly coupled to Helm's own store and auth UI. We want the same
shape in Prism but built on our own substrates: kernel, Loro CRDT, daemon
(Tauri), and VFS credential store.

Key questions for this ADR:

1. Where do provider records live — in the kernel CRDT (like local records)
   or in a separate read-through cache?
2. What is the contract between a provider and the kernel?
3. How do we handle credentials and OAuth flows when the user's host can be
   the Tauri desktop shell, a Capacitor mobile app, or a pure browser SPA
   hitting a Relay?
4. How does sync scheduling work, and who owns it — the daemon, the kernel,
   or the provider itself?
5. How do widgets express that they want external data without knowing
   which provider supplied it?

## Decision

We will introduce a **Provider Registry** in `@prism/core/providers`, a
thin contract on top of the existing kernel CRUD, and a **daemon-owned sync
worker** that materialises external records into **the same Loro
collection** as local records. External records are tagged with
`source: { providerId, externalId, syncedAt }` in their `data` payload so
widgets can filter, edit rules know they're read-only by default, and the
merge engine can reconcile deltas.

A widget never talks to a provider directly. It queries
`kernel.store.allObjects()` filtered by `type`, exactly as it does today —
and a record it gets back might be local, might be synced from a provider,
or might be a local override of a provider record. The provider system is
invisible to widget code.

## Rationale

### 1. Provider contract lives in `@prism/core/providers`

A provider is an object implementing:

```ts
export interface ProviderDefinition<Config = unknown> {
  id: string;                        // "google-calendar", "rss", "ical"
  label: string;                     // Human name
  category: "calendar" | "tasks" | "contacts" | "mail" | "feed" | "weather" | "filesystem";
  produces: readonly string[];       // Record types this provider can emit ("event", "task", …)
  authKind: "oauth2" | "apiKey" | "basic" | "none";
  configSchema?: FacetDefinition;    // Rendered via the existing FacetView panel
  normaliseOne(raw: unknown): GraphObject | null;
  plan(config: Config, cursor: ProviderCursor | null): Promise<ProviderSyncPlan>;
}
```

Providers are *pure modules*. They know how to map their raw shape into a
`GraphObject` (via the normalisers) and how to express a delta plan. They
do **not** touch the kernel store, the network, or the credential vault.
That separation is the single most important decision in this ADR — it
makes providers trivially testable and lets the daemon decide *when* and
*where* to call them.

The reason for living in `@prism/core/providers` (not `@prism/studio`) is
the same reason the lens system moved into core: Flux, Lattice, Cadence and
other Prism apps should all be able to register providers against the same
registry. Studio will just be the most common consumer.

### 2. Records land in the kernel CRDT, not a side cache

We considered a separate "external records" cache that widgets would
query via a second API. We rejected it because:

- Every widget would need a fallback branch ("local list + remote list"),
  doubling widget code for a feature that's supposed to feel seamless.
- Loro already gives us conflict-free merges, which we want for the case
  where a user edits an offline override of a synced record.
- A single data plane keeps lens projection, selection, search, automation
  rules, and edit history working without per-system plumbing.

The downside is **store bloat** if a provider emits 50k records. We
mitigate this by (a) per-provider `maxRecords` config, (b) a time-windowed
sync (e.g. "only events within ±90 days"), and (c) widget-side
pagination that reads from `allObjects()` lazily.

### 3. Provenance lives on the record, not in a side table

Every synced record carries, inside its `data` map:

```ts
data: {
  source: {
    providerId: "google-calendar",
    accountId: "cal-user-123",
    externalId: "abcdefg@google",
    syncedAt: "2026-04-13T12:00:00Z",
    etag?: string,
  },
  ...normalisedFields
}
```

Widgets can filter on `obj.data.source?.providerId === "google-calendar"`
to show provider-specific views. Automation rules default to treating
`data.source`-bearing records as read-only unless the user explicitly
writes an override. The `source` key is reserved — normalisers must not
use it for provider-native fields.

### 4. Credentials use the existing Vault + daemon, not a new subsystem

Prism already has:

- **`VaultRoster`** — named identities/keyrings
- **`VfsManager`** — content-addressed blob store with locks
- **`@prism/daemon`** (Rust, Tauri) — OS-level secure storage access

Provider credentials will be stored as blobs in the VFS under a reserved
prefix (`providers/<providerId>/<accountId>.cred`) encrypted with the
active vault's symmetric key. OAuth flows are brokered by the daemon via
`invoke("provider_oauth_start", { providerId, accountId })`, which opens a
system browser, completes PKCE, writes the refresh token into the VFS, and
emits a `provider:credentials_ready` bus event.

**On web (Relay-hosted)**: the OAuth broker lives on the Relay instead.
The same daemon-IPC call becomes a fetch against
`/api/providers/:providerId/oauth`, and the token is stored Relay-side in
the user's encrypted vault blob. Studio frontend is unchanged — it always
talks to `kernel.providers.authenticate(...)` which is the common surface.

We **do not** put credentials in plaintext localStorage, in the kernel
CRDT (which is replicated), or in a per-provider module. The credential
substrate is shared.

### 5. Sync worker lives in the daemon (desktop/mobile) or a Relay cron job (web)

The sync worker is the thing that (a) asks the provider for a delta, (b)
writes the normalised records into the kernel store, (c) records the new
cursor, and (d) schedules the next fetch.

On **Tauri desktop** and **Capacitor mobile** the worker lives in the Rust
daemon. It:

- Subscribes to the kernel PrismBus via IPC to learn which providers have
  active widgets (don't sync what nobody's viewing)
- Runs per-provider on a configurable cadence (default: 5 min for
  calendar/tasks, 15 min for mail, 1 h for weather/feeds)
- Respects `syncToken`/`deltaLink`-style incremental semantics when the
  provider supports it; falls back to full refetch otherwise
- Writes records via a dedicated `kernel.providerSync.apply(records, cursor)`
  API that creates, updates, or tombstones in one batch and suppresses
  undo history (provider-driven changes should not clutter the undo stack)

On **pure web** (Studio served by a Relay with no daemon), the same worker
runs as a Relay-side job keyed by session. The kernel applies deltas as
they stream in over the existing Relay websocket channel.

**Both paths call the same `ProviderDefinition.plan()` function** with the
same `ProviderCursor`. The only thing that differs is where the network
fetch happens, and that is hidden behind `plan()`'s returned `SyncPlan`,
which is either `{ kind: "records", records }` (resolved) or
`{ kind: "fetch", request }` (host must execute).

### 6. Widgets stay dumb

No widget imports the provider registry. No widget reads `source`. A
widget does exactly what it does today — query by type, filter, render —
and the user wires up a provider binding at the *collection* level, not at
the *widget* level. A collection binding is a small record (type
`provider-binding`) created via a dedicated lens/panel that says "keep
objects of type `event` in sync with Google Calendar account X, windowed
to the next 90 days".

This has two big benefits:

- Users can mix local + synced records in the same widget without any
  special UI.
- Widget code is identical whether the data is local, synced, or both.

### 7. Priority order for first providers

Guided by what unlocks the most demo value per engineering hour:

1. **Google Calendar** (oauth2 + events) — covers "real live stream"
2. **Gmail** (oauth2 + messages as captures) — covers mail inbox
3. **Local filesystem inbox** (no auth, via daemon) — covers offline drop
4. **RSS / Atom feeds** (no auth, via fetch) — covers bookmarks/feeds
5. **Weather** (api key, via fetch) — covers polling/interval
6. **iCal public URL** (no auth) — covers "no OAuth but still live"
7. **GitHub** (oauth2) — covers issues as tasks
8. **Linear** (oauth2) — covers real task sync

Apple Calendar/Reminders/Contacts require the native daemon path and are
explicitly deferred to a later ADR once we have the Tauri macOS bridge.

## Consequences

### Positive

- Widgets stay the single source of truth for rendering. No external-data
  fork in widget code.
- Kernel, search, undo, selection, and automation keep working on provider
  records for free.
- Credentials use the existing VFS/vault/daemon stack — no new secret
  store to audit.
- The sync worker is centralised, so we can add global features (rate
  limiting, offline replay, metrics) once instead of per-provider.
- Web vs desktop vs mobile all share the same `ProviderDefinition`
  contract; the fetch path is the only thing that varies.
- Provider code is pure and testable (a normaliser test is just
  `normaliseOne(fixture) → snapshot`).

### Negative

- Kernel store size grows with the number of synced records. A naive
  Google Calendar full-year sync could add thousands of records. We
  mitigate with windowed sync + `maxRecords`, but this is a real cost.
- Conflict handling between local edits and provider updates is tricky.
  If a user renames a synced calendar event, do we push the rename back?
  For Phase 1 we say "provider records are read-only unless the user
  explicitly forks them"; write-back comes in a later ADR.
- OAuth flows differ on desktop (deep link / loopback) vs mobile (custom
  URL scheme) vs web (Relay redirect). The daemon handles desktop/mobile,
  the Relay handles web, and we need to keep both paths working. This is
  non-trivial surface area.
- Relay-hosted sync for web users means the Relay now holds user tokens,
  which changes its trust profile. We already encrypt-at-rest per vault,
  but operational review is required before we ship Gmail/Calendar on
  web.

### Mitigations

- **Store bloat**: enforce a hard cap on synced records per binding, with
  a visible warning when approaching it. Add a store-size Lens so users
  can see what's consuming space.
- **Conflict handling**: start by marking provider records with a
  read-only badge in the inspector and blocking edits. A future ADR will
  introduce override records (a local layer that masks fields on a synced
  record, mergeable via Loro).
- **OAuth complexity**: unify the three flows behind a single
  `kernel.providers.authenticate(providerId, accountLabel?)` method so
  widget and panel code never branches on host. Write E2E tests for each
  flow path (desktop Tauri, Playwright+Relay, Capacitor mocked WebView).
- **Relay token custody**: require an explicit opt-in for Relay-hosted
  provider sync. Default state for a Relay-hosted Studio is "local-only
  providers" until the user checks a consent box.

## Non-goals

- Real-time push from providers (webhooks, IMAP IDLE) — first version
  polls. Push is a later optimisation once polling works.
- Writing back user edits to providers — read-only in Phase 1.
- General-purpose HTTP "pull this URL, jq it into a record" — that's a
  different feature (scriptable providers) and deserves its own ADR.

## Open questions

- Should `provider-binding` records live at the workspace or the page
  level? Workspace feels right (same binding powers every view in the
  vault), but page-level bindings allow per-dashboard configs. Probably
  workspace-level with per-page overrides later.
- Do we need a "virtual collection" type for providers that don't map
  cleanly onto an existing record type (e.g. weather)? Or does the user
  just create custom entity types for those? Leaning toward the latter —
  users already author custom types via the Entity Builder panel.
- How do we surface rate-limit and failure state in the UI? Probably via
  the existing NotificationStore with a new `provider:error` channel and
  a dedicated provider-status indicator in the header bar.

## Next steps (if accepted)

1. Land `@prism/core/providers` with `ProviderRegistry`, `ProviderDefinition`,
   and `ProviderCursor` types plus a null/in-memory test provider.
2. Extend the kernel with a `providerSync.apply(records, cursor)` method
   that batches writes and suppresses undo history.
3. Write the `provider-binding` entity type and a small
   `provider-bindings-panel` lens for CRUD.
4. Build the daemon-side sync worker (Rust) with a single
   `PollProvider(providerId)` IPC entrypoint.
5. Implement the Google Calendar provider end-to-end as the reference
   impl, including the normaliser test fixture.
6. Write the Tier 2 E2E: "user adds a Google Calendar binding, the events
   widget surfaces real events within 60 seconds".
7. Revisit and harden credential flows for web/Relay hosts before
   exposing provider selection in the Relay UI.
