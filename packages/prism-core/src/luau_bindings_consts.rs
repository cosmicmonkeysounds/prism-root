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
pub const OBJECTS_HANDLE_TYPE_DEF: &str = r#"export type Objects = {
    list_types: (self: Objects) -> {string},
    get_type: (self: Objects, type_name: string) -> any?,
    list_edge_types: (self: Objects) -> {string},
    get_edge_type: (self: Objects, relation: string) -> any?,
    get_category: (self: Objects, type_name: string) -> string,
    can_connect: (self: Objects, relation: string, source_type: string, target_type: string) -> boolean,
    get_effective_tabs: (self: Objects, type_name: string) -> {TabDefinition},
    get_entity_fields: (self: Objects, type_name: string) -> {any},
}"#;

pub const CONFIG_HANDLE_TYPE_NAME: &str = "Config";
pub const CONFIG_HANDLE_TYPE_DEF: &str = r#"export type Config = {
    get: (self: Config, key: string) -> any,
    set: (self: Config, key: string, value: any, scope: SettingScope?) -> (),
    reset: (self: Config, key: string, scope: SettingScope?) -> (),
    is_overridden: (self: Config, key: string) -> boolean,
}"#;
