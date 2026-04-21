//! Cascading style properties for the App → Page → Component hierarchy.
//!
//! Every level can set any subset of style properties. Resolution
//! follows CSS-like cascade order: component > page > app. For each
//! field, the most specific non-`None` value wins.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StyleProperties {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_spacing: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<f32>,
}

impl StyleProperties {
    /// True when the node carries a visual background or border that
    /// needs a wrapper `Rectangle` in the Slint output.
    pub fn has_background_or_border(&self) -> bool {
        self.background.is_some() || self.border_radius.is_some()
    }
}

pub fn resolve_cascade(
    app: &StyleProperties,
    page: &StyleProperties,
    node: &StyleProperties,
) -> StyleProperties {
    StyleProperties {
        font_family: node
            .font_family
            .clone()
            .or_else(|| page.font_family.clone())
            .or_else(|| app.font_family.clone()),
        font_size: node.font_size.or(page.font_size).or(app.font_size),
        font_weight: node.font_weight.or(page.font_weight).or(app.font_weight),
        line_height: node.line_height.or(page.line_height).or(app.line_height),
        letter_spacing: node
            .letter_spacing
            .or(page.letter_spacing)
            .or(app.letter_spacing),
        color: node
            .color
            .clone()
            .or_else(|| page.color.clone())
            .or_else(|| app.color.clone()),
        background: node
            .background
            .clone()
            .or_else(|| page.background.clone())
            .or_else(|| app.background.clone()),
        accent: node
            .accent
            .clone()
            .or_else(|| page.accent.clone())
            .or_else(|| app.accent.clone()),
        base_spacing: node.base_spacing.or(page.base_spacing).or(app.base_spacing),
        border_radius: node
            .border_radius
            .or(page.border_radius)
            .or(app.border_radius),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cascade_is_all_none() {
        let result = resolve_cascade(
            &StyleProperties::default(),
            &StyleProperties::default(),
            &StyleProperties::default(),
        );
        assert_eq!(result, StyleProperties::default());
    }

    #[test]
    fn app_values_flow_through() {
        let app = StyleProperties {
            font_family: Some("Inter".into()),
            font_size: Some(16.0),
            ..Default::default()
        };
        let result = resolve_cascade(
            &app,
            &StyleProperties::default(),
            &StyleProperties::default(),
        );
        assert_eq!(result.font_family.as_deref(), Some("Inter"));
        assert_eq!(result.font_size, Some(16.0));
    }

    #[test]
    fn page_overrides_app() {
        let app = StyleProperties {
            font_family: Some("Inter".into()),
            font_size: Some(16.0),
            color: Some("#000".into()),
            ..Default::default()
        };
        let page = StyleProperties {
            font_size: Some(18.0),
            ..Default::default()
        };
        let result = resolve_cascade(&app, &page, &StyleProperties::default());
        assert_eq!(result.font_family.as_deref(), Some("Inter"));
        assert_eq!(result.font_size, Some(18.0));
        assert_eq!(result.color.as_deref(), Some("#000"));
    }

    #[test]
    fn node_overrides_page_and_app() {
        let app = StyleProperties {
            font_family: Some("Inter".into()),
            font_size: Some(16.0),
            color: Some("#000".into()),
            ..Default::default()
        };
        let page = StyleProperties {
            font_size: Some(18.0),
            background: Some("#fff".into()),
            ..Default::default()
        };
        let node = StyleProperties {
            font_size: Some(24.0),
            accent: Some("#f00".into()),
            ..Default::default()
        };
        let result = resolve_cascade(&app, &page, &node);
        assert_eq!(result.font_family.as_deref(), Some("Inter"));
        assert_eq!(result.font_size, Some(24.0));
        assert_eq!(result.color.as_deref(), Some("#000"));
        assert_eq!(result.background.as_deref(), Some("#fff"));
        assert_eq!(result.accent.as_deref(), Some("#f00"));
    }

    #[test]
    fn serde_roundtrip() {
        let style = StyleProperties {
            font_family: Some("Fira Code".into()),
            font_size: Some(14.0),
            font_weight: Some(600),
            color: Some("#d8dee9".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&style).unwrap();
        let parsed: StyleProperties = serde_json::from_str(&json).unwrap();
        assert_eq!(style, parsed);
    }

    #[test]
    fn empty_json_deserializes_to_default() {
        let parsed: StyleProperties = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, StyleProperties::default());
    }
}
