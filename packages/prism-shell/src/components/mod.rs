//! Shell-only components — toolbar / chrome / overlay primitives that
//! must NOT show up in the user-visible document component palette.
//!
//! Every primitive is declared as a [`prism_builder::BlockSpec`] const
//! and registered into a sibling [`ShellComponentRegistry`] via the
//! `SHELL_BUILTINS` table in `registry.rs`. The strategy and rationale
//! are documented in `docs/dev/clay-migration-plan.md` §12 + §33.

pub mod app_card;
pub mod app_window;
pub mod builder_canvas;
pub mod chrome;
pub mod code_editor;
pub mod command_palette;
pub mod component_palette;
pub mod component_picker;
pub mod context_menu;
pub mod dock_divider;
pub mod dock_panel;
pub mod dock_tab;
pub mod dock_tab_bar;
pub mod dock_workspace;
pub mod docs_content;
pub mod docs_sidebar;
pub mod docs_view;
pub mod drag_number_field;
pub mod explorer;
pub mod field_editor;
pub mod gizmo_move;
pub mod gizmo_rotate;
pub mod gizmo_scale;
pub mod help_tooltip;
pub mod icon_button;
pub mod inspector_row;
pub mod inspector_tree;
pub mod launchpad;
pub mod menu_bar_row;
pub mod menu_dropdown;
pub mod menu_item;
pub mod nav_button;
pub mod nav_graph;
pub mod nav_page_list;
pub mod nav_page_row;
pub mod properties_panel;
pub mod registry;
pub mod resize_handle;
pub mod schema_designer;
pub mod schema_row;
pub mod section_header;
pub mod signal_connection_row;
pub mod signals_panel;
pub mod status_bar;
pub mod toast;
pub mod toast_stack;
pub mod toolbar_separator;
pub mod transform_editor;
pub mod workflow_page_bar;
pub mod workflow_page_button;

pub use registry::{register_shell_builtins, ShellComponentRegistry, SHELL_BUILTINS};
