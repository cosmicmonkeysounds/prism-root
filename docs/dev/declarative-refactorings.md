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
- ⬜ Wire palette aggregation: Luau's `node_palette()` should
  concatenate built-in entries with a registry of derived
  `NODE_DEF` constants. Today the only consumer is the integration
  test in `prism-builder/tests/derive_macros.rs`.

---

## 5. `#[derive(SlintBinding)]` — Rust↔Slint property bridge  🟡 derive shipped, migration pending

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
  `#[derive(SlintBinding)]` with struct attribute
  `#[slint(global = "AppGlobal")]`. Emits two methods on the host
  struct:
  - `bind_to(&AppGlobal<'_>)` — calls `handle.set_<field>(self.field.clone().into())` for every field.
  - `pull_from(&mut self, &AppGlobal<'_>)` — calls `self.field = handle.get_<field>().into()` for every field.
  The `.into()` coercion punts type-mapping to Slint's generated
  `Set*` traits — Rust `String` flows through `slint::SharedString`,
  primitives flow as-is. Callbacks (`on_*`) are deliberately not in
  scope; the derive is for state shuttling only.
- ⬜ Survey: the hand-written `set_*` calls in
  `prism-shell/src/app/sync.rs` and `commands.rs` are the migration
  target. Many properties don't live in a `slint::Global<...>` but
  on the root `AppWindow` directly, which the derive supports if
  the user passes `AppWindow` for the `global` attribute (the
  `set_<field>` / `get_<field>` shape matches).
- ⬜ Migration: pick a small struct that already projects flatly
  into a Slint global — `panels::navigation::PageRow` or similar —
  and derive `SlintBinding` on its mirror struct. Defer wider
  migration until shell-state structure stabilises.

---

## 6. `#[daemon_module]` — auto-generate `install()`  ⬜ not started

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

A `#[daemon_module(id = "prism.crdt")]` annotation on the module
struct collects every `register_*` symbol visible in the same module
and emits the full `DaemonModule` impl automatically:

```rust
#[daemon_module(id = "prism.crdt")]
struct CrdtModule;
```

The macro detects state capture by inspecting the `register_*`
signatures (stateless vs `fn(&Registry, Arc<State>)`) and
synthesises the builder acquire calls. Modules that need custom
builder slots (e.g. `doc_manager_slot`) can annotate the relevant
field with `#[module_slot]`.

This is the logical complement to `#[daemon_command]` — the two
macros together make a module fully declarative with zero hand-written
boilerplate.

### Status

- ⬜ Not started. Blocked on nothing — `#[daemon_command]` is fully
  shipped and provides all the `register_*` hooks the module macro
  needs to collect.

---

## 7. Luau `.d.luau` stub generation from `#[daemon_command]`  ⬜ not started

### Current state

The status note in #3 records: "Manually maintain a `.d.luau` stub
(or skip and lose intellisense)." The macro already has request and
response types in scope at proc-macro time via the handler signature;
nothing prevents it from also emitting a Luau type declaration.

### Design

Extend `#[daemon_command]` with an optional `luau` flag (default on)
that emits a `const <FN_NAME_UPPER>_LUAU_STUB: &str` constant
containing the Luau function signature for the command. A companion
`collect_luau_stubs!(module)` macro (or a build-script step) sweeps
all `*_LUAU_STUB` constants from a module and writes them into a
`.d.luau` file alongside the daemon binary.

The `prism-luau-derive` crate already contains all the primitives
needed for Luau type emission (`SymbolEmmyDocEmitter`,
`signal_symbols`, `generate_signal_type_stubs`).

### Status

- ⬜ Not started. Low design risk — purely additive to the existing
  macro. Priority rises once the visual scripting Luau integration
  (luau-integration-plan phase 4+) makes daemon stubs high-traffic.

---

## 8. Typed `Props` accessor for blocks  ⬜ not started

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

- ⬜ Not started. Depends on `#[derive(PrismField)]` covering all
  field kinds (item #2 above). The 9 remaining hand-rolled schemas
  in `schemas.rs` need `File`/`Currency`/`Calculation` kind support
  before this is viable end-to-end.

---

## 9. Widget contributions aggregator  ⬜ not started

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

- ⬜ Not started. Lower priority — the hand-maintained list is short
  and changes infrequently. Straightforward `macro_rules!` once the
  aggregation site is identified.

---

## 10. `#[derive(LuauType)]` for design-token structs  ⬜ not started

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

- ⬜ Not started. The emit infrastructure (`SymbolEmmyDocEmitter`,
  `LUAU_TYPE_DEF` pattern) already exists in `prism-luau-derive`.
  This is mostly a new derive macro that reuses existing emit logic.

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
5. **#6 daemon_module**: natural next step after #3 — no new design
   work, just collecting what `#[daemon_command]` already emits.
6. **#8 typed Props**: depends on #2 PrismField completing for all
   field kinds (`File`/`Currency`/`Calculation`).
7. **#10 LuauType**: independent, reuses existing emit infrastructure.
8. **#7 Luau stubs**: additive to #3, priority rises with
   luau-integration phase 4+.
9. **#5 SlintBinding**: independent, lower priority.
10. **#4 visual_node**: lower priority — visual scripting is still
    evolving rapidly.
11. **#9 widget aggregator**: lowest priority, hand-maintained list
    changes infrequently.

The luau-integration plan's open phases (4.3–4.7, 6) are *consumers*
of these refactorings: declarative widget definition in Luau (Phase 6
of that plan) lands cleanly on top of #1 Phase 2.
