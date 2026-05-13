//! Type-stub string constants for the hand-rolled stateful Luau
//! surfaces. Lives outside [`crate::luau_bindings`] (which is gated on
//! the `luau` feature) so the codegen pipeline can scrape them
//! without flipping the runtime feature on. Keep these strings in
//! lockstep with the `UserData` impls in `luau_bindings.rs`.

pub const GRAPH_OBJECT_TYPE_NAME: &str = "GraphObject";
pub const GRAPH_OBJECT_TYPE_DEF: &str = r#"export type GraphObject = {
    id: string,
    type: string,
    name: string,
    parent_id: string?,
    position: number,
    status: string?,
    tags: {string},
    date: string?,
    end_date: string?,
    description: string,
    color: string?,
    image: string?,
    pinned: boolean,
    data: { [string]: any },
    created_at: string,
    updated_at: string,
    deleted_at: string?,
}"#;

pub const OBJECT_EDGE_TYPE_NAME: &str = "ObjectEdge";
pub const OBJECT_EDGE_TYPE_DEF: &str = r#"export type ObjectEdge = {
    id: string,
    source_id: string,
    target_id: string,
    relation: string,
    position: number?,
    created_at: string,
    data: { [string]: any },
}"#;

pub const OBJECTS_HANDLE_TYPE_NAME: &str = "Objects";
pub const OBJECTS_HANDLE_TYPE_DEF: &str = r#"export type ObjectFilter = {
    types: {string}?,
    tags: {string}?,
    statuses: {string}?,
    parent_id: string?,
    exclude_deleted: boolean?,
}

export type Objects = {
    list_types: (self: Objects) -> {string},
    get_type: (self: Objects, type_name: string) -> any?,
    list_edge_types: (self: Objects) -> {string},
    get_edge_type: (self: Objects, relation: string) -> any?,
    get_category: (self: Objects, type_name: string) -> string,
    can_connect: (self: Objects, relation: string, source_type: string, target_type: string) -> boolean,
    get_effective_tabs: (self: Objects, type_name: string) -> {TabDefinition},
    get_entity_fields: (self: Objects, type_name: string) -> {any},
    -- Phase 4 instance API. Available only when the host installs a
    -- live collection (shell-side); errors out in daemon-only contexts.
    get: (self: Objects, id: string) -> GraphObject?,
    list: (self: Objects, filter: ObjectFilter?) -> {GraphObject},
    query: (self: Objects, filter: ObjectFilter?) -> {GraphObject},
    create: (self: Objects, type_name: string, payload: { [string]: any }?) -> string,
    update: (self: Objects, id: string, patch: { [string]: any }) -> (),
    delete: (self: Objects, id: string) -> boolean,
}"#;

pub const EDGES_HANDLE_TYPE_NAME: &str = "Edges";
pub const EDGES_HANDLE_TYPE_DEF: &str = r#"export type EdgeFilter = {
    source_id: string?,
    target_id: string?,
    relation: string?,
}

export type Edges = {
    -- Phase 4 instance API. Available only when the host installs a
    -- live collection (shell-side); errors out in daemon-only contexts.
    get: (self: Edges, id: string) -> ObjectEdge?,
    list: (self: Edges, filter: EdgeFilter?) -> {ObjectEdge},
    query: (self: Edges, filter: EdgeFilter?) -> {ObjectEdge},
    create: (self: Edges, relation: string, payload: { source_id: string, target_id: string, position: number?, data: { [string]: any }?, id: string? }) -> string,
    delete: (self: Edges, id: string) -> boolean,
}"#;

pub const CONFIG_HANDLE_TYPE_NAME: &str = "Config";
pub const CONFIG_HANDLE_TYPE_DEF: &str = r#"export type Config = {
    get: (self: Config, key: string) -> any,
    set: (self: Config, key: string, value: any, scope: SettingScope?) -> (),
    reset: (self: Config, key: string, scope: SettingScope?) -> (),
    is_overridden: (self: Config, key: string) -> boolean,
}"#;

// ───── RegistrarHandle (DSL self-bootstrap Loop 4) ────────────────────
// Type stub for the `prism.app` userdata in `crate::luau_bindings`.
// Mirrors `prism_core::app_registry`'s {Panel,Component,Service}
// Registration shapes — keep these in lockstep when fields move.

pub const REGISTRAR_HANDLE_TYPE_NAME: &str = "AppRegistrar";
pub const REGISTRAR_HANDLE_TYPE_DEF: &str = r#"export type PanelRegistration = {
    id: string,
    label: string?,
    icon_hint: string?,
    min_width: number?,
    min_height: number?,
    allow_multiple: boolean?,
    tag: string?,
}

export type ComponentRegistration = {
    id: string,
    render_key: string?,
    render: ((props: any, children: any) -> any)?,
}

export type ServiceRegistration = {
    id: string,
    on_event_key: string?,
    on_event: ((ctx: any, event: any) -> any)?,
}

export type AppRegistrar = {
    register_panel: (self: AppRegistrar, spec: PanelRegistration) -> (),
    register_component: (self: AppRegistrar, spec: ComponentRegistration) -> (),
    register_service: (self: AppRegistrar, spec: ServiceRegistration) -> (),
}"#;

// ───── Reactive substrate (Phase 5) ───────────────────────────────────
// Type stubs for the userdata impls in `crate::luau_reactive`. Lands
// in a sibling `reactive.d.luau` file (next to `signals.d.luau`) once
// the codegen pipeline picks it up. The `Signal` exposed here is the
// Phase 5 reactive cell (`prism-core::reactive::Signal<Value>`), not
// the `prism_builder::signal::SignalDef` event channel — see §5 of
// `docs/dev/dioxus-inspiration.md` for the naming hygiene rule.

pub const REACTIVE_SIGNAL_TYPE_NAME: &str = "Signal";
pub const REACTIVE_SIGNAL_TYPE_DEF: &str = r#"export type Signal = {
    read: (self: Signal) -> any,
    peek: (self: Signal) -> any,
    track: (self: Signal) -> (),
    write: (self: Signal, value: any) -> (),
    set: (self: Signal, value: any) -> (),
}"#;

pub const REACTIVE_MEMO_TYPE_NAME: &str = "Memo";
pub const REACTIVE_MEMO_TYPE_DEF: &str = r#"export type Memo = {
    read: (self: Memo) -> any,
    peek: (self: Memo) -> any,
}"#;
