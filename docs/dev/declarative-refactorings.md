# Declarative Refactorings

> Catalogue of "we're defining the same thing twice (or more)" patterns
> in Prism, ranked by payoff. Each entry follows the `luau-integration-plan.md`
> shape: current state, design, status, work breakdown.

The unifying goal mirrors the Luau plan's first principle — **single
source of truth**: one Rust definition, every other surface (UI render
target, codegen, RPC wire, type stubs, intellisense) derived.

---

## 1. `Block` trait collapse + `#[derive(PrismBlock)]`  🟢 Phase 1 shipped

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
- ⬜ Phase 2: extend `TemplateNode` to cover advanced cases (asset
  paths, conditional structure, modal nesting), then ship the
  `#[derive(PrismBlock)]` proc-macro that takes a single
  `fn template(&self, props) -> TemplateNode` and emits both render
  impls.
- ⬜ Phase 3: deprecate `WidgetContribution` / `CoreWidgetComponent`
  in favor of the unified `Block`.

### Work breakdown

| Crate | Work |
|---|---|
| `prism-builder` | New `block.rs` with `Block` trait. Migrate the 16 builtins from `starter.rs` + `html_starter.rs` into one file each (or one merged catalog file). Update `ComponentRegistry` + `HtmlRegistry` to take `Arc<dyn Block>`. |
| `prism-relay` | Confirm `render_slint` is feature-gated; no Slint deps leak into relay's graph. |
| `prism-shell` | None — the registries' public API stays the same. |
| `prism-builder` tests | All 367+ existing tests must pass; add round-trip tests that the same `Block` produces matching schema across both render paths. |

---

## 2. `#[derive(PrismField)]` — unify the four field schemas  🟡 derive shipped, migrations pending

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
- 🟡 Migration: `prism-builder::schemas::text` migrated to
  `#[derive(PrismField)]` on a private `TextProps` struct as the
  first proof. The remaining 9 schemas in `schemas.rs` continue to
  use hand-rolled builders pending derive coverage of `File` /
  `Currency` / `Calculation` kinds (which take config payloads
  that don't map cleanly to a Rust field type alone).

---

## 3. `#[daemon_command]` proc-macro  🟡 typed-helper shipped, macro pending

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
- ⬜ Migrate the rest of the modules (`crypto`, `vfs`, `actors`,
  `watcher`, `admin`, `crdt`, `luau`, `debug`). `crypto` and `vfs`
  need typed response structs first — they currently build inline
  `json!()` literals.
- ⬜ Pick an attribute macro shape (function attr vs derive on a unit
  struct) once enough modules are typed to see the patterns clearly.
- ⬜ Add to `prism-luau-derive` or a new `prism-daemon-derive` crate
  that desugars to the same `register_typed` glue this layer provides.

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

## Sequencing

Implementation order optimises for value × independence:

1. **#1 Phase 1** (collapse `Component` + `HtmlBlock`): biggest LOC
   reduction, mechanical, no new crate dependencies.
2. **#3 daemon_command**: independent of #1, large surface area, very
   mechanical once the macro shape is fixed.
3. **#2 PrismField**: enables #1 Phase 2 (the derive needs a unified
   field shape).
4. **#1 Phase 2** (PrismBlock derive): depends on #2 and a richer
   `TemplateNode`.
5. **#5 SlintBinding**: independent, lower priority.
6. **#4 visual_node**: lowest priority — visual scripting is still
   evolving rapidly.

The luau-integration plan's open phases (4.3–4.7, 6) are *consumers*
of these refactorings: declarative widget definition in Luau (Phase 6
of that plan) lands cleanly on top of #1 Phase 2.
