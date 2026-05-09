//! Shell-only components — toolbar / chrome / overlay primitives that
//! must NOT show up in the user-visible document component palette.
//!
//! Every primitive is a [`prism_builder::Block`] impl, registered into a
//! sibling [`ShellComponentRegistry`]. The strategy and rationale are
//! documented in `docs/dev/clay-migration-plan.md` §12.

pub mod app_card;
pub mod app_window;
pub mod chrome;
pub mod docs_content;
pub mod drag_number_field;
pub mod field_editor;
pub mod icon_button;
pub mod inspector_row;
pub mod menu_bar_row;
pub mod nav_button;
pub mod registry;
pub mod section_header;
pub mod toast;
pub mod toolbar_separator;
pub mod transform_editor;

pub use app_card::AppCard;
pub use app_window::AppWindow;
pub use docs_content::DocsContent;
pub use drag_number_field::DragNumberField;
pub use field_editor::FieldEditor;
pub use icon_button::IconButton;
pub use inspector_row::InspectorRow;
pub use menu_bar_row::MenuBarRow;
pub use nav_button::NavButton;
pub use registry::{register_shell_builtins, ShellComponentRegistry};
pub use section_header::SectionHeader;
pub use toast::Toast;
pub use toolbar_separator::ToolbarSeparator;
pub use transform_editor::TransformEditor;
