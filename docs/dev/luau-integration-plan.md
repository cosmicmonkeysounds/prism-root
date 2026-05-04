# Luau Deep Integration Plan

> Goal: Luau scripts can access, read, edit, create, and generate anything in
> Prism. Eliminate duplicate struct definitions by deriving Luau type stubs and
> runtime bindings from the same Rust source that defines the types.

---

## Current State

Phases 1 + 2a + 4.1 + 4.2 + 5a have shipped. The fundamental gap is
narrowed but not closed: scripts can read tokens / config / the
entity-type registry, mutate the document via `Custom` signal
handlers, and resolve facet data via `FacetKind::Script`. They still
can't subscribe to atoms, mutate the object graph, touch the VFS,
fire signals, or run as long-lived watchers.

| Layer | What exists | Gap |
|-------|------------|-----|
| Daemon (`luau_module.rs`) | `luau.exec` — fire-and-forget script with JSON args/return + `prism` global preinstalled | No reactive subscriptions, no `luau.eval` REPL, no persistent script lifecycle, no `luau.register_widget` / `luau.register_automation` |
| Daemon (`prism_context.rs`) | `PrismContext { tokens, shell_mode, permission, objects, config }` | No `document` / `app` / `selection` / `vfs` / `signals` / `commands` / `crypto` / `automation` surfaces |
| Macro (`prism-luau-derive`) | `#[luau_expose]` for named-field structs, unit-only enums, **and tagged-union enums** (Phase 1.1 — table-shaped `{ tag = "Variant", … }` round-trip); `manual_impl` / `mutable` / `read_only` / `rename` / `luau_skip` opts | `#[luau_expose]` on free functions |
| Core (`luau_types.rs` + `luau_bindings*.rs`) | Hand-rolled `GraphObject` / `ObjectEdge` / `ObjectsHandle` / `ConfigHandle` UserData; codegen registry collects every `LUAU_TYPE_DEF` const | `BuilderDocument`, `Node`, `PrismApp`, `Page`, `LayoutMode`, `StyleProperties`, `Connection`, signal payloads, timeline / Flux types |
| Core (`language/luau/`) | Parser (full-moon), syntax provider, visual language, signal-aware completions | Read-only intelligence — no mutation path; signal stubs from `prism_builder::signal::generate_signal_type_stubs` are not yet emitted by `prism codegen luau-types` |
| Builder (`signal.rs`) | `ActionKind::Custom { handler }` round-trips through `DispatchResult::Custom` | — (now executed; see Shell row) |
| Builder (`facet/mod.rs`) | `FacetKind::Script { source, language, graph }` data type; `ScriptLanguage::{Luau, VisualGraph}` | — (now executed; see Shell row) |
| Shell (`app/mod.rs::fire_signal`) | `Custom` connection handler runs through `prism_daemon::modules::luau_module::exec`; return values with `set_properties` / `navigate` keys are applied back to the document | Handler scripts still see only the default `PrismContext` — no document reference, no per-event lifecycle |
| Shell (`app/sync.rs`) | `FacetKind::Script` resolves through `luau_module::exec` per facet evaluation | No incremental re-eval on dependency change; one-shot per sync pass |
| CLI (`codegen.rs`) | `prism codegen luau-types` emits `<workspace>/types/core.d.luau`, **`builder.d.luau`** (Phase 5 fan-out — `StyleProperties`, `Dimension`, `GridPlacement`, layout enums), and **`signals.d.luau`** (per-component signal payloads from the built-in `ComponentRegistry` via `generate_signal_type_stubs`) | Per-workspace component contributions still aren't walked — the registry instantiation in `render_signals_stub` only seeds built-ins. |

---

## Design Principles

1. **Single source of truth** — Rust structs define the shape. Luau sees
   derived type stubs and runtime bindings. No hand-maintained parallel
   definitions.

2. **Declarative exposure** — A `#[luau_expose]` attribute macro on a Rust
   struct/enum/fn generates: (a) the `mlua` `UserData` / `FromLua` impls,
   (b) a `.d.luau` type stub for LuaLS intellisense, (c) a help entry for
   the Prism help registry.

3. **Object-safe, capability-scoped** — Scripts receive a `PrismContext`
   userdata that grants access proportional to their trust level
   (`ShellMode` × `Permission`). Admin scripts see everything; user-mode
   scripts see their own workspace data.

4. **Reactive** — Scripts can subscribe to atoms, CRDT changes, and signals.
   The runtime manages subscription lifetimes tied to the script's lifecycle.

5. **Bidirectional** — Rust can call Luau (event handlers, automations,
   facet resolvers) and Luau can call Rust (mutations, queries, commands).

---

## Phase 1: Derive Macro — `#[luau_expose]`  ✅ shipped (incl. Phase 1.1 tagged unions)

### Crate: `prism-luau-derive`

A proc-macro crate that generates mlua bindings + type stubs from Rust types.
Ships today: named-field structs (read-only by default, `mutable` opt-in),
unit-only enums (round-trip as Luau strings via `IntoLua` / `FromLua`), and
**tagged-union enums** (Phase 1.1 — `{ tag = "Variant", value = … }` /
`{ tag = "Variant", field1 = …, field2 = … }` table round-trip; the
discriminator is always the literal key `tag`, deliberately not driven
by `#[serde(tag = "...")]` so scripts see one shape regardless of how
the type serialises elsewhere). The `manual_impl` escape hatch
suppresses the auto-generated `UserData` / `IntoLua` / `FromLua` impls
so stateful subsystems (`ObjectRegistry`, `ConfigModel`, `GraphObject`,
`ObjectEdge`) hand-write a method API and still surface their type
stub through `LUAU_TYPE_NAME` / `LUAU_TYPE_DEF` constants.

```rust
// In prism-core/src/design_tokens.rs
#[luau_expose]
pub struct DesignTokens {
    pub colors: Colors,
    pub spacing: Spacing,
    pub radius: Radius,
    pub typography: Typography,
}
```

**Generates:**

```rust
// mlua UserData impl (compile-time)
impl mlua::UserData for DesignTokens {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("colors", |_, this| Ok(this.colors.clone()));
        fields.add_field_method_get("spacing", |_, this| Ok(this.spacing.clone()));
        // ...
    }
}
```

```luau
-- .d.luau type stub (emitted at build time)
export type DesignTokens = {
    colors: Colors,
    spacing: Spacing,
    radius: Radius,
    typography: Typography,
}
```

### Attribute options

```rust
#[luau_expose(
    read_only,           // only getters, no setters
    rename = "Tokens",   // Luau-side name differs from Rust
)]
pub struct DesignTokens { ... }

#[luau_expose(mutable)]  // generates setters too
pub struct Node { ... }

#[luau_expose]
pub enum ShellMode {
    Use,
    Build,
    Admin,
}
// → generates string union type: type ShellMode = "Use" | "Build" | "Admin"

#[luau_expose]
pub fn resolve_cascade(app: &StyleProperties, page: &StyleProperties, node: &StyleProperties) -> StyleProperties { ... }
// → generates a free function binding on the Prism global
```

### Enum handling

```rust
#[luau_expose]
pub enum LayoutMode {
    Flow(FlowProps),
    Free,
    Absolute(AbsoluteProps),
    Relative(FlowProps),
}
```

Becomes tagged-union userdata:
```luau
export type LayoutMode =
    { tag: "Flow", value: FlowProps }
  | { tag: "Free" }
  | { tag: "Absolute", value: AbsoluteProps }
  | { tag: "Relative", value: FlowProps }
```

---

## Phase 2: PrismContext — The God Object  🟡 partial

A single userdata injected as a global into every Luau execution context.
Provides namespaced access to every Prism subsystem.

**Shipped (Phase 2a)**: `prism.tokens`, `prism.shell_mode`,
`prism.permission`, `prism.objects` (ObjectRegistry read API),
`prism.config` (ConfigModel get/set/reset/is_overridden). Injection
plumbing in `prism-daemon/src/modules/prism_context.rs` is the single
shared install point for every entry path (`luau.exec`, the shell's
`Custom` handler dispatch, facet `Script` resolution).

**Open**: `prism.document`, `prism.app`, `prism.selection`,
`prism.objects` write API (create/update/delete/query — read API ships;
mutations require a `CrdtSync` reference threaded through), `prism.edges`,
`prism.vfs`, `prism.signals`, `prism.commands`, `prism.crypto`,
`prism.automation`, `prism.atoms`, `prism.store`, `prism.crdt` (the
last three are Phase 3).

```luau
-- Available as `prism` global in every script
local doc = prism.document          -- active BuilderDocument
local app = prism.app               -- active PrismApp
local tokens = prism.tokens         -- DesignTokens (read-only)
local mode = prism.shell_mode       -- ShellMode
local selection = prism.selection   -- SelectionModel

-- Object tree (Loro CRDT-backed)
local task = prism.objects:get("task-123")
local tasks = prism.objects:query({
    filters = {{ field = "status", op = "eq", value = "active" }},
    sorts = {{ field = "priority", dir = "desc" }},
    limit = 50,
})
prism.objects:create("task", { title = "New task", status = "active" })
prism.objects:update("task-123", { status = "done" })
prism.objects:delete("task-123")

-- Edges
prism.edges:create("parent_child", { source = "proj-1", target = "task-123" })
local children = prism.edges:query({ type = "parent_child", source = "proj-1" })

-- Document tree manipulation
local node = doc:find("node-abc")
node.props.text = "Hello"
doc:insert(parent_id, { component = "button", props = { label = "Click" } })
doc:remove("node-abc")
doc:move("node-abc", new_parent_id, index)

-- VFS
local bytes = prism.vfs:get("sha256-abc...")
prism.vfs:put(bytes, { mime = "image/png", filename = "logo.png" })

-- Signals
prism.signals:fire("node-abc", "clicked", { x = 100, y = 200 })
prism.signals:on("node-abc", "changed", function(payload)
    -- reactive handler
end)

-- Commands
prism.commands:execute("undo")
prism.commands:execute("navigate", { page_id = "settings" })

-- Config
local font_size = prism.config:get("editor.fontSize")
prism.config:set("editor.fontSize", 16)

-- Crypto
local keypair = prism.crypto:keypair()
local encrypted = prism.crypto:encrypt(keypair.public, "secret")

-- Automation
prism.automation:trigger("on_task_complete", { task_id = "task-123" })
```

### Implementation

`PrismContext` wraps references to existing kernel subsystems:

```rust
pub struct PrismContext {
    // Daemon-side (via kernel.invoke or direct reference)
    doc_manager: Arc<DocManager>,
    vfs_manager: Arc<VfsManager>,
    crypto_module: Arc<CryptoModule>,
    actors_manager: Arc<ActorsManager>,

    // Core-side (direct reference when in-process)
    object_registry: Rc<ObjectRegistry>,
    crdt_sync: Option<Rc<CrdtSync>>,
    config_model: Rc<ConfigModel>,
    
    // Shell-side (when running in shell context)
    store: Option<Rc<Store<AppState>>>,
    signal_runtime: Option<Rc<SignalRuntime>>,
    command_registry: Option<Rc<CommandRegistry>>,
    
    // Capability scope
    mode: ShellModeContext,
}
```

---

### Sandbox installation paths

`PrismContext` is constructed at three different entry points, each
with progressively richer capabilities. The capability matrix is the
contract — a script that runs in all three environments must not
assume more than the daemon entry can give it.

| Entry point | Caller | Constructed in | Capabilities populated |
|-------------|--------|----------------|------------------------|
| `luau.exec` (daemon RPC) | Remote / IPC clients | `prism-daemon::modules::luau_module::exec` | `tokens`, `shell_mode`, `permission`, `objects` (read+write), `config`, `edges`, `vfs`, `crypto`. **No** `document` / `signals` / `selection` / `app` (the daemon has no live UI tree). |
| `Custom` signal handler | Shell `fire_signal` | `prism-shell::app::ShellInner::exec_custom_handlers` | All daemon-side capabilities **plus** `document` (live `BuilderDocument`), `signals` (re-entrant `fire`), `selection`, `app` (active `PrismApp`). |
| `FacetKind::Script` resolver | Shell `sync_builder_document` | `prism-shell::app::sync` facet branch | Same as Custom handler **but** `document` is read-only (a sync pass that mutates the document mid-resolution would loop). `signals.fire` is queued, not re-entrant. |

The shared install point in `prism-daemon::modules::prism_context::install`
remains the single function every entry calls. The new shape is:

```rust
pub struct PrismContext {
    // Daemon-resident (always populated)
    pub tokens: DesignTokens,
    pub shell_mode: ShellMode,
    pub permission: Permission,
    pub objects: ObjectsHandle,    // read+write once Phase 4 lands
    pub edges: EdgesHandle,        // new in Phase 4
    pub config: ConfigHandle,
    pub vfs: Option<VfsHandle>,    // Some(_) when daemon has VfsManager
    pub crypto: Option<CryptoHandle>,

    // Shell-resident (None when called from daemon RPC)
    pub document: Option<DocumentHandle>,
    pub signals: Option<SignalsHandle>,
    pub selection: Option<SelectionHandle>,
    pub app: Option<AppHandle>,

    // Lifecycle (Phase 3)
    pub atoms: Option<AtomsHandle>,
    pub store: Option<StoreHandle>,
}
```

Every shell-resident handle is `None` from the daemon's vantage point.
Scripts that touch them surface a typed Luau error
(`prism.document is not available in this context`) rather than silently
no-op. The `.d.luau` stubs mark these fields as `Document?` so LuaLS
flags missing nil-checks at edit time.

### Threading `CrdtSync` into the sandbox

The daemon owns `DocManager`, which wraps a `LoroDoc` per workspace.
`CrdtSync` (in `prism-core::kernel::crdt::sync`) is the
write-through bridge between in-memory `ObjectRegistry` /
`CollectionStore` state and the Loro CRDT. Today `luau_module::exec`
is a free function with no kernel handle; Phase 4 turns it into a
method on a new `LuauModule` struct that holds:

```rust
pub struct LuauModule {
    doc_manager: Arc<DocManager>,
    object_registry: Rc<RefCell<ObjectRegistry>>,
    config_model: Rc<ConfigModel>,
    crdt_sync: Rc<CrdtSync>,     // writes mirror into the active LoroDoc
    vfs: Option<Arc<VfsManager>>,
    crypto: Option<Arc<CryptoModule>>,
}
```

`LuauModule::exec(&self, source, args)` builds a `PrismContext` from
its fields and calls `prism_context::install`. Existing callers update:

- `prism-daemon::registry` registers `luau.exec` against
  `LuauModule::exec` instead of the free function.
- `prism-shell::app::ShellInner` keeps an `Rc<LuauModule>` (cheap
  clone of the daemon's instance) so shell-side entry points reuse the
  same handles. The shell augments the context with `document` /
  `signals` / `selection` / `app` before calling `install`.

Object/edge writes go through `CrdtSync::apply_object_change` /
`apply_edge_change` so a Luau mutation is indistinguishable from a
local UI mutation downstream of the CRDT — peers receive the same
delta and Loro history is preserved.

---

## Phase 3: Reactive Subscriptions  ⬜ not started

Scripts that live beyond a single execution (event handlers, watchers,
facet resolvers) can subscribe to reactive state:

```luau
-- Subscribe to atom changes
local unsub = prism.atoms:watch("task-123", function(object)
    print("Task changed:", object.data.title)
end)

-- Subscribe to store slices (selector pattern)
prism.store:select("selection", function(sel)
    print("Selected:", #sel.node_ids, "nodes")
end)

-- CRDT sync events
prism.crdt:on_sync(function(event)
    if event.type == "ObjectChanged" then
        -- remote peer mutated an object
    end
end)

-- Cleanup is automatic when the script's lifecycle ends
-- or manual via unsub()
```

### Async model — coroutines

Long-running operations (CRDT round-trips, VFS reads, AI calls,
inter-actor messaging) suspend the calling Luau thread via
`coroutine.yield`. The runtime resumes the coroutine when the
underlying future settles. This aligns with Luau's native idiom and
keeps subscription callbacks single-threaded.

```luau
-- prism.objects:fetch is non-blocking — yields under the hood.
local task = prism.objects:fetch("task-123")
print(task.data.title)

-- Equivalent explicit form for clarity:
local co = coroutine.running()
prism.objects:fetch_async("task-123", function(task)
    coroutine.resume(co, task)
end)
local task = coroutine.yield()
```

Implementation: every async method on a handle is a thin wrapper that
captures the current coroutine, fires the underlying op, and resumes
on completion. Subscription callbacks (`prism.atoms:watch`, etc.)
are *not* coroutines — they execute on the main Luau thread in the
order events fire to keep observation deterministic. A subscription
callback that needs to do async work spawns a fresh coroutine via
`prism.spawn(fn)`.

### Lifecycle management

```rust
pub struct ScriptLifecycle {
    id: String,
    subscriptions: Vec<Box<dyn FnOnce()>>,  // unsub closures
    atoms: Vec<AtomSubscription>,
    store_subs: Vec<Subscription>,
}

impl Drop for ScriptLifecycle {
    fn drop(&mut self) {
        // All subscriptions torn down automatically
    }
}
```

---

## Phase 4: Script Locations — Where Luau Lives in Prism

Sub-status: 4.1 ✅ · 4.2 ✅ · 4.3 ⬜ · 4.4 ⬜ · 4.5 ⬜ · 4.6 ⬜ · 4.7 ⬜.

### 4.1 Signal handlers (Connection::Custom)  ✅ shipped

`ActionKind::Custom { handler }` round-trips through `DispatchResult::Custom`
in `prism-builder` and is executed in `prism-shell::app::fire_signal` via
`prism_daemon::modules::luau_module::exec`. Return values are interpreted:
`{ set_properties = { node_id = { key = value, ... } } }` mutates props,
`{ navigate = "page-id" }` switches pages. Handler bodies are authored in
the Signals panel (or the visual graph editor — `connections_to_event_listeners`
/ `event_listeners_to_connections` keep both views in sync).

```luau
-- Attached to a Connection with ActionKind::Custom
-- Receives the signal payload + document context
function on_button_click(event)
    local count = prism.document:find(event.source):prop("count") or 0
    prism.document:find(event.source):set_prop("count", count + 1)
end
```

### 4.2 Facet resolvers (FacetKind::Script)  ✅ shipped

`FacetKind::Script { source, language: ScriptLanguage::{Luau, VisualGraph}, graph }`
is data-modelled in `prism-builder/src/facet/mod.rs` and executed in
`prism-shell::app::sync` via `luau_module::exec(&effective_source, None)`
during facet resolution. The `VisualGraph` language compiles its
`ScriptGraph` to Luau source before exec via the `prism-core::language::visual`
bridge.

Open: incremental re-eval. Today the facet re-runs whenever
`sync_builder_document` runs; per-dependency invalidation needs Phase 3's
atom subscriptions.

```luau
-- Facet data resolution script
-- Must return an array of records
function resolve()
    local tasks = prism.objects:query({
        filters = {{ field = "type", op = "eq", value = "task" }},
        sorts = {{ field = "createdAt", dir = "desc" }},
    })
    return table.map(tasks, function(t)
        return { title = t.data.title, status = t.data.status }
    end)
end
```

### 4.3 Automation actions (AutomationEngine)  ⬜ not started

```luau
-- Triggered by automation rules
function on_task_status_change(ctx)
    if ctx.new_value == "done" then
        prism.objects:update(ctx.object_id, {
            completedAt = os.time(),
        })
        -- cascade: check if parent project is now complete
        local parent = prism.edges:query({
            type = "parent_child",
            target = ctx.object_id,
        })[1]
        if parent then
            check_project_completion(parent.source)
        end
    end
end
```

### 4.4 Computed fields (expression extension)  ⬜ not started

```luau
-- Field formula (runs in expression evaluator context)
-- Receives the current record as `self`
function compute_progress(self)
    local children = prism.edges:query({
        type = "parent_child",
        source = self.id,
    })
    local done = 0
    for _, edge in ipairs(children) do
        local child = prism.objects:get(edge.target)
        if child.data.status == "done" then
            done = done + 1
        end
    end
    return #children > 0 and (done / #children) or 0
end
```

### 4.5 CLI plugins / build steps  ⬜ not started

```luau
-- prism build step (registered via manifest)
function build_step(ctx)
    local pages = prism.app.pages
    for _, page in ipairs(pages) do
        local html = prism.render:html(page.document)
        prism.vfs:put(html, {
            mime = "text/html",
            filename = page.route .. ".html",
        })
    end
    return { success = true, outputs = #pages .. " pages rendered" }
end
```

### 4.6 Widget templates (WidgetTemplate scriptable nodes)  ⬜ not started

```luau
-- Dynamic widget rendering (replaces static TemplateNode trees)
function render_chart(props, data)
    local bars = {}
    for i, item in ipairs(data) do
        table.insert(bars, {
            component = "container",
            props = {
                height = item.value .. "%",
                background = prism.tokens.colors.accent,
            },
        })
    end
    return { component = "columns", children = bars }
end
```

### 4.7 Interactive shell / REPL  ⬜ not started

```luau
-- Available via command palette or a dedicated Luau console panel
> prism.objects:query({ limit = 5 })
-- [{ id: "task-1", ... }, ...]

> prism.document.root.children
-- [{ id: "node-abc", component: "container", ... }, ...]

> prism.app:add_page({ title = "Test", route = "/test" })
-- Page { id: "page-xyz", ... }
```

---

## Phase 5: Type Stub Generation Pipeline  🟡 partial

`prism codegen luau-types` exists today and emits three files into
`<workspace>/types/`:

1. `core.d.luau` from `prism_core::luau_types::type_defs()`.
2. `builder.d.luau` from `prism_builder::luau_types::type_defs()` —
   the leaf set today is `FlowDisplay`, `FlexDirection`, `AlignOption`,
   `JustifyOption`, `Dimension`, `GridPlacement`, `StyleProperties`.
3. `signals.d.luau` from
   `prism_builder::signal::generate_signal_type_stubs(&registry, "prism")`
   over a freshly-built `ComponentRegistry` seeded with `register_builtins`.

Each registry is one `Vec<(name, def)>` rather than an
`inventory!`-style link-time collection because `cdylib` + WASM
targets don't reliably surface distributed slices.

**Open**: deeper `prism-builder` annotation. The leaf set above is a
starter; `BuilderDocument`, `Node`, `PrismApp`, `Page`, `LayoutMode`,
`Connection`, `SignalDef`, `ResourceDef`, `PrefabDef`, `FacetDef`
still need `#[luau_expose]` (most are tagged unions, now unblocked
by Phase 1.1). Per-workspace component contributions also aren't
walked yet — the registry instantiation in
`render_signals_stub` only seeds built-ins.

At build time (or as a `prism codegen luau-types` command):

1. Walk all `#[luau_expose]`-annotated types across the workspace.
2. Emit a consolidated `.d.luau` file tree:

```
types/
├── prism.d.luau          -- PrismContext global
├── objects.d.luau        -- GraphObject, ObjectEdge, etc.
├── document.d.luau       -- BuilderDocument, Node, etc.
├── app.d.luau            -- PrismApp, Page, etc.
├── tokens.d.luau         -- DesignTokens, Colors, etc.
├── signals.d.luau        -- per-component signal payloads (existing)
├── layout.d.luau         -- LayoutMode, FlowProps, etc.
├── style.d.luau          -- StyleProperties, resolve_cascade
├── flux.d.luau           -- FluxRegistry entity/edge types
├── timeline.d.luau       -- TimelineEngine, Track, Clip, etc.
├── config.d.luau         -- ConfigRegistry keys + value types
└── crypto.d.luau         -- keypair, encrypt, decrypt signatures
```

3. These stubs feed into:
   - LuaLS / Luau LSP for IDE completions
   - The `LuauSyntaxProvider` for in-editor intelligence
   - Runtime validation (optional strict mode)

---

## Phase 6: Declarative Widget Definition in Luau  ⬜ not started

The endgame: define components entirely in Luau, eliminating the need to
write Rust `impl Component` for domain-specific widgets.

```luau
-- widgets/kanban.luau
return prism.widget {
    id = "kanban",
    label = "Kanban Board",
    category = "DataTable",
    
    schema = {
        prism.field.select("status_field", {
            label = "Status Field",
            options = { "status", "stage", "phase" },
            default = "status",
        }),
        prism.field.text("title_field", {
            label = "Title Field",
            default = "title",
        }),
    },
    
    data_query = {
        filters = {{ field = "type", op = "eq", value = "task" }},
    },
    
    signals = {
        prism.signal("card_moved", {
            { name = "card_id", kind = "text" },
            { name = "from_column", kind = "text" },
            { name = "to_column", kind = "text" },
        }),
    },
    
    variants = {
        prism.axis("density", {
            { id = "compact", label = "Compact", overrides = { gap = 4 } },
            { id = "comfortable", label = "Comfortable", overrides = { gap = 12 } },
        }),
    },
    
    render = function(props, data, ctx)
        local columns = {}
        local statuses = { "todo", "in_progress", "done" }
        
        for _, status in ipairs(statuses) do
            local cards = table.filter(data, function(item)
                return item[props.status_field] == status
            end)
            table.insert(columns, {
                component = "container",
                props = { direction = "column", gap = ctx.variants.density.gap },
                children = table.map(cards, function(card)
                    return {
                        component = "card",
                        props = { title = card[props.title_field] },
                        signals = {
                            clicked = function()
                                ctx.fire("card_moved", {
                                    card_id = card.id,
                                    from_column = status,
                                })
                            end,
                        },
                    }
                end),
            })
        end
        
        return { component = "columns", children = columns }
    end,
}
```

### Render strategy — node-tree intermediary

The Luau `render` function returns a **virtual node tree** of the same
shape as `prism_builder::Node` (component id + props + children) and
the host walks that tree through the existing `Component` registry to
emit Slint *or* HTML. Slint DSL is never produced directly from Luau.

Rationale:

- Symmetry between Slint (`render_slint`) and HTML SSR (`render_html`,
  used by `prism-relay`) — one Luau script renders to both targets
  for free.
- The walker can call back into `Component::render_slint` for built-in
  components, so a Luau-defined widget composes natively with `Card`,
  `Container`, `Form`, etc.
- Failures localise to a single virtual node rather than a malformed
  Slint string that breaks the whole page.

The trade-off (no escape hatch into raw Slint) is acceptable because
authors who need raw Slint can still define a Rust `Component` —
Luau is the high-leverage path, not the only path.

### Registration pipeline

```rust
// New: prism-builder/src/luau_component.rs
pub struct LuauComponent {
    contribution: WidgetContribution,   // built from the Luau table
    source: String,                     // .luau source (round-tripped for hot-reload)
    render_key: LuauRenderKey,          // registry key into the shared Lua state
}

/// Registry of compiled Luau render functions, keyed by component id.
/// One Lua state per shell instance; `LuauComponent` holds only a key
/// so it stays Send + Sync (Lua state is !Send).
pub struct LuauRenderRegistry {
    lua: mlua::Lua,
    render_fns: HashMap<LuauRenderKey, mlua::RegistryKey>,
}

impl Component for LuauComponent {
    fn id(&self) -> &str { &self.contribution.id }
    fn schema(&self) -> Vec<FieldSpec> { self.contribution.config_fields.clone() }

    fn render_slint(&self, node: &Node, ctx: &mut RenderSlintContext) -> Result<(), RenderError> {
        // 1. Resolve data (DataQuery, ObjectQuery, Lookup) using the same
        //    paths Component impls already use.
        // 2. Call render_fn(props, data, render_ctx) -> Lua table.
        // 3. Convert the returned table into Vec<Node> via FromLua.
        // 4. For each child node, look up Component in ctx.registry and
        //    delegate to its render_slint(). Built-ins handle themselves;
        //    nested LuauComponents recurse through this same path.
        let nodes = ctx.luau.invoke_render(&self.render_key, node, ctx)?;
        for child in nodes {
            ctx.registry.render_slint(&child, ctx)?;
        }
        Ok(())
    }

    fn render_html(&self, node: &Node, ctx: &mut RenderHtmlContext) -> Result<(), RenderError> {
        // Same call shape as render_slint — the only difference is which
        // walker the children pass through. The Luau render function
        // doesn't know which target it's serving.
    }
}
```

### Hot-reload — per-component re-registration

When the VFS watcher fires for a `.luau` widget file, the host
re-parses the contribution table and calls
`LuauRenderRegistry::replace(component_id, new_source)`. Only the
single component re-registers; other components keep their compiled
render functions. Module-level `local` state in the changed file is
discarded by design — components that need persistent state stash it
on `prism.store` (Phase 3) or the object graph, not on the Luau module
scope. The render walker re-invokes the new function on the next
sync pass; in-flight coroutines from the old version run to completion
against the old `RegistryKey`, which is dropped when the last
reference goes out of scope.

This keeps reload cost proportional to the changed file and avoids
re-parsing every widget on every save. The shared `mlua::Lua` instance
is reused — one Lua state per shell, not per component.

---

## Phase 7: Prism Manifest Scripting  ⬜ not started

The `.prism.json` manifest gains a `scripts` section:

```json
{
    "scripts": {
        "widgets": ["widgets/*.luau"],
        "automations": ["automations/*.luau"],
        "build_steps": ["build/*.luau"],
        "commands": ["commands/*.luau"]
    },
    "permissions": {
        "widgets/*.luau": { "mode": "build", "scope": ["document", "tokens"] },
        "automations/*.luau": { "mode": "admin", "scope": ["objects", "edges", "signals"] }
    }
}
```

Scripts are loaded at boot, validated against their declared capability
scope, and registered into their respective registries. Hot-reload via
the VFS watcher.

---

## Work Breakdown

### Crate changes

| Crate | Work |
|-------|------|
| `prism-luau-derive` (new) | Proc macro: `#[luau_expose]` for structs, enums, fns |
| `prism-core` | Annotate: `DesignTokens`, `ShellMode`, `Store`, `Atom`, `CrdtSync`, `ObjectRegistry`, `ConfigModel`, `FeatureFlags`, expression evaluator, `GraphObject`, `ObjectEdge`, `FieldSpec`, `FieldKind`, geometry types, spatial types |
| `prism-builder` | Annotate: `BuilderDocument`, `Node`, `PrismApp`, `Page`, `ComponentRegistry`, `LayoutMode`, `StyleProperties`, `Connection`, `SignalDef`, `ResourceDef`, `PrefabDef`, `FacetDef`. New: `LuauComponent` renderer |
| `prism-daemon` | Replace bare `Lua::new()` with `PrismContext`-equipped sandbox. New commands: `luau.eval` (REPL), `luau.register_widget`, `luau.register_automation`. Lifecycle management for persistent scripts |
| `prism-shell` | Wire `PrismContext` into signal dispatch (`ActionKind::Custom`), facet resolution (`FacetKind::Script`), command palette REPL panel. Luau-defined widgets hot-reload on VFS change |
| `prism-cli` | `prism codegen luau-types` subcommand. `prism dev` watches `.luau` files |

### Dependency additions

| Crate | New dep | Why |
|-------|---------|-----|
| `prism-luau-derive` | `syn`, `quote`, `proc-macro2` | Proc macro standard toolkit |
| `prism-core` | `prism-luau-derive` (optional, behind `luau` feature) | Annotations on types |
| `prism-builder` | `mlua` (optional, behind `luau` feature) | `LuauComponent` runtime |
| `prism-daemon` | (already has `mlua`) | Expand sandbox setup |

### Migration path (no big bang)

1. ✅ Ship `prism-luau-derive` with support for flat structs + simple enums.
2. ✅ Annotate the leaf types (`DesignTokens`, `Rgba`, `Spacing`, `Radius`,
   `Typography`, `ShellMode`, `Permission`, `EntityFieldType`,
   `RollupFunction`, `EnumOption`, `UiHints`, `EdgeBehavior`, `EdgeScope`,
   `EdgeCascade`, `DefaultChildView`, `TabDefinition`, `ApiOperation`,
   `DefaultSort`, `SortDir`, `SettingScope`, `SettingType`).
3. ✅ Build `PrismContext` with `tokens` + `shell_mode` + `permission` +
   `objects` (read API) + `config` access. Wired through `luau.exec`,
   `Custom` signal handlers, and `FacetKind::Script` resolution.
4. 🟡 Expand `objects` to a write API + add `edges`. Sub-tasks:
   - 4a. Promote the free `luau_module::exec` to `LuauModule::exec`
     holding `Arc<DocManager>`, `Rc<CrdtSync>`, and the existing
     `ObjectRegistry` / `ConfigModel` handles. Re-register the
     `luau.exec` command against the method.
   - 4b. Extend `ObjectsHandle` with `create` / `update` / `delete` /
     `query` methods on `prism-core::luau_bindings`. Each mutation
     funnels through `CrdtSync::apply_object_change` so peers see
     identical deltas.
   - 4c. Add `EdgesHandle` (new userdata in `luau_bindings.rs`) with
     `create` / `delete` / `query`, backed by
     `CrdtSync::apply_edge_change`. Mirror the existing
     `ObjectsHandle` test pattern in `prism_context.rs`.
   - 4d. Annotate `GraphObject` / `ObjectEdge` mutation payload types
     with `#[luau_expose]` so `.d.luau` stubs land for free.
5. ⬜ Expand to `document` + `signals`. Sub-tasks:
   - 5a. New `DocumentHandle` userdata in
     `prism-shell/src/luau/document.rs` wrapping
     `Rc<RefCell<Store<AppState>>>`. Methods: `find`, `insert`,
     `remove`, `move`, `set_prop`, `prop`. All mutations go through
     the existing `Store::mutate` path so undo snapshots and live-doc
     source edits stay in sync.
   - 5b. New `SignalsHandle` re-entrantly calling
     `ShellInner::fire_signal` (Custom-handler entry) or queuing the
     event for the next sync pass (facet-resolver entry, to avoid
     mid-sync recursion).
   - 5c. Shell-side `PrismContext` builder: extend
     `ShellInner::exec_custom_handlers` to install `document`,
     `signals`, `selection`, `app` before delegating to
     `LuauModule::exec`. `sync_builder_document`'s `FacetKind::Script`
     branch installs the same handles in read-only mode.
   - 5d. Replace the ad-hoc `_actions` / `set_properties` /
     `navigate` return-value protocol in `apply_luau_result` with
     direct handle calls — the script mutates `prism.document`
     directly instead of returning an action list. The old protocol
     stays for one release behind a deprecation warning so existing
     handler scripts keep working.
6. ⬜ `LuauComponent` renderer + manifest `scripts` section.
   Sub-tasks:
   - 6a. New `prism-builder/src/luau_component.rs` with
     `LuauComponent` (impls `Component` + `HtmlBlock`) and
     `LuauRenderRegistry` owning the shared `mlua::Lua` and a
     `HashMap<LuauRenderKey, mlua::RegistryKey>`.
   - 6b. Virtual node tree: `FromLua` impl on a new
     `prism_builder::VirtualNode` (same shape as `Node` but no IDs)
     so the walker can recurse through `ComponentRegistry` for both
     Slint and HTML targets.
   - 6c. `prism.widget { ... }` global helper (Luau-side) that builds
     a `WidgetContribution` from the table and registers the render
     function. Backed by a Rust-side `register_widget` callback
     installed by `LuauRenderRegistry::install_global`.
   - 6d. `.prism.json` `scripts` section + capability scope
     enforcement. Loader walks the glob, classifies by directory
     (`widgets/`, `automations/`, `build_steps/`, `commands/`),
     and registers each into its respective registry. VFS watcher
     calls `LuauRenderRegistry::replace` on change for per-component
     reload.
   - 6e. CLI hook: `prism dev shell` passes the project's
     `scripts.widgets` glob to the shell at boot so Luau-defined
     widgets appear in the component palette alongside built-ins.
7. ⬜ Port `Tabs` to pure Luau. Sub-tasks:
   - 7a. Author `widgets/tabs.luau` mirroring the existing
     `prism_builder::core_widget::tabs` schema (tab list field,
     active-tab signal).
   - 7b. Side-by-side parity test: render a document containing both
     the Rust `tabs` and the Luau `tabs-luau` and assert
     `render_html` output is byte-identical (after id-stripping).
   - 7c. Once parity holds, remove the Rust `tabs` impl and rename
     `tabs-luau` back to `tabs` so existing documents migrate
     transparently. This is the canary for the broader port.

---

## What This Eliminates

Status legend: ✅ realised today · 🟡 partial · ⬜ pending.

| Before | After | Status |
|--------|-------|--------|
| Hand-written `WidgetContribution` struct per widget (20+ fields) | `prism.widget { ... }` in Luau — 10 lines | ⬜ Phase 6 |
| `impl Component for X` + `impl HtmlBlock for X` per widget | `render` function in Luau, walker handles both targets | ⬜ Phase 6 |
| Hand-written JSON marshalling in every daemon module | `#[luau_expose]` auto-derives `UserData` impls | 🟡 leaf types annotated; stateful subsystems hand-roll via `manual_impl` (intentional — see Phase 1) |
| Hand-maintained `.d.luau` type stubs | Generated from annotated Rust types | 🟡 `prism codegen luau-types` ships `core.d.luau` + `builder.d.luau` + `signals.d.luau`; deeper `prism-builder` annotation (`BuilderDocument` / `Node` / `Connection` / `LayoutMode` / `FacetDef` / …) still pending |
| Duplicate `FieldSpec` builder calls across Rust + signal stubs | Single `prism.field.*` API in Luau, backed by the same `FieldSpec` | ⬜ Phase 6 |
| `ActionKind::Custom { handler }` as dead code | Live execution path through `PrismContext` | ✅ shell `fire_signal` runs Custom handlers via `luau_module::exec` |
| Separate `luau.exec` fire-and-forget model | Persistent scripts with subscriptions and lifecycle | ⬜ Phase 3 |

---

## Security Model

```
┌─────────────────────────────────────────────────┐
│                  Trust Levels                     │
├─────────────┬───────────────────────────────────┤
│ User mode   │ Read tokens, config, own data     │
│ Build mode  │ + Write document, fire signals    │
│ Admin mode  │ + Full object tree, crypto, VFS   │
├─────────────┼───────────────────────────────────┤
│ Sandboxed   │ No os/io/debug stdlib access      │
│ Metered     │ Instruction count limits           │
│ Audited     │ All mutations logged to activity   │
└─────────────┴───────────────────────────────────┘
```

The existing `identity::trust` module's Luau sandbox, hashcash gate, and
schema poison-pill validator compose with `PrismContext` capability scoping.
Scripts from untrusted peers get `User` mode with instruction metering.
Local scripts default to `Build` mode. Only manifest-declared admin
scripts get full access.

---

## Resolved Decisions

1. **Render path** — node-tree intermediary only. Luau `render`
   functions return virtual nodes; the host walker handles Slint and
   HTML emission via the existing `Component` registry. No raw Slint
   escape hatch from Luau. See Phase 6 "Render strategy".

2. **Async model** — coroutines. Async handle methods capture the
   current coroutine, fire the underlying op, and resume on
   completion. Subscription callbacks stay synchronous; callbacks
   needing async work spawn via `prism.spawn(fn)`. See Phase 3
   "Async model — coroutines".

3. **Hot-reload granularity** — per-component re-registration. The
   VFS watcher calls `LuauRenderRegistry::replace(component_id,
   source)` for the changed file only. Module-level state is
   discarded by design; persistent state belongs on `prism.store`
   or the object graph. See Phase 6 "Hot-reload —
   per-component re-registration".

## Resolved Decisions (cont.)

4. **Lua state granularity** — one `mlua::Lua` per document (page),
   not per shell. A runaway widget (infinite loop, runaway alloc,
   panic in `FromLua`) is contained to the document that hosts it;
   other documents and the shell chrome keep running. The same
   isolation answers automation actor scope: each automation actor
   gets its own `Lua`, matching the existing `actors_module` pattern.

   Implications for Phase 6:
   - `LuauRenderRegistry` becomes per-document, owned by the
     `DocumentHandle` (or a sibling field on the document's runtime
     state) rather than `ShellInner`. Hot-reload still replaces
     a single `RegistryKey` within that document's state.
   - Cross-document calls go through the daemon RPC surface
     (`luau.exec`), not direct Lua-to-Lua, so each state stays an
     isolation boundary.
   - Shared infrastructure (compiled bytecode cache, type stubs,
     `WidgetContribution` metadata) lives on the shell and is
     copied into each new `Lua` at boot — cheap because it's
     metadata, not state.
   - Instruction metering (`set_interrupt`) and memory caps are
     set per-`Lua`, so a misbehaving widget hits its own ceiling
     instead of the global one.

## Still Open

- **Cross-document subscriptions** — if a script in document A
  watches an atom mutated by document B, the wakeup has to cross
  Lua states. Likely path: the atom layer fires a host-side event
  that each subscribed `Lua` picks up on its own resume tick.
  Pin down when Phase 3 lands.
