//! Asset source resolution for builder components.
//!
//! Components that reference binary assets (images, video, audio) store
//! their source in `Node.props` as either a plain URL string or a VFS
//! `BinaryRef`-shaped object. [`AssetSource`] unifies both
//! representations so render paths don't juggle raw JSON.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where a component's asset comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AssetSource {
    /// External URL (backward-compatible with plain string props).
    Url { url: String },
    /// Content-addressed VFS reference (from [`prism_core::foundation::vfs`]).
    Vfs {
        hash: String,
        filename: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        size: usize,
    },
}

impl AssetSource {
    /// Parse from a node prop value. Strings become URLs; objects with
    /// a `hash` field become VFS references.
    pub fn from_prop(val: &Value) -> Option<Self> {
        match val {
            Value::String(s) if !s.is_empty() => {
                if s.starts_with('{') {
                    if let Ok(obj) = serde_json::from_str::<Value>(s) {
                        return Self::from_prop(&obj);
                    }
                }
                Some(AssetSource::Url { url: s.clone() })
            }
            Value::Object(map) => {
                let hash = map.get("hash").and_then(|v| v.as_str())?;
                Some(AssetSource::Vfs {
                    hash: hash.to_string(),
                    filename: map
                        .get("filename")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    mime_type: map
                        .get("mimeType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("application/octet-stream")
                        .to_string(),
                    size: map.get("size").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                })
            }
            _ => None,
        }
    }

    /// Convert to a `Value` suitable for storing in `Node.props`.
    pub fn to_prop(&self) -> Value {
        match self {
            AssetSource::Url { url } => Value::String(url.clone()),
            AssetSource::Vfs {
                hash,
                filename,
                mime_type,
                size,
            } => serde_json::json!({
                "hash": hash,
                "filename": filename,
                "mimeType": mime_type,
                "size": size,
            }),
        }
    }

    /// HTML-ready `src` attribute value. VFS refs resolve to
    /// `/asset/{hash}`; URLs pass through.
    pub fn to_html_src(&self) -> String {
        match self {
            AssetSource::Url { url } => url.clone(),
            AssetSource::Vfs { hash, .. } => format!("/asset/{hash}"),
        }
    }

    /// Display name for builder UI (filename for VFS, URL for external).
    pub fn display_name(&self) -> &str {
        match self {
            AssetSource::Url { url } => url,
            AssetSource::Vfs { filename, .. } => filename,
        }
    }

    pub fn is_vfs(&self) -> bool {
        matches!(self, AssetSource::Vfs { .. })
    }
}

/// Collect all VFS asset hashes referenced by nodes in a document tree.
pub fn collect_vfs_hashes(node: &crate::document::Node) -> Vec<String> {
    let mut hashes = Vec::new();
    collect_hashes_recursive(node, &mut hashes);
    hashes
}

fn collect_hashes_recursive(node: &crate::document::Node, hashes: &mut Vec<String>) {
    if let Some(props) = node.props.as_object() {
        for val in props.values() {
            if let Some(AssetSource::Vfs { ref hash, .. }) = AssetSource::from_prop(val) {
                if !hashes.contains(hash) {
                    hashes.push(hash.clone());
                }
            }
        }
    }
    for child in &node.children {
        collect_hashes_recursive(child, hashes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_url_string() {
        let val = json!("https://example.com/img.png");
        let src = AssetSource::from_prop(&val).unwrap();
        assert!(matches!(&src, AssetSource::Url { url } if url == "https://example.com/img.png"));
        assert_eq!(src.to_html_src(), "https://example.com/img.png");
        assert!(!src.is_vfs());
    }

    #[test]
    fn parse_vfs_object() {
        let val = json!({
            "hash": "abc123",
            "filename": "photo.png",
            "mimeType": "image/png",
            "size": 1234
        });
        let src = AssetSource::from_prop(&val).unwrap();
        assert!(matches!(&src, AssetSource::Vfs { hash, .. } if hash == "abc123"));
        assert_eq!(src.to_html_src(), "/asset/abc123");
        assert_eq!(src.display_name(), "photo.png");
        assert!(src.is_vfs());
    }

    #[test]
    fn empty_string_returns_none() {
        assert!(AssetSource::from_prop(&json!("")).is_none());
    }

    #[test]
    fn null_returns_none() {
        assert!(AssetSource::from_prop(&Value::Null).is_none());
    }

    #[test]
    fn object_without_hash_returns_none() {
        let val = json!({ "filename": "test.png" });
        assert!(AssetSource::from_prop(&val).is_none());
    }

    #[test]
    fn to_prop_round_trips_url() {
        let src = AssetSource::Url {
            url: "https://x.com/a.png".into(),
        };
        let val = src.to_prop();
        let back = AssetSource::from_prop(&val).unwrap();
        assert_eq!(src, back);
    }

    #[test]
    fn to_prop_round_trips_vfs() {
        let src = AssetSource::Vfs {
            hash: "deadbeef".into(),
            filename: "test.png".into(),
            mime_type: "image/png".into(),
            size: 42,
        };
        let val = src.to_prop();
        let back = AssetSource::from_prop(&val).unwrap();
        assert_eq!(src, back);
    }

    #[test]
    fn collect_hashes_from_tree() {
        use crate::document::Node;
        let tree = Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({}),
            children: vec![
                Node {
                    id: "img1".into(),
                    component: "image".into(),
                    props: json!({
                        "src": { "hash": "aaa", "filename": "a.png", "mimeType": "image/png", "size": 100 }
                    }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "img2".into(),
                    component: "image".into(),
                    props: json!({ "src": "https://example.com/b.png" }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "img3".into(),
                    component: "image".into(),
                    props: json!({
                        "src": { "hash": "bbb", "filename": "c.png", "mimeType": "image/jpeg", "size": 200 }
                    }),
                    children: vec![],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let hashes = collect_vfs_hashes(&tree);
        assert_eq!(hashes, vec!["aaa", "bbb"]);
    }

    #[test]
    fn collect_deduplicates_same_hash() {
        use crate::document::Node;
        let tree = Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({}),
            children: vec![
                Node {
                    id: "a".into(),
                    component: "image".into(),
                    props: json!({
                        "src": { "hash": "same", "filename": "a.png", "mimeType": "image/png", "size": 1 }
                    }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "b".into(),
                    component: "image".into(),
                    props: json!({
                        "src": { "hash": "same", "filename": "b.png", "mimeType": "image/png", "size": 1 }
                    }),
                    children: vec![],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(collect_vfs_hashes(&tree), vec!["same"]);
    }

    #[test]
    fn parse_json_encoded_string_as_vfs() {
        let json_str =
            r#"{"hash":"abc123","filename":"photo.png","mimeType":"image/png","size":1234}"#;
        let val = Value::String(json_str.to_string());
        let src = AssetSource::from_prop(&val).unwrap();
        assert!(matches!(&src, AssetSource::Vfs { hash, .. } if hash == "abc123"));
        assert_eq!(src.display_name(), "photo.png");
    }

    #[test]
    fn asset_source_serde_round_trip() {
        let vfs = AssetSource::Vfs {
            hash: "abc".into(),
            filename: "test.png".into(),
            mime_type: "image/png".into(),
            size: 42,
        };
        let json = serde_json::to_string(&vfs).unwrap();
        let back: AssetSource = serde_json::from_str(&json).unwrap();
        assert_eq!(vfs, back);

        let url = AssetSource::Url {
            url: "https://x.com".into(),
        };
        let json = serde_json::to_string(&url).unwrap();
        let back: AssetSource = serde_json::from_str(&json).unwrap();
        assert_eq!(url, back);
    }
}
