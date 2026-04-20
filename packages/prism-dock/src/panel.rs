use serde::{Deserialize, Serialize};

pub type PanelId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelKind {
    Builder,
    Inspector,
    Properties,
    Explorer,
    CodeEditor,
    Identity,
    Timeline,
    NodeGraph,
    AssetBrowser,
    ComponentPalette,
    Console,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelMeta {
    pub kind: PanelKind,
    pub label: &'static str,
    pub icon_hint: &'static str,
    pub min_width: f32,
    pub min_height: f32,
    pub allow_multiple: bool,
}

impl PanelKind {
    pub fn meta(self) -> PanelMeta {
        match self {
            Self::Builder => PanelMeta {
                kind: self,
                label: "Builder",
                icon_hint: "builder",
                min_width: 200.0,
                min_height: 100.0,
                allow_multiple: false,
            },
            Self::Inspector => PanelMeta {
                kind: self,
                label: "Inspector",
                icon_hint: "inspector",
                min_width: 180.0,
                min_height: 100.0,
                allow_multiple: false,
            },
            Self::Properties => PanelMeta {
                kind: self,
                label: "Properties",
                icon_hint: "properties",
                min_width: 180.0,
                min_height: 100.0,
                allow_multiple: false,
            },
            Self::Explorer => PanelMeta {
                kind: self,
                label: "Explorer",
                icon_hint: "explorer",
                min_width: 160.0,
                min_height: 100.0,
                allow_multiple: false,
            },
            Self::CodeEditor => PanelMeta {
                kind: self,
                label: "Code Editor",
                icon_hint: "code",
                min_width: 200.0,
                min_height: 100.0,
                allow_multiple: true,
            },
            Self::Identity => PanelMeta {
                kind: self,
                label: "Identity",
                icon_hint: "identity",
                min_width: 160.0,
                min_height: 100.0,
                allow_multiple: false,
            },
            Self::Timeline => PanelMeta {
                kind: self,
                label: "Timeline",
                icon_hint: "timeline",
                min_width: 300.0,
                min_height: 80.0,
                allow_multiple: false,
            },
            Self::NodeGraph => PanelMeta {
                kind: self,
                label: "Node Graph",
                icon_hint: "node-graph",
                min_width: 200.0,
                min_height: 200.0,
                allow_multiple: false,
            },
            Self::AssetBrowser => PanelMeta {
                kind: self,
                label: "Asset Browser",
                icon_hint: "assets",
                min_width: 200.0,
                min_height: 100.0,
                allow_multiple: false,
            },
            Self::ComponentPalette => PanelMeta {
                kind: self,
                label: "Components",
                icon_hint: "palette",
                min_width: 160.0,
                min_height: 100.0,
                allow_multiple: false,
            },
            Self::Console => PanelMeta {
                kind: self,
                label: "Console",
                icon_hint: "console",
                min_width: 200.0,
                min_height: 60.0,
                allow_multiple: false,
            },
        }
    }

    pub fn id(self) -> PanelId {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{self:?}").to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_kind_id_is_kebab() {
        assert_eq!(PanelKind::CodeEditor.id(), "code-editor");
        assert_eq!(PanelKind::NodeGraph.id(), "node-graph");
        assert_eq!(PanelKind::Builder.id(), "builder");
    }

    #[test]
    fn panel_meta_roundtrip() {
        let meta = PanelKind::Timeline.meta();
        assert_eq!(meta.label, "Timeline");
        assert!(!meta.allow_multiple);
    }

    #[test]
    fn all_kinds_have_meta() {
        let kinds = [
            PanelKind::Builder,
            PanelKind::Inspector,
            PanelKind::Properties,
            PanelKind::Explorer,
            PanelKind::CodeEditor,
            PanelKind::Identity,
            PanelKind::Timeline,
            PanelKind::NodeGraph,
            PanelKind::AssetBrowser,
            PanelKind::ComponentPalette,
            PanelKind::Console,
        ];
        for k in kinds {
            let meta = k.meta();
            assert!(!meta.label.is_empty());
            assert!(meta.min_width > 0.0);
        }
    }
}
