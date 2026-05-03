# Luau Deep Integration Plan

> Goal: Luau scripts can access, read, edit, create, and generate anything in
> Prism. Eliminate duplicate struct definitions by deriving Luau type stubs and
> runtime bindings from the same Rust source that defines the types.

---

## Current State

| Layer | What exists | Gap |
|-------|------------|-----|
| Daemon (`luau_module.rs`) | `luau.exec` — fire-and-forget script with JSON args/return | No access to kernel state, CRDT, VFS, objects, or signals |
| Core (`language/luau/`) | Parser (full-moon), syntax provider, visual language, signal-aware completions | Read-only intelligence — no mutation path |
| Builder (`signal.rs`) | `generate_signal_type_stubs` → `.d.luau` for LuaLS | Only signals, not the full type surface |
| Shell | Signal dispatch has `ActionKind::Custom { handler }` | Dead end — no runtime to invoke the handler |

**The fundamental gap**: Luau lives in an isolated sandbox with JSON
in/out. It can't touch the object tree, query data, mutate documents,
fire signals, subscribe to state changes, or access the VFS. Every
integration requires hand-writing a new daemon command + JSON marshalling.

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

## Phase 1: Derive Macro — `#[luau_expose]`

### New crate: `prism-luau-derive`

A proc-macro crate that generates mlua bindings + type stubs from Rust types.

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

## Phase 2: PrismContext — The God Object

A single userdata injected as a global into every Luau execution context.
Provides namespaced access to every Prism subsystem.

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

## Phase 3: Reactive Subscriptions

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

### 4.1 Signal handlers (Connection::Custom)

```luau
-- Attached to a Connection with ActionKind::Custom
-- Receives the signal payload + document context
function on_button_click(event)
    local count = prism.document:find(event.source):prop("count") or 0
    prism.document:find(event.source):set_prop("count", count + 1)
end
```

### 4.2 Facet resolvers (FacetKind::Script)

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

### 4.3 Automation actions (AutomationEngine)

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

### 4.4 Computed fields (expression extension)

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

### 4.5 CLI plugins / build steps

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

### 4.6 Widget templates (WidgetTemplate scriptable nodes)

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

### 4.7 Interactive shell / REPL

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

## Phase 5: Type Stub Generation Pipeline

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

## Phase 6: Declarative Widget Definition in Luau

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

### Registration pipeline

```rust
// In prism-builder, a new module: luau_component.rs
pub struct LuauComponent {
    contribution: WidgetContribution,  // derived from the Luau table
    source: String,                     // the .luau source
    render_fn: mlua::Function,          // cached compiled render function
}

impl Component for LuauComponent {
    fn id(&self) -> &str { &self.contribution.id }
    fn schema(&self) -> Vec<FieldSpec> { /* from contribution.config_fields */ }
    fn render_slint(&self, node: &Node, ctx: &mut RenderSlintContext) -> Result<(), RenderError> {
        // 1. Call render_fn(props, data, luau_ctx)
        // 2. Walk returned node tree
        // 3. Emit Slint for each node via ctx.registry lookups
    }
}
```

---

## Phase 7: Prism Manifest Scripting

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

1. Ship `prism-luau-derive` with support for flat structs + simple enums.
2. Annotate 5 leaf types (`DesignTokens`, `Rgba`, `Spacing`, `Radius`, `Typography`).
3. Build `PrismContext` with just `tokens` + `config` access. Run existing tests.
4. Expand to `objects` + `edges` (requires wiring `CrdtSync` into the sandbox).
5. Expand to `document` + `signals` (requires shell integration).
6. Ship `LuauComponent` renderer + manifest `scripts` section.
7. Port one existing built-in widget (e.g. Tabs) to pure Luau as proof-of-concept.

---

## What This Eliminates

| Before | After |
|--------|-------|
| Hand-written `WidgetContribution` struct per widget (20+ fields) | `prism.widget { ... }` in Luau — 10 lines |
| `impl Component for X` + `impl HtmlBlock for X` per widget | `render` function in Luau, walker handles both targets |
| Hand-written JSON marshalling in every daemon module | `#[luau_expose]` auto-derives `UserData` impls |
| Hand-maintained `.d.luau` type stubs | Generated from annotated Rust types |
| Duplicate `FieldSpec` builder calls across Rust + signal stubs | Single `prism.field.*` API in Luau, backed by the same `FieldSpec` |
| `ActionKind::Custom { handler }` as dead code | Live execution path through `PrismContext` |
| Separate `luau.exec` fire-and-forget model | Persistent scripts with subscriptions and lifecycle |

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

## Open Questions

1. **Should Luau-defined widgets support Slint DSL emission directly, or
   always go through the node-tree intermediary?** Node-tree is simpler
   and works for both Slint + HTML targets; direct DSL emission is more
   powerful but splits the render path.

2. **Async model**: Should long-running scripts (data fetches, AI calls)
   use coroutines (`coroutine.yield`) or a callback/promise pattern?
   Coroutines align with Luau's native model; callbacks align with the
   existing signal/subscription pattern.

3. **Hot-reload granularity**: When a `.luau` widget file changes, do we
   re-register just that component, or reload the entire script context?
   Per-component is faster but harder to implement if scripts share
   module-level state.
