//! `plugin_bundles::assets` — Asset management bundle.
//!
//! Port of `kernel/plugin-bundles/assets/assets.ts`: media assets,
//! content items, scanned documents, and user-defined collections.

use serde_json::json;

use super::builders::{
    edge_def, entity_def, enum_options, owned_strings, ui_hidden, ui_multiline, ui_placeholder,
    ui_readonly, ui_readonly_multiline, EdgeSpec, EntitySpec, Field,
};
use super::install::{PluginBundle, PluginInstallContext};
use crate::foundation::object_model::types::DefaultChildView;
use crate::foundation::object_model::{
    EdgeBehavior, EdgeTypeDef, EntityDef, EntityFieldDef, EntityFieldType, EnumOption,
};
use crate::kernel::plugin::{
    plugin_id, ActivityBarContributionDef, ActivityBarPosition, CommandContributionDef,
    PluginContributions, PrismPlugin, ViewContributionDef, ViewZone,
};

// ── Domain constants ────────────────────────────────────────────────────────

pub mod assets_categories {
    pub const MEDIA: &str = "assets:media";
    pub const CONTENT: &str = "assets:content";
    pub const COLLECTIONS: &str = "assets:collections";
}

pub mod assets_types {
    pub const MEDIA_ASSET: &str = "assets:media-asset";
    pub const CONTENT_ITEM: &str = "assets:content-item";
    pub const SCANNED_DOC: &str = "assets:scanned-doc";
    pub const COLLECTION: &str = "assets:collection";
}

pub mod assets_edges {
    pub const IN_COLLECTION: &str = "assets:in-collection";
    pub const DERIVED_FROM: &str = "assets:derived-from";
    pub const ATTACHED_TO: &str = "assets:attached-to";
}

// ── Option tables ───────────────────────────────────────────────────────────

fn media_kinds() -> Vec<EnumOption> {
    enum_options(&[
        ("image", "Image"),
        ("video", "Video"),
        ("audio", "Audio"),
        ("document", "Document"),
        ("archive", "Archive"),
        ("other", "Other"),
    ])
}

// ── Fields ──────────────────────────────────────────────────────────────────

fn media_asset_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("mediaKind", EntityFieldType::Enum)
            .label("Kind")
            .enum_values(media_kinds())
            .required()
            .build(),
        Field::new("mimeType", EntityFieldType::String)
            .label("MIME Type")
            .ui(ui_readonly())
            .build(),
        Field::new("fileSize", EntityFieldType::Int)
            .label("File Size (bytes)")
            .ui(ui_readonly())
            .build(),
        Field::new("width", EntityFieldType::Int)
            .label("Width (px)")
            .build(),
        Field::new("height", EntityFieldType::Int)
            .label("Height (px)")
            .build(),
        Field::new("duration", EntityFieldType::Float)
            .label("Duration (sec)")
            .build(),
        Field::new("blobRef", EntityFieldType::String)
            .label("Blob Reference")
            .ui(ui_hidden())
            .build(),
        Field::new("thumbnailRef", EntityFieldType::String)
            .label("Thumbnail Ref")
            .ui(ui_hidden())
            .build(),
        Field::new("altText", EntityFieldType::String)
            .label("Alt Text")
            .build(),
        Field::new("caption", EntityFieldType::Text)
            .label("Caption")
            .ui(ui_multiline())
            .build(),
        Field::new("tags", EntityFieldType::String)
            .label("Tags")
            .ui(ui_placeholder("comma-separated"))
            .build(),
        Field::new("source", EntityFieldType::Url)
            .label("Source URL")
            .build(),
        Field::new("license", EntityFieldType::String)
            .label("License")
            .build(),
    ]
}

fn content_item_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("contentType", EntityFieldType::Enum)
            .label("Content Type")
            .enum_values(enum_options(&[
                ("article", "Article"),
                ("note", "Note"),
                ("snippet", "Snippet"),
                ("template", "Template"),
                ("reference", "Reference"),
            ]))
            .build(),
        Field::new("body", EntityFieldType::Text)
            .label("Body")
            .ui(ui_multiline())
            .build(),
        Field::new("summary", EntityFieldType::Text)
            .label("Summary")
            .ui(ui_multiline())
            .build(),
        Field::new("tags", EntityFieldType::String)
            .label("Tags")
            .ui(ui_placeholder("comma-separated"))
            .build(),
        Field::new("author", EntityFieldType::String)
            .label("Author")
            .build(),
        Field::new("publishedAt", EntityFieldType::Datetime)
            .label("Published At")
            .build(),
        Field::new("sourceUrl", EntityFieldType::Url)
            .label("Source URL")
            .build(),
        Field::new("wordCount", EntityFieldType::Int)
            .label("Word Count")
            .ui(ui_readonly())
            .build(),
    ]
}

fn scanned_doc_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("blobRef", EntityFieldType::String)
            .label("Scan Blob Ref")
            .ui(ui_hidden())
            .build(),
        Field::new("mimeType", EntityFieldType::String)
            .label("MIME Type")
            .ui(ui_readonly())
            .build(),
        Field::new("pageCount", EntityFieldType::Int)
            .label("Pages")
            .build(),
        Field::new("ocrText", EntityFieldType::Text)
            .label("OCR Text")
            .ui(ui_readonly_multiline())
            .build(),
        Field::new("ocrConfidence", EntityFieldType::Float)
            .label("OCR Confidence")
            .ui(ui_readonly())
            .build(),
        Field::new("language", EntityFieldType::String)
            .label("Language")
            .build(),
        Field::new("category", EntityFieldType::String)
            .label("Category")
            .build(),
        Field::new("scannedAt", EntityFieldType::Datetime)
            .label("Scanned At")
            .ui(ui_readonly())
            .build(),
        Field::new("tags", EntityFieldType::String)
            .label("Tags")
            .ui(ui_placeholder("comma-separated"))
            .build(),
    ]
}

fn collection_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("description", EntityFieldType::Text)
            .label("Description")
            .ui(ui_multiline())
            .build(),
        Field::new("color", EntityFieldType::Color)
            .label("Color")
            .build(),
        Field::new("sortField", EntityFieldType::String)
            .label("Sort Field")
            .build(),
        Field::new("sortDirection", EntityFieldType::Enum)
            .label("Sort Direction")
            .enum_values(enum_options(&[
                ("asc", "Ascending"),
                ("desc", "Descending"),
            ]))
            .default(json!("asc"))
            .build(),
        Field::new("itemCount", EntityFieldType::Int)
            .label("Items")
            .default(json!(0))
            .ui(ui_readonly())
            .build(),
        Field::new("isSmartCollection", EntityFieldType::Bool)
            .label("Smart Collection")
            .default(json!(false))
            .build(),
        Field::new("filterExpression", EntityFieldType::String)
            .label("Filter Expression")
            .ui(ui_placeholder("e.g. tags contains 'photo'"))
            .build(),
    ]
}

// ── Entity + edge defs ──────────────────────────────────────────────────────

pub fn build_entity_defs() -> Vec<EntityDef> {
    vec![
        entity_def(EntitySpec {
            type_name: assets_types::MEDIA_ASSET,
            nsid: "io.prismapp.assets.media-asset",
            category: assets_categories::MEDIA,
            label: "Media Asset",
            plural_label: "Media Assets",
            default_child_view: Some(DefaultChildView::Grid),
            child_only: false,
            extra_child_types: None,
            fields: media_asset_fields(),
        }),
        entity_def(EntitySpec {
            type_name: assets_types::CONTENT_ITEM,
            nsid: "io.prismapp.assets.content-item",
            category: assets_categories::CONTENT,
            label: "Content Item",
            plural_label: "Content Items",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: None,
            fields: content_item_fields(),
        }),
        entity_def(EntitySpec {
            type_name: assets_types::SCANNED_DOC,
            nsid: "io.prismapp.assets.scanned-doc",
            category: assets_categories::CONTENT,
            label: "Scanned Document",
            plural_label: "Scanned Documents",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: None,
            fields: scanned_doc_fields(),
        }),
        entity_def(EntitySpec {
            type_name: assets_types::COLLECTION,
            nsid: "io.prismapp.assets.collection",
            category: assets_categories::COLLECTIONS,
            label: "Collection",
            plural_label: "Collections",
            default_child_view: Some(DefaultChildView::Grid),
            child_only: false,
            extra_child_types: Some(owned_strings([
                assets_types::MEDIA_ASSET,
                assets_types::CONTENT_ITEM,
                assets_types::SCANNED_DOC,
            ])),
            fields: collection_fields(),
        }),
    ]
}

pub fn build_edge_defs() -> Vec<EdgeTypeDef> {
    vec![
        edge_def(EdgeSpec {
            relation: assets_edges::IN_COLLECTION,
            nsid: "io.prismapp.assets.in-collection",
            label: "In Collection",
            behavior: EdgeBehavior::Membership,
            source_types: owned_strings([
                assets_types::MEDIA_ASSET,
                assets_types::CONTENT_ITEM,
                assets_types::SCANNED_DOC,
            ]),
            target_types: Some(owned_strings([assets_types::COLLECTION])),
            description: None,
            suggest_inline: false,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: assets_edges::DERIVED_FROM,
            nsid: "io.prismapp.assets.derived-from",
            label: "Derived From",
            behavior: EdgeBehavior::Weak,
            source_types: owned_strings([assets_types::MEDIA_ASSET]),
            target_types: Some(owned_strings([assets_types::MEDIA_ASSET])),
            description: Some(
                "Links derivative works to their source (e.g. thumbnail, crop, transcode)",
            ),
            suggest_inline: false,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: assets_edges::ATTACHED_TO,
            nsid: "io.prismapp.assets.attached-to",
            label: "Attached To",
            behavior: EdgeBehavior::Weak,
            source_types: owned_strings([assets_types::MEDIA_ASSET, assets_types::SCANNED_DOC]),
            target_types: None,
            description: Some("Attaches a media asset or scan to any object"),
            suggest_inline: true,
            undirected: false,
        }),
    ]
}

// ── Automation presets ──────────────────────────────────────────────────────

pub fn build_automation_presets() -> Vec<super::flux_types::FluxAutomationPreset> {
    use super::flux_types::{
        FluxActionKind, FluxAutomationAction, FluxAutomationPreset, FluxTriggerKind,
    };
    vec![
        FluxAutomationPreset {
            id: "assets:auto:collection-count".into(),
            name: "Update collection item count".into(),
            entity_type: assets_types::COLLECTION.into(),
            trigger: FluxTriggerKind::OnUpdate,
            condition: Some("true".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SetField,
                target: "itemCount".into(),
                value: "{{count(children)}}".into(),
            }],
        },
        FluxAutomationPreset {
            id: "assets:auto:ocr-complete".into(),
            name: "Notify on OCR completion".into(),
            entity_type: assets_types::SCANNED_DOC.into(),
            trigger: FluxTriggerKind::OnStatusChange,
            condition: Some("status == 'completed'".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SendNotification,
                target: "owner".into(),
                value: "Document '{{name}}' OCR complete ({{ocrConfidence}}% confidence)".into(),
            }],
        },
    ]
}

// ── Plugin ──────────────────────────────────────────────────────────────────

fn view(id: &str, label: &str, component_id: &str, description: &str) -> ViewContributionDef {
    ViewContributionDef {
        id: id.into(),
        label: label.into(),
        zone: ViewZone::Content,
        component_id: component_id.into(),
        icon: None,
        default_visible: None,
        description: Some(description.into()),
        tags: None,
    }
}

fn command(id: &str, label: &str, action: &str) -> CommandContributionDef {
    CommandContributionDef {
        id: id.into(),
        label: label.into(),
        category: "Assets".into(),
        shortcut: None,
        description: None,
        action: action.into(),
        payload: None,
        when: None,
    }
}

pub fn build_plugin() -> PrismPlugin {
    PrismPlugin::new(plugin_id("prism.plugin.assets"), "Assets").with_contributes(
        PluginContributions {
            views: Some(vec![
                view(
                    "assets:media",
                    "Media Library",
                    "MediaLibraryView",
                    "Media asset browser",
                ),
                view(
                    "assets:content",
                    "Content Library",
                    "ContentLibraryView",
                    "Content management",
                ),
                view(
                    "assets:scanner",
                    "Document Scanner",
                    "DocumentScannerView",
                    "OCR document scanner",
                ),
                view(
                    "assets:collections",
                    "Collections",
                    "CollectionBrowserView",
                    "Asset collections",
                ),
            ]),
            commands: Some(vec![
                command("assets:import-media", "Import Media", "assets.importMedia"),
                command(
                    "assets:scan-document",
                    "Scan Document",
                    "assets.scanDocument",
                ),
                command(
                    "assets:new-collection",
                    "New Collection",
                    "assets.newCollection",
                ),
            ]),
            activity_bar: Some(vec![ActivityBarContributionDef {
                id: "assets:activity".into(),
                label: "Assets".into(),
                icon: None,
                position: Some(ActivityBarPosition::Top),
                priority: Some(35),
            }]),
            keybindings: None,
            context_menus: None,
            settings: None,
            toolbar: None,
            status_bar: None,
            weak_ref_providers: None,
            immersive: None,
        },
    )
}

pub struct AssetsBundle;

impl PluginBundle for AssetsBundle {
    fn id(&self) -> &str {
        "prism.plugin.assets"
    }

    fn name(&self) -> &str {
        "Assets"
    }

    fn install(&self, ctx: &mut PluginInstallContext<'_>) {
        ctx.object_registry.register_all(build_entity_defs());
        ctx.object_registry.register_edges(build_edge_defs());
        ctx.plugin_registry.register(build_plugin());
    }
}

pub fn create_assets_bundle() -> Box<dyn PluginBundle> {
    Box::new(AssetsBundle)
}
