//! Hand-rolled type-stub constants for builder structs/enums whose
//! `#[serde(tag = "...")]` discriminator names don't match the
//! `#[luau_expose]` macro's fixed `tag` literal. Same pattern
//! `prism-core::luau_bindings_consts` uses for stateful `UserData`
//! types.
//!
//! Each constant carries the canonical Luau `export type ...` body
//! that mirrors the serde wire shape so a script that reads one of
//! these out of a document JSON sees fields LuaLS already knows
//! about. The constants are always-compiled (no `luau` feature gate)
//! because the codegen pipeline scrapes them through
//! [`crate::luau_types::type_defs`] regardless of whether the
//! `mlua`-backed runtime is on.

pub const SIGNAL_DEF_TYPE_NAME: &str = "SignalDef";
pub const SIGNAL_DEF_TYPE_DEF: &str = r#"export type SignalDef = {
    name: string,
    description: string,
    payload: {FieldSpec},
}"#;

pub const FIELD_SPEC_TYPE_NAME: &str = "FieldSpec";
pub const FIELD_SPEC_TYPE_DEF: &str = r#"export type FieldSpec = {
    key: string,
    label: string,
    kind: FieldKind,
    default: any,
    required: boolean,
    help: string?,
    group: string?,
}"#;

pub const FIELD_KIND_TYPE_NAME: &str = "FieldKind";
pub const FIELD_KIND_TYPE_DEF: &str = r#"export type FieldKind =
    { kind: "text" }
  | { kind: "text-area" }
  | { kind: "number", data: { min: number?, max: number?, step: number? } }
  | { kind: "integer", data: { min: number?, max: number?, step: number? } }
  | { kind: "boolean" }
  | { kind: "select", data: {{ value: string, label: string }} }
  | { kind: "color" }
  | { kind: "file", data: { accept: {string} } }
  | { kind: "date" }
  | { kind: "date-time" }
  | { kind: "duration" }
  | { kind: "currency", data: { currency_code: string? } }
  | { kind: "calculation", data: { formula: string } }
  | { kind: "custom", data: { tag: string, data: any } }"#;

pub const ACTION_KIND_TYPE_NAME: &str = "ActionKind";
pub const ACTION_KIND_TYPE_DEF: &str = r#"export type ActionKind =
    { type: "set-property", key: string, value: any }
  | { type: "toggle-visibility" }
  | { type: "navigate-to", target: string }
  | { type: "play-animation", animation: string }
  | { type: "emit-signal", signal: string }
  | { type: "custom", handler: string }
  | { type: "bind", target_key: string, source: string }"#;

pub const CONNECTION_TYPE_NAME: &str = "Connection";
pub const CONNECTION_TYPE_DEF: &str = r#"export type Connection = {
    id: string,
    source_node: string,
    signal: string,
    target_node: string,
    action: ActionKind,
    params: any,
}"#;

pub const RESOURCE_KIND_TYPE_NAME: &str = "ResourceKind";
pub const RESOURCE_KIND_TYPE_DEF: &str = r#"export type ResourceKind =
    "style-preset"
  | "color-palette"
  | "typography-scale"
  | "animation-curve"
  | "data-source"
  | "media-asset"
  | "icon-set""#;

pub const RESOURCE_DEF_TYPE_NAME: &str = "ResourceDef";
pub const RESOURCE_DEF_TYPE_DEF: &str = r#"export type ResourceDef = {
    id: string,
    kind: ResourceKind,
    label: string,
    description: string,
    data: any,
}"#;

pub const EXPOSED_SLOT_TYPE_NAME: &str = "ExposedSlot";
pub const EXPOSED_SLOT_TYPE_DEF: &str = r#"export type ExposedSlot = {
    key: string,
    target_node: string,
    target_prop: string,
    spec: FieldSpec,
}"#;

pub const PREFAB_DEF_TYPE_NAME: &str = "PrefabDef";
pub const PREFAB_DEF_TYPE_DEF: &str = r#"export type PrefabDef = {
    id: string,
    label: string,
    description: string,
    root: any,
    exposed: {ExposedSlot},
    variants: {any},
    thumbnail: string?,
}"#;

pub const FLOW_PROPS_TYPE_NAME: &str = "FlowProps";
pub const FLOW_PROPS_TYPE_DEF: &str = r#"export type FlowProps = {
    display: FlowDisplay,
    width: Dimension,
    height: Dimension,
    min_width: Dimension,
    min_height: Dimension,
    max_width: Dimension,
    max_height: Dimension,
    padding: { top: number, right: number, bottom: number, left: number },
    margin: { top: number, right: number, bottom: number, left: number },
    flex_grow: number,
    flex_shrink: number,
    flex_basis: Dimension,
    flex_direction: FlexDirection,
    align_self: AlignOption,
    align_items: AlignOption,
    justify_content: JustifyOption,
    grid_column: GridPlacement,
    grid_row: GridPlacement,
    gap: number,
}"#;

pub const ABSOLUTE_PROPS_TYPE_NAME: &str = "AbsoluteProps";
pub const ABSOLUTE_PROPS_TYPE_DEF: &str = r#"export type AbsoluteProps = {
    width: Dimension,
    height: Dimension,
    min_width: Dimension,
    min_height: Dimension,
    max_width: Dimension,
    max_height: Dimension,
}"#;

pub const LAYOUT_MODE_TYPE_NAME: &str = "LayoutMode";
pub const LAYOUT_MODE_TYPE_DEF: &str = r#"export type LayoutMode =
    { mode: "flow" } & FlowProps
  | { mode: "free" }
  | { mode: "absolute" } & AbsoluteProps
  | { mode: "relative" } & FlowProps"#;

pub const NODE_TYPE_NAME: &str = "Node";
pub const NODE_TYPE_DEF: &str = r#"export type Node = {
    id: string,
    component: string,
    props: any,
    children: {Node},
    layout_mode: LayoutMode,
    transform: any,
    modifiers: {any},
    style: StyleProperties,
}"#;

pub const NAVIGATION_STYLE_TYPE_NAME: &str = "NavigationStyle";
pub const NAVIGATION_STYLE_TYPE_DEF: &str = r#"export type NavigationStyle =
    "Tabs" | "Sidebar" | "BottomBar" | "None""#;

pub const NAVIGATION_CONFIG_TYPE_NAME: &str = "NavigationConfig";
pub const NAVIGATION_CONFIG_TYPE_DEF: &str = r#"export type NavigationConfig = {
    style: NavigationStyle,
}"#;

pub const APP_ICON_TYPE_NAME: &str = "AppIcon";
pub const APP_ICON_TYPE_DEF: &str = r#"export type AppIcon =
    "Cube" | "Globe" | "Music" | "Film" | "Zap" | "Star" | "Heart" | "Code""#;

pub const PAGE_TYPE_NAME: &str = "Page";
pub const PAGE_TYPE_DEF: &str = r#"export type Page = {
    id: string,
    title: string,
    route: string,
    source: string,
    style: StyleProperties,
}"#;

pub const PRISM_APP_TYPE_NAME: &str = "PrismApp";
pub const PRISM_APP_TYPE_DEF: &str = r#"export type PrismApp = {
    id: string,
    name: string,
    description: string,
    icon: AppIcon,
    pages: {Page},
    active_page: number,
    navigation: NavigationConfig,
    style: StyleProperties,
}"#;

pub const BUILDER_DOCUMENT_TYPE_NAME: &str = "BuilderDocument";
pub const BUILDER_DOCUMENT_TYPE_DEF: &str = r#"export type BuilderDocument = {
    root: Node?,
    zones: { [string]: {Node} },
    page_layout: any,
    resources: { [string]: ResourceDef },
    connections: {Connection},
    prefabs: { [string]: PrefabDef },
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_def_carries_its_export_keyword_and_name() {
        for (expected_name, def) in [
            (SIGNAL_DEF_TYPE_NAME, SIGNAL_DEF_TYPE_DEF),
            (FIELD_SPEC_TYPE_NAME, FIELD_SPEC_TYPE_DEF),
            (FIELD_KIND_TYPE_NAME, FIELD_KIND_TYPE_DEF),
            (ACTION_KIND_TYPE_NAME, ACTION_KIND_TYPE_DEF),
            (CONNECTION_TYPE_NAME, CONNECTION_TYPE_DEF),
            (RESOURCE_KIND_TYPE_NAME, RESOURCE_KIND_TYPE_DEF),
            (RESOURCE_DEF_TYPE_NAME, RESOURCE_DEF_TYPE_DEF),
            (EXPOSED_SLOT_TYPE_NAME, EXPOSED_SLOT_TYPE_DEF),
            (PREFAB_DEF_TYPE_NAME, PREFAB_DEF_TYPE_DEF),
            (FLOW_PROPS_TYPE_NAME, FLOW_PROPS_TYPE_DEF),
            (ABSOLUTE_PROPS_TYPE_NAME, ABSOLUTE_PROPS_TYPE_DEF),
            (LAYOUT_MODE_TYPE_NAME, LAYOUT_MODE_TYPE_DEF),
            (NODE_TYPE_NAME, NODE_TYPE_DEF),
            (NAVIGATION_STYLE_TYPE_NAME, NAVIGATION_STYLE_TYPE_DEF),
            (NAVIGATION_CONFIG_TYPE_NAME, NAVIGATION_CONFIG_TYPE_DEF),
            (APP_ICON_TYPE_NAME, APP_ICON_TYPE_DEF),
            (PAGE_TYPE_NAME, PAGE_TYPE_DEF),
            (PRISM_APP_TYPE_NAME, PRISM_APP_TYPE_DEF),
            (BUILDER_DOCUMENT_TYPE_NAME, BUILDER_DOCUMENT_TYPE_DEF),
        ] {
            let header = format!("export type {expected_name}");
            assert!(
                def.contains(&header),
                "type def for `{expected_name}` missing `{header}`:\n{def}"
            );
        }
    }

    #[test]
    fn layout_mode_discriminator_matches_serde_mode_tag() {
        // `LayoutMode` uses `#[serde(tag = "mode", rename_all = "kebab-case")]`.
        // Lock the stub to that wire shape — the macro doesn't honour
        // custom serde tags so this file is the single source of
        // truth for what scripts see when reading `node.layout_mode`.
        for tag in ["flow", "free", "absolute", "relative"] {
            let needle = format!(r#"mode: "{tag}""#);
            assert!(
                LAYOUT_MODE_TYPE_DEF.contains(&needle),
                "LayoutMode stub missing variant `{tag}`"
            );
        }
    }

    #[test]
    fn action_kind_tags_match_serde_kebab() {
        // The macro can't auto-generate this enum because it uses
        // `#[serde(tag = "type", rename_all = "kebab-case")]`. Lock
        // the hand-written stub to the serde shape so the two can't
        // silently drift.
        for tag in [
            "set-property",
            "toggle-visibility",
            "navigate-to",
            "play-animation",
            "emit-signal",
            "custom",
            "bind",
        ] {
            let needle = format!(r#"type: "{tag}""#);
            assert!(
                ACTION_KIND_TYPE_DEF.contains(&needle),
                "ActionKind stub missing variant `{tag}`"
            );
        }
    }
}
