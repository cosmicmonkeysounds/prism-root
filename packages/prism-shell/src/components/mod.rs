//! Shell-only components — toolbar / chrome / overlay primitives that
//! must NOT show up in the user-visible document component palette.
//!
//! Every primitive is a [`prism_builder::Block`] impl, registered into a
//! sibling [`ShellComponentRegistry`]. The strategy and rationale are
//! documented in `docs/dev/clay-migration-plan.md` §12.

pub mod app_card;
pub mod app_window;
pub mod chrome;
pub mod command_palette;
pub mod component_palette;
pub mod context_menu;
pub mod dock_divider;
pub mod dock_panel;
pub mod dock_tab;
pub mod dock_tab_bar;
pub mod docs_content;
pub mod docs_sidebar;
pub mod docs_view;
pub mod drag_number_field;
pub mod explorer;
pub mod field_editor;
pub mod help_tooltip;
pub mod icon_button;
pub mod inspector_row;
pub mod inspector_tree;
pub mod launchpad;
pub mod menu_bar_row;
pub mod menu_dropdown;
pub mod menu_item;
pub mod nav_button;
pub mod properties_panel;
pub mod registry;
pub mod section_header;
pub mod status_bar;
pub mod toast;
pub mod toast_stack;
pub mod toolbar_separator;
pub mod transform_editor;
pub mod workflow_page_bar;
pub mod workflow_page_button;

pub use app_card::AppCard;
pub use app_window::AppWindow;
pub use command_palette::CommandPalette;
pub use component_palette::ComponentPalette;
pub use context_menu::ContextMenu;
pub use dock_divider::DockDivider;
pub use dock_panel::DockPanel;
pub use dock_tab::DockTab;
pub use dock_tab_bar::DockTabBar;
pub use docs_content::DocsContent;
pub use docs_sidebar::DocsSidebar;
pub use docs_view::DocsView;
pub use drag_number_field::DragNumberField;
pub use explorer::Explorer;
pub use field_editor::FieldEditor;
pub use help_tooltip::HelpTooltip;
pub use icon_button::IconButton;
pub use inspector_row::InspectorRow;
pub use inspector_tree::InspectorTree;
pub use launchpad::Launchpad;
pub use menu_bar_row::MenuBarRow;
pub use menu_dropdown::MenuDropdown;
pub use menu_item::MenuItem;
pub use nav_button::NavButton;
pub use properties_panel::PropertiesPanel;
pub use registry::{register_shell_builtins, ShellComponentRegistry};
pub use section_header::SectionHeader;
pub use status_bar::StatusBar;
pub use toast::Toast;
pub use toast_stack::ToastStack;
pub use toolbar_separator::ToolbarSeparator;
pub use transform_editor::TransformEditor;
pub use workflow_page_bar::WorkflowPageBar;
pub use workflow_page_button::WorkflowPageButton;
