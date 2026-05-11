//! Compile-time asset bundle — every SVG icon in `ui/icons/` baked
//! into the binary so the [`prism_ui_runtime::images::ImageCache`]
//! has zero runtime filesystem dependency. Adding an icon is one
//! line in [`ICONS`].
//!
//! The host plugs [`loader`] into the femtovg / web backend via
//! `backends::femtovg::run(.., loader())`. Sources without a
//! matching row return `None` and the cache records the negative.
//!
//! See `docs/dev/ui-migration-followups.md` item B1.

use std::sync::Arc;

use prism_ui_runtime::images::AssetLoader;

/// Single source of truth: source string → embedded bytes. Each row
/// uses `include_bytes!` so the linker rejects dead references at
/// compile time — a typo in a filename here fails the build rather
/// than producing a silently-blank icon at runtime.
///
/// The convention is `"icons/<name>.svg"` to match the strings
/// authored in `.prism-ui` components.
const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/arrow-left.svg",
        include_bytes!("../ui/icons/arrow-left.svg"),
    ),
    ("icons/book.svg", include_bytes!("../ui/icons/book.svg")),
    ("icons/box.svg", include_bytes!("../ui/icons/box.svg")),
    (
        "icons/chevron-down.svg",
        include_bytes!("../ui/icons/chevron-down.svg"),
    ),
    (
        "icons/chevron-left.svg",
        include_bytes!("../ui/icons/chevron-left.svg"),
    ),
    (
        "icons/chevron-right.svg",
        include_bytes!("../ui/icons/chevron-right.svg"),
    ),
    (
        "icons/chevron-up.svg",
        include_bytes!("../ui/icons/chevron-up.svg"),
    ),
    ("icons/code.svg", include_bytes!("../ui/icons/code.svg")),
    (
        "icons/columns.svg",
        include_bytes!("../ui/icons/columns.svg"),
    ),
    ("icons/copy.svg", include_bytes!("../ui/icons/copy.svg")),
    (
        "icons/duplicate.svg",
        include_bytes!("../ui/icons/duplicate.svg"),
    ),
    ("icons/eye.svg", include_bytes!("../ui/icons/eye.svg")),
    ("icons/file.svg", include_bytes!("../ui/icons/file.svg")),
    ("icons/folder.svg", include_bytes!("../ui/icons/folder.svg")),
    ("icons/globe.svg", include_bytes!("../ui/icons/globe.svg")),
    ("icons/grip.svg", include_bytes!("../ui/icons/grip.svg")),
    (
        "icons/help-circle.svg",
        include_bytes!("../ui/icons/help-circle.svg"),
    ),
    ("icons/home.svg", include_bytes!("../ui/icons/home.svg")),
    ("icons/image.svg", include_bytes!("../ui/icons/image.svg")),
    ("icons/layout.svg", include_bytes!("../ui/icons/layout.svg")),
    ("icons/link.svg", include_bytes!("../ui/icons/link.svg")),
    ("icons/list.svg", include_bytes!("../ui/icons/list.svg")),
    ("icons/minus.svg", include_bytes!("../ui/icons/minus.svg")),
    ("icons/paste.svg", include_bytes!("../ui/icons/paste.svg")),
    ("icons/plus.svg", include_bytes!("../ui/icons/plus.svg")),
    ("icons/redo.svg", include_bytes!("../ui/icons/redo.svg")),
    (
        "icons/scissors.svg",
        include_bytes!("../ui/icons/scissors.svg"),
    ),
    ("icons/search.svg", include_bytes!("../ui/icons/search.svg")),
    (
        "icons/sidebar-left.svg",
        include_bytes!("../ui/icons/sidebar-left.svg"),
    ),
    (
        "icons/sidebar-right.svg",
        include_bytes!("../ui/icons/sidebar-right.svg"),
    ),
    (
        "icons/sliders.svg",
        include_bytes!("../ui/icons/sliders.svg"),
    ),
    ("icons/table.svg", include_bytes!("../ui/icons/table.svg")),
    ("icons/trash.svg", include_bytes!("../ui/icons/trash.svg")),
    ("icons/tree.svg", include_bytes!("../ui/icons/tree.svg")),
    ("icons/type.svg", include_bytes!("../ui/icons/type.svg")),
    ("icons/undo.svg", include_bytes!("../ui/icons/undo.svg")),
    ("icons/user.svg", include_bytes!("../ui/icons/user.svg")),
    ("icons/x.svg", include_bytes!("../ui/icons/x.svg")),
    ("icons/zap.svg", include_bytes!("../ui/icons/zap.svg")),
];

/// Build an [`AssetLoader`] that looks up `source` in the embedded
/// icon table. Cheap to call repeatedly: the returned closure
/// captures a `'static` reference to [`ICONS`] and the bytes live in
/// the binary's read-only data segment, so resolution is a linear
/// scan over 39 string compares.
pub fn loader() -> AssetLoader {
    Arc::new(|source: &str| -> Option<Vec<u8>> {
        ICONS
            .iter()
            .find(|(k, _)| *k == source)
            .map(|(_, bytes)| bytes.to_vec())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_resolves_every_row_in_the_icon_table() {
        let load = loader();
        for (key, bytes) in ICONS {
            let resolved = load(key).unwrap_or_else(|| panic!("unresolved icon `{key}`"));
            // Sanity-check the embed actually round-tripped — every
            // row should hand back the same bytes the macro stamped.
            assert_eq!(resolved.as_slice(), *bytes);
        }
    }

    #[test]
    fn loader_returns_none_for_unknown_sources() {
        let load = loader();
        assert!(load("icons/not-a-real-icon.svg").is_none());
        assert!(load("").is_none());
        assert!(load("absolute/path/to/elsewhere").is_none());
    }

    #[test]
    fn icon_table_keys_are_unique() {
        // A double-entered row would silently shadow the second copy.
        // Lock the invariant at the table level so cargo test fails
        // before the binary ships.
        let mut seen = std::collections::HashSet::new();
        for (key, _) in ICONS {
            assert!(seen.insert(*key), "duplicate icon key `{key}`");
        }
    }
}
