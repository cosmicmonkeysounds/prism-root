# Declarative Refactorings

> Catalogue of "we're defining the same thing twice (or more)" patterns
> in Prism, ranked by payoff. Each entry follows the `luau-integration-plan.md`
> shape: current state, design, status, work breakdown.

The unifying goal mirrors the Luau plan's first principle — **single
source of truth**: one Rust definition, every other surface (UI render
target, codegen, RPC wire, type stubs, intellisense) derived.

---

## 1. `Block` trait collapse + `#[derive(PrismBlock)]`  ✅ all three phases shipped

### Current state

Every built-in block in `prism-builder` is *two* impls with the same
`ComponentId`:

| Crate file | Trait | What it emits |
|---|---|---|
| `prism-builder/src/starter.rs` (1547 lines) | `impl Component for FooComponent` | `.slint` DSL via `SlintEmitter` for Studio's live builder |
| `prism-builder/src/html_starter.rs` (1012 lines) | `impl HtmlBlock for HtmlFoo` | HTML via `Html` for `prism-relay` SSR |

The non-renderer methods (`id`, `schema`, `signals`, `variants`,
`help_entry`, `toolbar_actions`) are typically identical or trivially
re-aliased between the two impls — both pull from `crate::schemas`
and `crate::variant::presets`. The "Adding a new block type"
checklist in `prism-builder/CLAUDE.md` literally says steps 1–4
must be done in both files.

`CoreWidgetComponent` already proves the converged shape works:
`WidgetContribution` carries a `TemplateNode` tree, and the walker
fans out to either render target. But it only covers core engine
contributions, not the 16 hand-written builtins.

### Design

Two phases.

**Phase 1: trait collapse.** Merge `Component` and `HtmlBlock` into a
single `Block` trait with both render methods. One struct per block.
Shared `id`/`schema`/`signals`/`variants`/`help_entry` written once.

```rust
pub trait Block: Send + Sync {
    fn id(&self) -> &ComponentId;
    fn schema(&self) -> Vec<FieldSpec>;
    fn signals(&self) -> Vec<SignalDef> { common_signals() }
    fn variants(&self) -> Vec<VariantAxis> { vec![] }
    fn help_entry(&self) -> Option<HelpEntry> { None }
    fn toolbar_actions(&self) -> Vec<ToolbarAction> { vec![] }

    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> { /* default: transparent Rectangle */ }

    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> { /* default: <div> with children */ }
}
```

The single registry holds `Arc<dyn Block>`. `ComponentRegistry` and
`HtmlRegistry` keep separate handles into the same trait object set
(or merge into one registry exposing both views).

**Relay dep graph constraint.** `prism-relay` currently keeps the
Slint feature off so the relay's transitive deps stay Slint-free.
The collapsed trait keeps `render_slint` behind a feature gate, or
takes a generic `out: &mut dyn SlintLike` so the relay never
instantiates the Slint emitter.

Resolution: `render_slint` lives behind `cfg(feature = "interpreter")`
on the trait itself; the relay sees a smaller trait. Or: keep the
Slint emitter type stub-only when the feature is off.

**Phase 2: derive macro.** Once `TemplateNode` is rich enough to
express every builtin (currently it can't — `Image` needs
asset-path resolution, `Text` needs conditional `<a>` wrapping,
`Tabs` needs interactive state), add `#[derive(PrismBlock)]` that
takes a `fn template(&self, props: &Value) -> TemplateNode` and
generates both render impls.

```rust
#[derive(PrismBlock)]
#[block(id = "container")]
struct ContainerBlock;

impl ContainerBlock {
    fn schema() -> Vec<FieldSpec> { schemas::container() }
    fn template(props: &Value, children: &[Node]) -> TemplateNode { ... }
}
```

Until then, manual `Block` impls are fine — Phase 1 is the value
extraction, Phase 2 is icing.

### Status

- ✅ Phase 1: `Block` trait in `prism-builder/src/block.rs` with
  blanket impls for `Component` + `HtmlBlock`. All 16 starter
  builtins (Text, Image, Container, Form, Input, Button, Code,
  Divider, Spacer, Columns, List, Table, Tabs, Accordion, GraphView,
  plus the card prefab and facet which keep their bespoke `Component`
  + `HtmlBlock` impls intentionally) migrated. `html_starter.rs`
  shrank from 967 → 467 lines (12 duplicated structs deleted; the
  registrar now points at the unified `*Block` types in `starter.rs`).
  `cargo test --workspace` clean. `cargo clippy -p prism-builder`
  clean.
- ✅ Phase 2: `TemplateNode` extended with three new variants —
  `Image { src_field, alt_field, fit }` (resolves an `AssetSource`
  prop through `ctx.asset_paths` for Slint and `/asset/{hash}` for
  HTML), `Link { href_field, child }` (conditionally wraps the child
  in `<a>` when the field is non-empty; Slint passes through), and
  `Children` (emits the surrounding block's children slot at this
  position). Walkers (`render_template_node` / `render_template_html`)
  now thread `children: &[Node]` through every variant and are
  re-exported from `prism-builder` so external blocks can call them.
  `#[derive(PrismBlock)]` shipped in `prism-luau-derive`: takes
  `#[block(id = "...")]`, expects `Self::schema() -> Vec<FieldSpec>`
  + `Self::template(&Value, &[Node]) -> TemplateNode`, and emits the
  full `Block` impl wired through the template walkers. Integration
  tests in `prism-builder/tests/derive_macros.rs` cover the derive
  end-to-end (id+schema surface and HTML render through the walker).
- ✅ Phase 3: `CoreWidgetComponent` + `CoreWidgetHtmlBlock` collapsed
  into a single `CoreWidgetBlock: Block` (`prism-builder/src/core_widget.rs`).
  The blanket `Block`→`Component` and `Block`→`HtmlBlock` impls in
  `block.rs` mean one `Arc<CoreWidgetBlock>` registers into both
  registries — `register_core_widgets` and `register_core_html_widgets`
  both wrap each `WidgetContribution` in the same `CoreWidgetBlock`
  type. The `WidgetContribution` data shape stays as the pure-data
  declaration that core engines emit (45 contributions across
  `domain::*::widget_contributions()`, `interaction::*::widget_contributions()`,
  and `widget::view_contributions()`); the duplicate trait impl is
  what's gone. 359 builder + 6 derive tests pass; clippy clean.

### Work breakdown

| Crate | Work |
|---|---|
| `prism-builder` | New `block.rs` with `Block` trait. Migrate the 16 builtins from `starter.rs` + `html_starter.rs` into one file each (or one merged catalog file). Update `ComponentRegistry` + `HtmlRegistry` to take `Arc<dyn Block>`. |
| `prism-relay` | Confirm `render_slint` is feature-gated; no Slint deps leak into relay's graph. |
| `prism-shell` | None — the registries' public API stays the same. |
| `prism-builder` tests | All 367+ existing tests must pass; add round-trip tests that the same `Block` produces matching schema across both render paths. |

---

## 2. `#[derive(PrismField)]` — unify the four field schemas  ✅ derive shipped, all default schemas migrated

### Current state

There are at least four parallel "typed field with default + UI hints"
shapes in the workspace:

| Type | Where | Used for |
|---|---|---|
| `FieldSpec` (`prism-builder/src/registry.rs`) | Component prop schema for the Studio property panel | Renders the editor for each prop in a node's `props` map |
| `SchemaField` (`prism-builder/src/facet/schema.rs`) | Facet-record column definitions | Validates objects against a typed shape |
| `ExposedSlot` (`prism-builder/src/prefab.rs`) | Pinned inner-node props as instance-editable fields on a prefab | Field overrides on a prefab instance |
| `SignalSpec.payload_fields` (`prism-core/src/widget/`) | Signal payload field declarations | Drives Luau type stubs + dispatch |

These overlap structurally: name, label, kind (text / number / select /
file / boolean / enum), default, optional bounds, optional select
options. They diverge in subtle ways — different defaults for the
same kind, different naming conventions — which has been a source of
quiet bugs.

### Design

Single derive on a Rust struct; emits all four shapes, plus the Luau
type stub via the existing `prism-luau-derive` infra.

```rust
#[derive(PrismField)]
struct CardProps {
    #[field(label = "Title", default = "")]
    title: String,
    #[field(label = "Body", default = "", multiline)]
    body: String,
    #[field(label = "Variant", select = ["primary", "secondary"], default = "primary")]
    variant: String,
}
```

Generates: `CardProps::field_specs() -> Vec<FieldSpec>`,
`CardProps::schema_fields() -> Vec<SchemaField>`,
`CardProps::exposed_slots(ids: &SlotIdMap) -> Vec<ExposedSlot>`,
plus Luau `type CardProps = { title: string, body: string, ... }`.

### Status

- ✅ Audit: `SchemaField` is already collapsed into `FieldSpec` (the
  table in the design above is stale on this point — `FacetSchema`
  carries `Vec<FieldSpec>` directly). `ExposedSlot` wraps a
  `FieldSpec` verbatim. `SignalPayloadField` lives in
  `prism-core::language::syntax` and is structurally separate from
  the property-panel field shape, so it is out of scope for this
  derive.
- ✅ Canonical kind enum: kept `FieldKind` closed for built-ins
  (`Text` / `Number` / `Boolean` / `Select` / etc.) and added
  `FieldKind::Custom { tag, data }` as the plugin extension hatch,
  bridged through `FieldKindRegistry` + `FieldKindContribution` in
  `prism-core::widget::field`. Same compromise `ScriptNodeKind`
  already made — built-ins stay enum arms, plugins go through the
  registry.
- ✅ Derive shipped in `prism-luau-derive` as
  `#[derive(PrismField)]`. Field attribute surface:
  `label`, `default` (lit), `multiline`, `required`, `group`,
  `help`, `min`, `max`, `select("a", "b", …)` /
  `select = "a,b,c"`. Type→kind mapping covers `String`/`bool`/
  `f32`/`f64` plus all signed and unsigned ints. Generated
  function: `Foo::field_specs() -> Vec<FieldSpec>`.
- ✅ Rich-kind coverage: derive grew an explicit `kind = "..."`
  attribute escape-hatch covering `color`, `date`, `datetime`,
  `duration`, `file` (with `accept = "..."`), `currency` (with
  `currency = "USD"`), and `calculation` (with `formula = "..."`).
  These bypass type-based inference because they need attribute-driven
  config. Type-based inference still drives the simple kinds.
- ✅ Migration: every default schema in
  `prism-builder::schemas` is now a `#[derive(PrismField)]` struct —
  `text`, `image` (file kind), `container`, `form`, `input`, `button`,
  `code`, `spacer`, `columns`, `list`, `table`, `tabs`, `accordion`,
  `facet`. `divider` stays as `vec![]` since it has no fields.
  361 unit + 7 derive tests pass; clippy clean.

---

## 3. `#[daemon_command]` proc-macro  ✅ macro shipped, every default module migrated

### Current state

Every daemon module under `packages/prism-daemon/src/modules/`
hand-rolls the same shape:

1. Define wire types with `serde::{Deserialize, Serialize}`.
2. Implement `DaemonModule::install(&self, builder: &mut DaemonBuilder)`.
3. Inside `install`, call `builder.register(...)` for each command,
   passing a closure that takes a `JsonValue` payload, deserializes
   it, dispatches to a typed handler, re-serializes the result, and
   maps errors to `CommandError`.
4. Set the permission tier per command.
5. Manually maintain a `.d.luau` stub (or skip and lose intellisense).
6. Manually maintain the TS-side wire types if a TS consumer exists.

`actors_module.rs` and `vfs_module.rs` are the worst offenders.
`admin_module.rs` is the most boilerplate-heavy at the registration
seam.

### Design

```rust
#[daemon_command(id = "actors.spawn", permission = User)]
fn spawn(state: &ActorsState, req: SpawnReq) -> Result<SpawnResp, ActorError> {
    // typed handler — no JSON, no closures
}
```

Derives: registration entry on `DaemonBuilder`, JSON wire glue,
`CommandError` mapping from the user's error type, Luau binding
via `prism-luau-derive`, and a `.d.luau` stub line.

A module-level `#[daemon_module(id = "actors")]` collects every
annotated command in the same module into the install impl.

### Status

- ✅ Typed-handler stepping stone: `prism-daemon/src/typed_command.rs`
  ships a `CommandRegistryExt` trait with
  `register_typed` / `register_typed_with_permission` /
  `register_typed_user`. The handler signature is
  `Fn(Req) -> Result<Resp, E>` where `Req: DeserializeOwned`,
  `Resp: Serialize`, `E: Display`. JSON marshaling, command-name
  attribution on errors, and permission gating are derived.
- ✅ Smoke-test migration: `build.run_step` in `build_module.rs`
  collapsed from a 12-line `register` closure to a 7-line
  `register_typed` call. All 108 daemon lib tests + 9 integration
  tests pass.
- ✅ Wave 1 migrations: `watcher` (3 cmds), `crdt` (4 cmds),
  `luau` (1 cmd), `admin` (1 cmd, with public typed `AdminSnapshot`),
  and `actors` (6 cmds) all collapsed onto `register_typed` /
  `register_typed_user`. Inline `json!()` response literals replaced
  with module-private response structs (`WatchResp`, `BytesResp`,
  `SpawnResp`, …); `admin` exposes `AdminSnapshot` / `HealthSnapshot`
  / `Metric` / `ServiceEntry` as crate-public types so transports
  can deserialize the snapshot without round-tripping JSON. 108
  lib + 12 integration + 2 stdio_bin tests pass; clippy clean.
- ✅ Wave 2 migrations: `debug` (9 cmds), `crypto` (6 cmds), and
  `vfs` (6 cmds) collapsed onto `register_typed`. The byte-array-heavy
  modules grew typed response structs alongside their existing
  arg structs (`KeypairResp`/`EncryptResp`/`DecryptResp` for crypto,
  `PutResp`/`HasResp`/`StatsResp` for vfs, `LaunchResp`/`StoppedResp`/
  `InspectResp` for debug). `vfs.has`'s conditional `size` field uses
  `#[serde(skip_serializing_if = "Option::is_none")]` to preserve the
  pre-migration wire shape; `vfs.stats` adds `backend` as a struct
  field rather than the post-hoc JSON splice it was. Per-command
  `parse()` / `decode_*` helpers shrunk from `(payload, command)`
  callsites to plain `String`-error helpers since `register_typed`
  now owns command-name attribution. 108 lib + 12 integration + 2
  stdio_bin tests pass; clippy clean.
- ✅ Attribute-macro shape picked: function attribute (not derive on a
  unit struct). Arity decides the wiring — `fn(req)` is stateless,
  `fn(&state, req)` threads an `Arc<StateType>` through the closure.
  Required `id = "x.y"`; optional `permission = User|Dev` (default
  `Dev`). Returns must be `Result<_, _>`; the macro errors at compile
  time otherwise.
- ✅ Shipped in `prism-luau-derive` as `#[daemon_command]`. Each
  annotation emits the original function plus a sibling
  `register_<fname>` helper that calls
  `register_typed_with_permission` against the typed handler. The
  daemon crate adds `extern crate self as prism_daemon;` so the
  macro's `::prism_daemon::…` paths resolve when the macro is used
  inside the daemon itself.
- ✅ Migrations: every default-feature module is on the macro —
  `watcher` (3), `crdt` (4), `luau` (1), `build` (1), `crypto` (6),
  `actors` (6), `debug` (9), `vfs` (6), `admin` (1 at `User` tier).
  Admin's handler-captured registry handle moved into a single
  `AdminState { started_at, module_ids, registry }` struct so the
  closure stays one captured `Arc<AdminState>` instead of three
  independent moves. The `typed_command::CommandRegistryExt` trait
  remains the underlying glue and is still exercised directly by
  hand-written closures in tests. 108 lib + 12 integration + 12
  stdio + 2 ipc + 3 macro tests pass; clippy clean.
- 🟡 Feature-gated modules outside the default surface
  (`whisper`, `conferencing`) are unmigrated — they need to opt-in
  on their own build profile, but the macro shape is proven to
  handle every command pattern those modules use.

---

## 4. `#[visual_node]` for the graph node catalog  🟡 attribute shipped, migration pending

### Current state

The visual scripting bridge in `prism-core/src/language/visual/`
compiles `ScriptGraph` to Luau via `LuauVisualLanguage`. Each
node kind today lives in at least three places: palette UI metadata
(category, label, port shapes), graph→Luau codegen (template
strings), and runtime semantics (the actual Luau body).

### Design

A single annotation on a Rust function whose signature *is* the
node — params become input ports, return type becomes output
port, body is mirrored in Luau.

```rust
#[visual_node(category = "Math", label = "Add")]
fn add(a: f64, b: f64) -> f64 { a + b }
```

Generates: palette entry, codegen template (`local r = a + b`),
runtime semantics (the Luau-side stdlib expansion if invoked from
a graph eval). Body parity is enforced by an explicit Luau snippet
attribute when the Rust body can't trivially translate.

### Status

- ✅ Attribute macro shipped as `#[visual_node(category, label,
  luau)]` in `prism-luau-derive`. Annotates a free Rust function
  whose signature determines port shapes — params become typed
  `PortDef::Input`s, the return type becomes a `result` output
  port, primitive types map to `DataType::Number` / `String` /
  `Boolean`, everything else falls to `Any`. Emits a sibling
  `pub fn <FN_NAME_UPPER>_NODE_DEF() -> NodeKindDef` whose
  `kind` is `ScriptNodeKind::Custom(fn_name)` and whose
  `description` defaults to a Luau call template (overridable via
  `luau = "..."`).
- ⬜ Migration: the 14 hand-rolled `palette_entry(...)` calls in
  `prism-core::language::luau::visual::node_palette` still live as
  closed-enum entries (`LocalAssignment`, `Branch`, `DaemonCommand`,
  …). These are language-control-flow nodes, not pure functions, so
  they don't fit the `#[visual_node]` shape — the derive is for
  user-extended math/logic/utility palette entries that round-trip
  to a single Luau call.
- ✅ Palette aggregation wired: `LuauVisualLanguage` carries a
  `palette_extensions: Vec<NodeKindDef>` registry, populated via
  `with_node_def(NodeKindDef)` (builder) or `register_node_def(&mut)`
  (in-place). `node_palette()` returns built-ins concatenated with the
  registry. No global `inventory`/`OnceCell` — hosts construct an
  instance per language surface and register the derived `*_NODE_DEF()`
  fns they want exposed. Covered by
  `luau_palette_aggregates_derived_node_defs_with_builtins` in
  `prism-builder/tests/derive_macros.rs`.

---

## 5. `#[derive(SlintBinding)]` — Rust↔Slint property bridge  ✅ derive shipped, first migration landed

### Current state

`prism-shell` shuttles state between `slint::ComponentHandle` and Rust
`AppState` with hand-written `set_*` getters and `on_*` callback
registrations per property. `slint::include_modules!` already gives
the Slint type info at compile time, so a derive can validate field
names match the `.slint` global's exported props.

### Design

```rust
#[derive(SlintBinding)]
#[slint(global = "AppGlobal")]
struct AppBindings {
    selected_node_id: String,
    is_dragging: bool,
    zoom: f64,
}
```

Generates: `bind(handle: &AppGlobal, state: &AppState)` that wires
every field through the Slint handle in both directions. Callbacks
declared in `.slint` get matched to Rust fns by name.

### Status

- ✅ Derive shipped in `prism-luau-derive` as
  `#[derive(SlintBinding)]`. Struct attribute
  `#[slint(global = "...")]` parses its argument as a Rust type, so
  both Slint handle shapes work directly: root component handles
  (`global = "AppWindow"`) and generated global borrow types
  (`global = "AppGlobal<'_>"`). Emits two methods on the host
  struct:
  - `bind_to(&Target)` — calls `handle.set_<field>(self.field.clone().into())` for every field.
  - `pull_from(&mut self, &Target)` — calls `self.field = handle.get_<field>().into()` for every field.
  The `.into()` coercion punts type-mapping to Slint's generated
  `Set*` traits — Rust `String` flows through `slint::SharedString`,
  primitives flow as-is. Callbacks (`on_*`) are deliberately not in
  scope; the derive is for state shuttling only.
- ✅ Direction flags + per-field overrides: struct-level
  `#[slint(push_only)]` / `#[slint(pull_only)]` skip the unwanted
  half (push-only is required when the target Slint property is `in`
  rather than `in-out`, since Slint only generates a `set_*` for
  `in`). Per-field `#[slint(skip)]` drops a field from both directions,
  and `#[slint(rename = "other")]` lets the Rust field name diverge
  from the Slint property name.
- ✅ Survey of the hand-written `set_*` call sites in
  `prism-shell/src/app/sync.rs` (108 calls): most target Slint `in`
  properties on the root `AppWindow`, so the migration target is
  push-only mirrors keyed off `AppState` rather than the in-out
  globals the derive was originally designed for. `commands.rs` has
  zero direct `set_*` calls — every shell mutation flows through
  `sync_ui_from_shared`, so the migration scope is exclusively
  `sync.rs`.
- ✅ First migration: `ChromeBindings` in
  `prism-shell/src/app/sync.rs` collapses the four shell-chrome
  setters (`set_show_activity_bar` / `set_show_left_sidebar` /
  `set_show_right_sidebar` / `set_viewport_width`) into a single
  `ChromeBindings::from(state).bind_to(window)`. Push-only because
  the Slint properties are `in`. Renaming an `AppState` field
  without updating the Slint property of the same name now fails
  to compile rather than silently dropping the push.
- 🟡 Wider migration deferred: the remaining ~100 `set_*` call sites
  are interleaved with derived state (`SharedString::from(match …)`,
  conditional pushes guarded by `if let Some(...)`, model-driven
  pushes that compute a count alongside the value). Each cluster
  needs its own mirror struct with a `From<&AppState>` constructor;
  `ChromeBindings` is the template. Worth tackling cluster-by-cluster
  once the `app/` decomposition (#21) splits `sync.rs` along
  panel-feature lines.

---

## 6. `#[daemon_module]` — auto-generate `install()`  ✅ shipped, every default module migrated

### Current state

`#[daemon_command]` generates a `register_<fn>()` sibling for each
command, but the `DaemonModule::install()` body still calls each
`register_*` manually. Every module follows the identical shape:
acquire state/registry from the builder, call `register_<cmd>` for
each command.

```rust
impl DaemonModule for CrdtModule {
    fn id(&self) -> &str { "prism.crdt" }
    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        let manager = builder.doc_manager_slot()
            .get_or_insert_with(|| Arc::new(DocManager::new())).clone();
        let registry = builder.registry().clone();
        register_write(&registry, manager.clone())?;
        register_read(&registry, manager.clone())?;
        // …
        Ok(())
    }
}
```

### Design

A `#[daemon_module(id = "prism.crdt", …)]` annotation on the module
struct emits the full `DaemonModule` impl. Proc-macros only see their
annotated item, so the command list is explicit (`commands(write,
read, …)`) rather than scraped from the surrounding module — keeping
the dependency graph auditable and matching the pattern
`widget_providers!` will use in #9. State acquisition has three
declarative shapes:

```rust
// Stateless — every register_<cmd> takes only &CommandRegistry.
#[daemon_module(id = "prism.crypto",
    commands(keypair, derive_public, shared_secret, encrypt, decrypt, random_bytes))]
pub struct CryptoModule;

// Slot-backed — manager stashed on a DaemonBuilder::*_slot() accessor
// so hosts can inject a preconfigured one before build().
#[daemon_module(
    id = "prism.crdt",
    slot = doc_manager_slot,
    default = || Arc::new(DocManager::new()),
    commands(write, read, export, import),
)]
pub struct CrdtModule;

// Direct state — manager constructed inline at install time. Used
// when no slot accessor exists (debug sessions are per-install, not
// host-injectable).
#[daemon_module(
    id = "prism.debug",
    state = Arc::new(DebugManager::new()),
    commands(launch, set_breakpoints, r#continue, /* … */),
)]
pub struct DebugModule;
```

This is the logical complement to `#[daemon_command]` — the two
macros together make a module fully declarative with zero hand-written
`install()` boilerplate.

### Status

- ✅ Macro shipped in `prism-luau-derive` as `#[daemon_module]`. Three
  state forms (stateless, `slot`/`default`, `state` expression),
  required `id = "..."` and `commands(…)` lists. Each command ident
  in the list is desugared to a `register_<ident>` call against the
  builder's registry, threading an `Arc::clone(&__state)` for the
  stateful forms. Raw idents (`r#continue`) survive the
  `format_ident!` round-trip so keyword-named commands work.
- ✅ Migrations: every default-feature module has been collapsed to a
  single struct definition — `crypto` (stateless), `build` (stateless,
  1 cmd), `luau` (stateless, 1 cmd), `crdt`/`watcher`/`vfs`/`actors`
  (slot-backed), `debug` (direct-state). 9 modules, 36 commands, all
  hand-written `install()` bodies deleted. Admin stays hand-written
  intentionally — it captures `builder.module_ids` into its
  `AdminState`, which is fine but doesn't fit the macro's single-state
  shape. 108 lib + 12 integration + 12 stdio + 2 ipc + 6 macro tests
  pass; clippy clean.
- 🟡 Feature-gated `whisper` / `conferencing` modules are unmigrated
  for the same reason as #3 — they need their own opt-in build to
  exercise.

---

## 7. Luau `.d.luau` stub generation from `#[daemon_command]`  ✅ shipped (per-command + per-module constants)

### Current state

The macro now emits Luau function-signature stubs alongside the
existing register glue. Two pieces:

- `#[daemon_command(id = "x.y")]` emits a
  `pub const <FN_NAME_UPPER>_LUAU_STUB: &'static str` carrying the
  signature in the form `"x.y": (Req) -> Resp`. Rust types pass
  through `rust_type_to_luau` (the same mapper `#[luau_expose]`
  uses) so primitives collapse to `number` / `boolean` / `string`,
  collections become typed Luau tables, and named structs/enums
  remain as leaf identifiers expected to be `#[luau_expose]`-annotated
  elsewhere. Pass `luau = false` to opt out for commands whose
  request/response types don't have a meaningful Luau projection.
- `#[daemon_module(commands(a, b, c))]` emits a
  `pub const <Module>::LUAU_STUBS: &'static [&'static str]` slice
  pointing at each command's stub constant — sweepable by a future
  CLI codegen step into a per-module `.d.luau` file alongside the
  existing `prism codegen luau-types` pipeline.

### Status

- ✅ Macro-side shipped. The CLI sweep that aggregates `LUAU_STUBS`
  slices across registered modules and writes them next to
  `<workspace>/types/core.d.luau` is the natural follow-up; tracked
  alongside the rest of phase 4 in `docs/dev/luau-integration-plan.md`.

---

## 8. Typed `Props` accessor for blocks  ✅ shipped (extractor + starter migration + derive sugar)

### Current state

Every `render_slint` / `render_html` impl in `starter.rs` opens with
a cluster of manual prop extractions — 77 total calls to
`prop_str(props, "key", default)` / `prop_bool(...)` /
`prop_f64(...)`. This is stringly-typed: wrong key = silent wrong
default at runtime, not a compile error.

### Design

`#[derive(PrismBlock)]` gains an optional `#[block(props = "MyProps")]`
attribute pointing at a `#[derive(PrismField)]`-annotated struct. The
derive generates a typed `MyProps::from_value(&Value) -> MyProps`
extractor and passes a `&MyProps` instead of raw `&Value` into the
render closures or template fn.

```rust
#[derive(PrismField)]
struct TextProps {
    #[field(label = "Body", default = "", multiline)]
    body: String,
    #[field(label = "Level", select = ["paragraph","h1","h2","h3"], default = "paragraph")]
    level: String,
    #[field(label = "Link", default = "")]
    href: String,
}

#[derive(PrismBlock)]
#[block(id = "text", props = "TextProps")]
struct TextBlock;

impl TextBlock {
    fn template(props: &TextProps, children: &[Node]) -> TemplateNode { … }
}
```

`PrismField` + `PrismBlock` compose: the schema method is derived
from the struct fields, and the render template receives a fully
typed value instead of raw JSON.

### Status

- ✅ `#[derive(PrismField)]` extended with two new associated methods
  (`prism-luau-derive/src/prism_field.rs`):
  - `defaults() -> Self` — every field set from its
    `#[field(default = …)]` literal, falling back to the Rust-type
    default. Type→default mapping covers `String` / `bool` / signed
    + unsigned ints / `f32` + `f64`.
  - `from_value(&serde_json::Value) -> Self` — reads each field from
    the JSON map with type-appropriate accessors (`as_str` /
    `as_bool` / `as_i64` / `as_f64`); missing keys or wrong types
    fall back to the field's default. Raw idents (`r#type`) are
    stripped of the `r#` prefix when resolving JSON keys, so the
    wire shape matches what the property panel emits. Five new
    integration tests in `prism-builder/tests/derive_macros.rs`
    cover defaults, missing-key fallback, wrong-type fallback, and
    raw-ident handling.
- ✅ `schemas.rs` migrated: 14 prop structs are now `pub(crate)` with
  `pub(crate)` fields so `starter.rs` can import them and read fields
  directly. The dropped allows-stay (`#![allow(dead_code)]` is gone in
  spirit since the structs are instantiated for real now via
  `<Props>::from_value`).
- ✅ `starter.rs` migration: ~70 `prop_str` / `prop_bool` / `prop_f64`
  call sites replaced across 13 blocks (Text, Image, Container, Form,
  Input, Button, Code, Spacer, Columns, List, Table, Tabs, Accordion).
  Each `render_slint` and `render_html` opens with a single
  `let p = <Props>::from_value(props);` and reads typed fields
  thereafter. Unused `prop_bool` import dropped.
- ✅ Schema defaults reconciled with starter runtime fallbacks:
  `ContainerProps.border_color` gained `default = "#3b4252"` and
  `ButtonProps.text` gained `default = "Submit"` so the property
  panel and the render path agree on what unset means. Both were
  silent divergences pre-migration — the property panel offered an
  empty string while the renderer fell back to a colored value.
- ✅ Auxiliary props promoted in a follow-up pass: `ImageProps` gained
  `border_radius`; `CodeProps` gained `bg` + `color` (both `kind = "color"`,
  empty-as-sentinel preserved so the slint path still defers to the
  style cascade and the html path still falls back to the dark theme);
  `ListProps` gained `item_spacing`; `AccordionProps` gained
  `border_width` + `border_color` (`kind = "color"`) + `section_gap`.
  `GraphViewBlock`'s six-field hand-rolled schema collapsed into a
  `GraphViewProps` struct with the same shape. The `prop_str` /
  `prop_bool` / `prop_f64` / `prop_u64` imports in `starter.rs` are
  gone — every block in the catalog now reads typed fields off the
  prop struct.
- ✅ Derive sugar: `#[block(props = "MyProps")]` on
  `#[derive(PrismBlock)]` now wires the typed extractor through the
  template path. The derive routes `schema()` to
  `MyProps::field_specs()` and rewrites the generated
  `render_slint`/`render_html` to call
  `MyProps::from_value(props)` once and pass `&MyProps` into
  `template(&MyProps, &[Node])`. Block authors who already use the
  template shape (the `CoreWidgetBlock` path, `DemoCardBlock` in
  the derive integration tests, and the new `TypedCardBlock` test)
  drop the hand-written `schema()` method entirely. Two new
  integration tests in `prism-builder/tests/derive_macros.rs`
  exercise the typed-extraction path end-to-end (schema derivation
  + branching template based on a typed-prop field). The 16 starter
  blocks in `starter.rs` keep their hand-written `Component` impls
  for now — they aren't on the `template()` shape yet because the
  blocks need Slint-specific chrome (font-family / `Tabs` interactive
  state / `GraphView` layout fanout) the IR can't yet express. When
  those migrate, they'll opt into `props = "..."` automatically.

---

## 9. Widget contributions aggregator  ✅ shipped

### Current state

Every domain engine (`ledger`, `goals`, `calendar`, `spreadsheet`,
`habits`, `projects`, `fitness`, `crm`, `timekeeping`) and
interaction module (`comments`) exports a free
`pub fn widget_contributions() -> Vec<WidgetContribution>` with no
standard annotation. The call-site that aggregates them is a
hand-maintained list that must be updated whenever a new engine is
added.

### Design

A `widget_providers![ ledger, goals, calendar, … ]` declarative macro
at the aggregation site emits a single
`all_widget_contributions() -> Vec<WidgetContribution>` that
concatenates results from each listed module. Keeping the list
explicit (rather than an invisible `#[widget_provider]` attribute)
makes the dependency graph auditable and avoids surprising inclusion
of optional engines.

### Status

- ✅ Shipped. `widget_providers!` declarative macro lives in
  `prism-builder/src/core_widget.rs` next to `collect_all_contributions`;
  takes a comma-separated list of `widget_contributions()` call
  expressions and concatenates their results into a single
  `Vec<WidgetContribution>`. The 14 hand-rolled `all.extend(...)` calls
  in `collect_all_contributions` collapsed to one macro invocation
  with the engines listed inline. The list stays explicit (rather than
  an attribute scrape) so the dependency graph is auditable — adding
  a new engine is a one-line edit. 359 builder tests pass; clippy clean.

---

## 10. `#[derive(LuauType)]` for design-token structs  ✅ shipped (via existing `#[luau_expose]` + `luau_types!` macro)

### Current state

`DesignTokens`, `Colors`, `Typography`, `Spacing`, `Rgba`, and
`Radius` each hand-write `LUAU_TYPE_NAME` and `LUAU_TYPE_DEF` string
constants in `prism-core/src/design_tokens.rs`. This is the same
single-source-of-truth problem `#[derive(PrismField)]` solved for
field schemas — a struct field renamed in Rust requires a parallel
edit in the string constant, and divergence is silent until a Luau
type error surfaces at script runtime.

### Design

```rust
#[derive(LuauType)]
pub struct Colors {
    pub primary: Rgba,
    pub surface: Rgba,
    pub text: Rgba,
    // …
}
```

Generates `Colors::LUAU_TYPE_NAME` and `Colors::LUAU_TYPE_DEF` from
the field names and types, mapping Rust primitives to Luau types and
recursing into nested `#[derive(LuauType)]` structs by their
`LUAU_TYPE_NAME`. The registration table in `luau_types.rs` reduces
to a single macro invocation over the annotated types.

### Status

- ✅ Audit: the `LUAU_TYPE_NAME` / `LUAU_TYPE_DEF` per-struct constants
  the design contemplates are already emitted by the existing
  `#[luau_expose]` proc-attribute (`prism-luau-derive/src/lib.rs`).
  Every struct in `design_tokens.rs`, `shell_mode.rs`, the
  object-model leaves, and the config leaves is already annotated, so
  the single-source-of-truth goal is met for all the targets the
  design names. No new derive macro was needed — `#[luau_expose]`'s
  field walker (with its `Option<T>` / `Vec<T>` / `HashMap<K,V>`
  recursion through `rust_type_to_luau`) already covers every shape
  the design lists.
- ✅ Registration table collapsed: `crate::luau_types!(T1, T2, …)`
  in `prism-core/src/luau_types.rs` desugars to the
  `(T::LUAU_TYPE_NAME, T::LUAU_TYPE_DEF)` pair list. The hand-typed
  pair literals in `type_defs()` are gone — the 23 derive-driven
  types now read as a single comma-separated list, with the four
  hand-rolled stateful userdata types (the `luau_bindings_consts::*`
  free constants) appended via `extend_from_slice` since their
  type-stub constants aren't associated items the macro can reach.
  All 7 `luau_types::tests` still pass; clippy clean.

---

## 11. `#[derive(Editable)]` — shell stringly-typed field dispatch  🟡 derive shipped + extended; `apply_style_edit` + `apply_page_layout_edit` migrated

### Current state

`prism-shell/src/app/mutations.rs` contains 8 `apply_*` functions
(600+ lines total) that all do the same thing: dispatch on a `&str`
key, parse the `&str` value to the correct Rust type, clamp/validate,
and assign to a struct field. The pattern is forced by the Slint
callback boundary — field names arrive as strings from the property
inspector.

```rust
pub(super) fn apply_style_edit(style: &mut StyleProperties, key: &str, value: &str) {
    match key {
        "font_family" => style.font_family = Some(value.to_string()),
        "font_size"   => style.font_size   = value.parse().ok(),
        "font_weight" => style.font_weight = value.parse().ok(),
        "color"       => style.color       = Some(value.to_string()),
        // … 9 more arms
        _ => {}
    }
}
```

Every arm is mechanical: parse with the right coercion (`to_string`,
`.parse::<f32>()`, `parse_enum`, `clamp`), write the field. A
renamed field silently stops applying without a compile error.

### Design

```rust
#[derive(Editable)]
struct StyleProperties {
    #[edit]
    font_family: Option<String>,
    #[edit(parse = "f32", clamp(0.0, 200.0))]
    font_size: Option<f32>,
    #[edit(parse = "enum")]
    font_weight: Option<FontWeight>,
    // …
}

// Generates:
impl Editable for StyleProperties {
    fn apply_field(&mut self, key: &str, value: &str) {
        match key {
            "font_family" => self.font_family = Some(value.to_string()),
            "font_size"   => self.font_size   = value.parse::<f32>().ok()
                                 .map(|v| v.clamp(0.0, 200.0)),
            // …
            _ => {}
        }
    }
}
```

The 8 `apply_*` fns in `mutations.rs` become 8 one-liners:
`T::apply_field(entity, key, value)`.

### Status

- ✅ Derive shipped in `prism-luau-derive` as `#[derive(Editable)]`
  (`src/editable.rs`). Emits an inherent
  `apply_field(&mut self, key: &str, value: &str)` method whose body
  is a `match key` table over the struct's named fields. Type → parser
  mapping is driven by the field's Rust type: `Option<String>` clears
  on empty input and writes `Some(value.to_string())` otherwise;
  `Option<T: numeric>` parses through `value.parse::<T>().ok()`;
  `Option<bool>` writes `Some(value == "true")`; required `String` /
  `bool` / numeric fields parse with the same shape but fall through
  silently on parse failure rather than zeroing. Per-field attributes:
  `#[edit(skip)]`, `#[edit(rename = "key")]`, and
  `#[edit(clamp(lo, hi))]` for bounded numerics.
- ✅ Migration: `StyleProperties` (the 10-field cascade struct in
  `prism-builder/src/style.rs`) now derives `Editable` and
  `prism-shell::app::mutations::apply_style_edit` collapses to a
  one-line `style.apply_field(key, value)` thunk. Behavioural tests
  cover the dispatch table, the empty-string-clears-`Option<String>`
  semantics, the parse-failure-clears-`Option<numeric>` semantics, and
  the unknown-key no-op. 12 builder tests (was 8) + workspace clippy
  clean.
- ✅ Derive extended with two new field attributes covering nested
  delegation and sibling fan-out:
  - `#[edit(nested)]` (and `#[edit(nested, prefix = "...")]`) —
    when the inbound key starts with `<field_name>.` (or the explicit
    prefix), strip it and delegate to `self.<field>.apply_field(rest,
    value)`. The nested type just needs an inherent `apply_field`
    method; typically that comes from the same derive but it can also
    be hand-written (e.g. the new `Edges<f32>::apply_field` in
    `prism-core::foundation::geometry`, keyed on `top` / `right` /
    `bottom` / `left`). Nested arms are emitted *before* the flat
    `match key` table.
  - `#[edit(also = "name")]` — replicate the parsed value into a
    sibling field of the same type. The value is parsed and clamped
    once and the same `__v` binding is written to both targets, so
    fan-out can never drift between fields. Stack multiple `also` on
    one field for N-way fan-out.
- ✅ Migration: `prism-builder::layout::PageLayout` now derives
  `Editable`; `apply_page_layout_edit` shrank from a 28-line match
  block to a 4-line wrapper that handles only the `page_size` enum
  (which carries a `Custom { width, height }` payload — outside the
  flat-struct derive's lane) before delegating to the derived
  `apply_field`. `margin_top` / `margin_right` / `margin_bottom` /
  `margin_left` route through `#[edit(nested, prefix = "margin_")]`
  to the new `Edges<f32>::apply_field`; `column_gap` writes both gap
  axes via `#[edit(also = "row_gap")]`. 5 new builder tests +
  2 new core tests covering the nested + fan-out + skip + parse-fail +
  unknown-key surface.
- 🟡 Remaining hand-rolled `apply_*` fns in `mutations.rs` —
  `apply_facet_edit`, `apply_layout_to_node`, `apply_node_layout_edit`,
  `apply_transform_to_node`. These aren't blocked on the derive
  surface anymore; they're blocked on the underlying *type shape*:
  - `apply_transform_to_node` writes `position[0]` / `position[1]` /
    `scale[0]` / `scale[1]` (i.e. it edits *array indices*, not named
    fields), converts `transform.rotation` from degrees to radians on
    the way in, and maps free-form strings (`"top-left"`, etc.) to
    `Anchor` variants. A clean derive migration would first refactor
    `Transform2D::position` / `scale` from `[f32; 2]` to a struct
    with `x` / `y` fields, then add a sibling `#[derive(EditableEnum)]`
    that honours `serde(rename_all = "kebab-case")` for `Anchor`.
    Both are larger refactors than the value extraction here.
  - `apply_layout_to_node` is a state machine that branches on
    `LayoutMode::{Flow, Absolute, Free, Relative}` *and* mutates the
    enum variant in place when `layout.display` changes. That kind
    of cross-arm transition can't be expressed as a flat field table
    — it'd need a `#[derive(EditableEnum)]` over tagged enums plus a
    way to reseat the variant from a key.
  - `apply_facet_edit` is the same shape one level worse: it routes
    by enum variant (`FacetKind::ObjectQuery { query }` etc.), by
    string-prefix dotted paths (`binding.<slot>`,
    `record.<idx>.<field>`, `variant_rule.<idx>.<name>`), and reseats
    `FacetDataSource` / `FacetTemplate` / `FacetOutput` variants
    based on `source_kind` / `template_type` / `output_type`
    selectors. It's better thought of as a small interpreter than a
    dispatch table.
  - `apply_node_layout_edit` / `apply_node_transform_edit` are tree
    walkers that find the target node by id and then call the two
    `apply_*_to_node` fns above; they'll fall out naturally once
    those migrate.

---

## 12. Transport invoke adapter  ✅ trait shipped, HTTP + gRPC migrated

### Current state

All three daemon transports repeat the same core logic: extract a
command name + JSON payload, call `kernel.invoke(cmd, payload)` on a
blocking pool, map `CommandError` variants to transport-specific
response codes.

```rust
// http_axum.rs
let result = spawn_blocking(move || kernel.invoke(&cmd, payload)).await??;
match result {
    Ok(v)  => (StatusCode::OK, Json(v)).into_response(),
    Err(CommandError::NotFound(_)) => StatusCode::NOT_FOUND.into_response(),
    Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
}

// grpc_tonic.rs — same logic, different response type
// ipc_local.rs  — same logic, different frame type
```

The `CommandError → status/code` mapping is re-written independently
in each transport.

### Design

A shared `CommandErrorMapper` trait with a blanket impl for each
response type:

```rust
pub trait CommandErrorMapper {
    fn map_not_found(msg: &str) -> Self;
    fn map_internal(msg: &str) -> Self;
    // …
}

impl CommandErrorMapper for axum::Response { … }
impl CommandErrorMapper for tonic::Status { … }
impl CommandErrorMapper for IpcResponse { … }

pub fn invoke_mapped<R: CommandErrorMapper>(
    kernel: &DaemonKernel,
    cmd: &str,
    payload: JsonValue,
) -> R { … }
```

Each transport's handler shrinks to a single `invoke_mapped` call
plus the transport-specific serialization.

### Status

- ✅ Shared `CommandErrorMapper` trait shipped in
  `prism-daemon/src/transport/mapper.rs`. One method per `CommandError`
  variant (`not_found` / `already_registered` / `handler_error` /
  `lock_poisoned` / `permission_denied`); the provided
  `from_command_error(command, err)` fans out by variant. Implementors
  receive `(command, message)` so the response envelope (HTTP JSON
  body, gRPC status message) can carry both.
- ✅ HTTP migration: `axum::response::Response` impls
  `CommandErrorMapper` in `transport::http_axum`. The legacy
  `command_error_to_response` thunks through `Response::from_command_error`
  so existing call sites (`invoke`, `admin_snapshot`) are unchanged.
  Adds the previously-missing `PermissionDenied → 403` arm.
- ✅ gRPC migration: `tonic::Status` impls `CommandErrorMapper` in
  `transport::grpc_tonic`. The hand-rolled match in
  `KernelGrpcService::invoke` collapses to
  `tonic::Status::from_command_error(&req.command, err)`. Adds the
  previously-missing `PermissionDenied → grpc PermissionDenied` arm.
  Wire-level test coverage (`invoke_unknown_command_maps_to_grpc_not_found`,
  `invoke_handler_error_maps_to_grpc_internal`) still passes — the
  message payloads now use `CommandError`'s `Display` impl directly
  (which the tests assert on substring, not exact match).
- 🟡 IPC unmigrated by design. `IpcResponse` carries no kind tag (the
  envelope is `{ ok: bool, error: Option<String> }`) and the request
  `id` must be threaded into every response, which a stateless
  trait impl can't see. The single-line `e.to_string()` mapping in
  `dispatch` is not duplication worth abstracting.

---

## 13. Relay `RelayResult<T>` response type  ✅ type shipped, primary error-path handlers migrated

### Current state

All 25+ route handlers in `prism-relay/src/routes/` follow the same
shape:

```rust
pub async fn issue_token(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<IssueTokenRequest>,
) -> impl IntoResponse {
    match state.capability_tokens().issue(input) {
        Ok(r)  => (StatusCode::OK,       Json(json!(r))).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}
```

The `Ok` branch is always `200 + json!(result)`. The `Err` branch is
always one of 3-4 status codes plus `json!({ "error": … })`. The
wrapping and error mapping is repeated in every handler.

### Design

A typed `RelayResult<T>` that implements `IntoResponse`:

```rust
pub struct RelayResult<T>(Result<T, RelayError>);

impl<T: Serialize> IntoResponse for RelayResult<T> {
    fn into_response(self) -> Response {
        match self.0 {
            Ok(v)  => (StatusCode::OK, Json(v)).into_response(),
            Err(e) => e.into_response(),
        }
    }
}

pub struct RelayError { status: StatusCode, message: String }
impl IntoResponse for RelayError { … }
```

Handlers reduce to:

```rust
pub async fn issue_token(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<IssueTokenRequest>,
) -> RelayResult<IssueTokenResponse> {
    state.capability_tokens().issue(input)
        .map_err(|e| RelayError::conflict(e))
        .into()
}
```

This mirrors what `register_typed` did for daemon commands — moves
the wire serialization boilerplate out of the handler body.

### Status

- ✅ Type shipped in `prism-relay/src/result.rs` as `RelayResult<T> =
  Result<T, RelayError>` plus a `RelayError` whose `IntoResponse` impl
  emits either `{ "error": "<msg>" }` (when constructed via
  `with_message` / the `*_msg` constructors) or a bare status code (when
  constructed via `new` / the no-arg constructors), preserving the
  legacy "raw `StatusCode` return" wire shape. `From<StatusCode>` keeps
  callers' `.map_err(|_| StatusCode::CONFLICT)?` pattern legal during
  incremental migration.
- ✅ Migrated handlers across every route module that had explicit
  `Result<Json, StatusCode>` / `match { Some => Ok, None => Err }` shapes:
  `portals` (`get_portal`, `export_portal`), `vaults` (4 handlers),
  `collections` (`get_snapshot`, `import_snapshot`), `auth_password` (5
  handlers), `autorest` (5 handlers), `escrow` (`claim`), `templates`
  (`get_template`), `trust` (`get_peer_trust`), `acme`
  (`acme_challenge_response`, `get_certificate`), `forms` (2 handlers).
  Bare `StatusCode`-returning handlers (e.g.
  `delete_collection`, `unban_peer`) and pure-200 handlers
  (`list_*`, `verify_token`) remain on `impl IntoResponse` — the type
  buys nothing for them. 26 lib + 8 integration tests pass; clippy
  clean under `-D warnings`.

---

## 14. Test fixture helpers  ⬜ not started

### Current state

Integration tests across three packages repeat the same setup
boilerplate:

```rust
// prism-daemon/tests/kernel_integration.rs (repeated 12 times)
let kernel = DaemonBuilder::new()
    .with_crdt().with_luau().with_vfs().build().unwrap();

// prism-relay/tests/routes.rs (repeated 8 times)
let state = Arc::new(FullRelayState::new(RelayConfig::dev_mode())
    .with_all_modules());
let app = build_full_router(state);

// prism-builder/tests/derive_macros.rs (repeated per test)
let mut reg = ComponentRegistry::new();
register_builtins(&mut reg).unwrap();
```

### Design

Each package gets a `tests/common/mod.rs` (the standard Rust pattern)
with typed fixture constructors:

```rust
// prism-daemon/tests/common/mod.rs
pub fn test_kernel() -> Arc<DaemonKernel> {
    DaemonBuilder::new().with_defaults().build().unwrap()
}

// prism-relay/tests/common/mod.rs
pub fn test_router() -> Router { … }

// prism-builder/tests/common/mod.rs
pub fn test_registry() -> ComponentRegistry { … }
```

No new abstraction — just moving existing setup into a shared location.
This is a pure cleanup, no design needed.

### Status

- ⬜ Not started. Smallest item on this list; can be done incrementally
  test-file by test-file. Good first-contribution task.

---

## 15. `SignalSpec` fluent builder  ✅ shipped

### Current state

`SignalSpec { name, payload_fields, triggers }` is constructed as a
struct literal in ~30 places across the 11 domain engine
`widget_contributions()` functions:

```rust
.signal(SignalSpec {
    name: "on_milestone_complete".to_string(),
    payload_fields: vec![
        SignalPayloadField { name: "milestone_id".to_string(), kind: SignalPayloadKind::String },
        SignalPayloadField { name: "progress".to_string(),     kind: SignalPayloadKind::Number },
    ],
    triggers: Triggers::default(),
})
```

`WidgetContribution`'s builder already has `.signal()`. The gap is
that `SignalSpec` itself has no builder, so every call site repeats
the nested struct literal verbatim.

### Design

A minimal fluent builder on `SignalSpec` — no macro needed:

```rust
impl SignalSpec {
    pub fn new(name: impl Into<String>) -> Self { … }
    pub fn payload(mut self, name: &str, kind: SignalPayloadKind) -> Self { … }
}

// Before: 6 lines. After:
.signal(SignalSpec::new("on_milestone_complete")
    .payload("milestone_id", SignalPayloadKind::String)
    .payload("progress",     SignalPayloadKind::Number))
```

The existing `SignalDef::new` / `.with_payload` pattern in
`prism-builder/src/signal.rs` is the model — `SignalSpec` (in
`prism-core`) just needs the same treatment.

### Status

- ✅ Shipped. `SignalSpec` (in `prism-core/src/widget/contribution.rs`)
  grew a chainable `.payload(FieldSpec)` plus typed shorthands
  `.payload_text` / `.payload_number` / `.payload_boolean` /
  `.payload_date` / `.payload_date_time`. Migrated every
  `with_payload(vec![...])` call site in `prism-core` —
  `widget::views`, `interaction::comments`, and the `focus_planner`,
  `calendar`, `goals`, `projects`, `habits`, `fitness`, `reminders`,
  `spreadsheet`, and `timekeeping` engines all now read as fluent
  chains. `with_payload` stays for callers passing a pre-built
  `Vec<FieldSpec>`. 1913 prism-core tests pass; workspace check clean.

---

---

## 16. `VfsBackend` / `build_module` stringly-typed errors  ⬜ not started

### Current state

The `VfsBackend` trait and all its implementations (`LocalVfsBackend`,
`InMemoryVfsBackend`, `S3VfsBackend`, `GcsVfsBackend`) use
`Result<T, String>` for every method signature
(`vfs_module.rs:71–85`). The `build_module.rs` internal helpers
(`run_step`, `emit_file`, `compile_luau`, lines 62–123) do the same.

This means:
- Callers can't pattern-match error kinds — only `display()` strings.
- The `?` operator widens any upstream error into a `String` via
  `.to_string()` / `.map_err(|e| e.to_string())`, discarding
  structured cause chains.
- Introducing a new error condition requires callers to string-match,
  which silently breaks when the message changes.

### Design

Two small `thiserror` enums:

```rust
// vfs_module.rs
#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("hash not found: {0}")]
    NotFound(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}
```

```rust
// build_module.rs
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("compile error: {0}")]
    Compile(String),
    #[error("{0}")]
    Other(String),
}
```

Both are crate-private (no pub re-export needed). The `VfsBackend`
trait becomes `fn put(&self, …) -> Result<(), VfsError>`. Both error
types implement `Display` so `CommandError` mapping via
`register_typed_with_permission`'s `E: Display` bound continues to
work unchanged.

### Status

- ⬜ Not started. `thiserror` is already in the workspace.
  `vfs_module.rs` changes are self-contained — the error type is
  visible only inside the module (backends call each other, not
  the outside world). `build_module.rs` changes are similarly
  internal. No cross-crate impact.

---

## 17. `EmptyArgs` duplication across modules  ✅ shipped

### Current state

`crypto_module.rs:157` and `vfs_module.rs:486` both define an
identical `struct EmptyArgs {}` for commands that take no request
payload. Neither module can see the other's definition, so both
re-declare the same zero-field struct.

```rust
// crypto_module.rs:157
struct EmptyArgs {}

// vfs_module.rs:486
struct EmptyArgs {}
```

Any future module adding a no-payload command will create a third
copy.

### Design

Two options:

**Option A (preferred):** Change the `#[daemon_command]` macro to
accept `()` as the request type directly, skipping deserialization
when the request is the unit type. The handler becomes
`fn my_cmd() -> Result<Resp, E>` with no args struct at all.

**Option B (minimal):** Add `pub(crate) struct NoArgs;` in
`src/typed_command.rs` (next to `CommandRegistryExt`) and have both
modules import it.

Option A is cleaner because it eliminates the struct entirely rather
than sharing it. The macro already has the `fn(req)` vs
`fn(&state, req)` arity detection needed to special-case `()`.

### Status

- ✅ Shipped via Option B — single `pub struct EmptyArgs` lives in
  `prism-daemon::typed_command` next to `CommandRegistryExt`, with
  `#[derive(Debug, Default, Deserialize)]` so it deserializes from
  an empty JSON object identically to the legacy per-module copies.
  Both `crypto_module` (`crypto.keypair`) and `vfs_module`
  (`vfs.list`, `vfs.stats`) import it from the shared location. The
  duplicated structs are gone. Option A (macro special-cases `()` as
  the request) was the documented preference but is more invasive —
  `()` doesn't `DeserializeOwned` from `{}`, so the macro would have
  to bypass `register_typed` and emit a separate dispatch path.
  Option B keeps `register_typed` as the single seam. 113 daemon lib
  tests pass; clippy clean.

---

## 18. `schemas.rs` per-struct `#[allow(dead_code)]`  ✅ shipped

### Current state

`prism-builder/src/schemas.rs` has 14 individual
`#[allow(dead_code)]` attributes, one per `#[derive(PrismField)]`
struct (lines 15, 34, 55, 72, 85, 110, 131, 148, 159, 170, 181,
194, 205, 218, …). Each struct is "dead" at the type level because
it is never instantiated — only its derived `::field_specs()` method
is called. The allow is correct but the repetition is noise.

### Design

Replace the 14 per-struct allows with a single module-level
suppression at the top of `schemas.rs`:

```rust
#![allow(dead_code)] // structs are used only via their derived ::field_specs()
```

This removes 13 lines and makes the intent explicit in one place.

### Status

- ✅ Shipped. Module-level `#![allow(dead_code)]` at the top of
  `prism-builder/src/schemas.rs` replaces the 14 per-struct allows.
  Comment on the suppression names the intent ("structs are used only
  via their derived ::field_specs()") so the noise is gone but the
  reason isn't.

---

## 19. `#[allow(clippy::module_inception)]` in prism-core  ⬜ not started

### Current state

Three modules in `prism-core` suppress the `module_inception` lint
(a module containing a submodule with the same name, making
`use foo::foo::Thing` necessary):

- `src/identity/manifest/mod.rs:16`
- `src/language/syntax/mod.rs:12`
- `src/kernel/plugin_bundles/flux_types.rs:14`

The lint fires because the inner `mod manifest` / `mod syntax` /
`mod flux_types` shadows the outer module name. The conventional fix
is to rename the inner submodule (e.g. `mod manifest_inner` or
`mod types`) and re-export the public surface from `mod.rs`.

### Design

For each of the three occurrences:
1. Rename the inner submodule to avoid the clash (e.g.
   `mod manifest` → `mod inner` or `mod impl_`).
2. Re-export all currently-public items from `mod.rs` so call sites
   are unchanged.
3. Remove the `#[allow]`.

No caller changes needed if the `pub use inner::*` pattern is used
in `mod.rs`.

### Status

- ⬜ Not started. Each fix is a 2-file rename + re-export. No design
  risk — purely cosmetic. Low priority; the suppression is harmless
  but adds noise to `cargo clippy` output when new members audit the
  project.

---

## 20. `ObjectSnapshot` large-variant boxing  ⬜ not started

### Current state

`prism-core/src/foundation/undo/types.rs:18` suppresses
`clippy::large_enum_variant` on `ObjectSnapshot`:

```rust
#[allow(clippy::large_enum_variant)]
pub enum ObjectSnapshot {
    Object {
        before: Option<GraphObject>,
        after: Option<GraphObject>,
    },
    Edge {
        before: Option<ObjectEdge>,
        after: Option<ObjectEdge>,
    },
}
```

The `Object` variant holds two `Option<GraphObject>` fields inline,
making it substantially larger than the `Edge` variant. Every
`ObjectSnapshot::Edge` allocation carries padding to fit the larger
variant, and `Vec<ObjectSnapshot>` in undo batches pays that cost
per entry.

### Design

Box the large fields:

```rust
pub enum ObjectSnapshot {
    Object {
        before: Option<Box<GraphObject>>,
        after: Option<Box<GraphObject>>,
    },
    Edge {
        before: Option<ObjectEdge>,
        after: Option<ObjectEdge>,
    },
}
```

Remove the `#[allow]`. Any construction/match sites need one
`Box::new(…)` / `*deref` each.

### Status

- ⬜ Not started. Affects `prism-core` only. Impact is proportional
  to how large `GraphObject` actually is — worth profiling undo-batch
  allocation before and after to confirm the saving is real.

---

## 21. `prism-shell/src/app/` decomposition  ⬜ not started

### Current state

The shell's application layer is concentrated in four files that
together exceed 10,400 lines:

| File | Lines |
|---|---|
| `src/app/mod.rs` | 2,736 |
| `src/app/callbacks.rs` | 2,460 |
| `src/app/sync.rs` | 2,449 |
| `src/panels/properties.rs` | 2,798 |

`mod.rs` owns `AppState`, its constructor, and Slint window setup.
`callbacks.rs` holds every `on_*` Slint callback registration.
`sync.rs` handles every state → Slint push. All three files reference
the same `AppState` fields, making them tightly coupled but not
cohesive — large swaths of each file belong to a single feature
(builder panel, canvas, navigation, timeline) but are interleaved
with unrelated code.

### Design

Decompose each file by panel/feature boundary:

```
src/app/
  mod.rs          ← AppState struct + constructor only (< 300 lines)
  window.rs       ← Slint window setup/teardown
  callbacks/
    builder.rs    ← builder-panel on_* registrations
    canvas.rs     ← canvas drag/select/resize on_* registrations
    navigation.rs ← nav + page-switcher on_* registrations
    timeline.rs   ← timeline on_* registrations
  sync/
    builder.rs    ← builder → Slint model pushes
    canvas.rs     ← canvas selection state pushes
    navigation.rs ← page/nav model pushes
    timeline.rs   ← timeline model pushes

src/panels/
  properties/
    mod.rs        ← router + shared helpers
    style.rs      ← style cascade panel (< 400 lines)
    layout.rs     ← layout/position panel
    signals.rs    ← signal/connection panel
    variants.rs   ← variant axis panel
```

No logic moves — only file boundaries. Each module stays `pub(super)`
to the `app` module so the public API is unchanged.

This is a pure structural cleanup with no design risk. The `#[derive(SlintBinding)]`
migration (item #5 above) becomes much easier once `sync.rs` is split
into per-feature modules.

### Status

- ✅ Done!

---

## Sequencing

Implementation order optimises for value × independence:

1. **#1 Phase 1** (collapse `Component` + `HtmlBlock`): biggest LOC
   reduction, mechanical, no new crate dependencies.
2. **#3 daemon_command**: ✅ macro shipped and every default-feature
   module migrated. Only feature-gated `whisper` / `conferencing`
   still use `register_typed` directly — same wire behaviour either
   way.
3. **#2 PrismField**: ✅ derive shipped, every default schema
   migrated, rich kinds covered via the `kind = "..."` attribute
   escape-hatch.
4. **#1 Phase 2** (PrismBlock derive): ✅ shipped — depended on a
   richer `TemplateNode` (now grew `Image`/`Link`/`Children` arms)
   and on the walkers being callable from derived impls.
5. **#15 SignalSpec builder**: no dependencies, tiny — land any time.
6. **#14 test fixtures**: no dependencies, pure cleanup — land any time.
7. **#13 RelayResult**: ✅ shipped — `RelayResult<T>` + `RelayError`
   live in `prism-relay/src/result.rs`; every handler with an explicit
   error-path mapping migrated.
8. **#6 daemon_module**: ✅ shipped — every default-feature module
   collapsed onto `#[daemon_module(id, slot/state, commands(…))]`.
   Hand-written `install()` bodies deleted across 9 modules.
9. **#12 transport adapter**: ✅ shipped — `CommandErrorMapper` trait in
   `transport::mapper`; HTTP + gRPC migrated. IPC stays hand-rolled
   (no kind tag, request id must thread through).
10. **#11 Editable derive**: ✅ shipped — `#[derive(Editable)]` in
    `prism-luau-derive`; `StyleProperties` migrated. Other `apply_*`
    fns in `mutations.rs` stay hand-rolled (nested enum / multi-field
    fan-out cases the flat-struct derive doesn't fit).
11. **#8 typed Props**: ✅ shipped — `#[derive(PrismField)]` grew
    `defaults()` + `from_value(&Value) -> Self`; every block in
    `starter.rs` now extracts `let p = <Props>::from_value(props);`
    once and reads typed fields. Two schema defaults
    (`ContainerProps.border_color`, `ButtonProps.text`) reconciled
    with starter runtime fallbacks. Auxiliary props (`bg`, `color`,
    `border_radius`, `item_spacing`, etc.) and the GraphView's
    hand-rolled six-field schema were promoted into typed structs in
    a second pass — the `prop_str` / `prop_bool` / `prop_f64` /
    `prop_u64` imports in `starter.rs` are gone.
12. **#10 LuauType**: ✅ shipped — the per-struct constants are already
    emitted by the pre-existing `#[luau_expose]` macro across
    `design_tokens.rs` and the other leaf crates. The registration
    table in `luau_types.rs` collapsed to a single
    `crate::luau_types![…]` invocation; the four hand-rolled userdata
    types are appended explicitly because their stub constants are
    free `pub const`s, not associated items.
13. **#7 Luau stubs**: additive to #3, priority rises with
    luau-integration phase 4+.
14. **#5 SlintBinding**: ✅ derive shipped (now supports `push_only` /
    `pull_only` direction flags + per-field `skip` / `rename`); first
    migration `ChromeBindings` in `prism-shell/src/app/sync.rs`
    collapses the four shell-chrome `set_*` calls into one
    `bind_to(window)`. Wider migration of the remaining ~100
    `set_*` sites is deferred until #21 splits `sync.rs`.
15. **#4 visual_node**: lower priority — visual scripting is still
    evolving rapidly.
16. **#9 widget aggregator**: ✅ shipped — `widget_providers!` macro
    in `core_widget.rs` collapses the 14 `all.extend(...)` calls in
    `collect_all_contributions` to a single declarative invocation.
17. **#18 schemas.rs dead_code consolidation**: ✅ shipped — one
    module-level `#![allow(dead_code)]` replaces 14 per-struct allows.
18. **#17 EmptyArgs deduplication**: ✅ shipped via Option B — shared
    `pub struct EmptyArgs` in `prism-daemon::typed_command`; the
    duplicated module-private copies in `crypto_module` and
    `vfs_module` are gone. Option A (macro special-case for `()`) was
    rejected — `()` doesn't deserialize from `{}` cleanly through
    `register_typed`, and the workaround would split the dispatch path.
19. **#16 VfsBackend / build_module typed errors**: self-contained in
    `prism-daemon`; `thiserror` already in workspace.
20. **#20 ObjectSnapshot boxing**: check `GraphObject` size first; only
    worth doing if the allocation saving is measurable.
21. **#19 module_inception cleanup**: cosmetic, 2-file change per
    occurrence — good first-contribution task.
22. **#21 app/ decomposition**: ✅ largest cleanup item; start with
    `properties.rs`, then `callbacks/` splits. Unblocks `#5 SlintBinding`
    migration.

The luau-integration plan's open phases (4.3–4.7, 6) are *consumers*
of these refactorings: declarative widget definition in Luau (Phase 6
of that plan) lands cleanly on top of #1 Phase 2.
