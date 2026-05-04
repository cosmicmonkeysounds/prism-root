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
