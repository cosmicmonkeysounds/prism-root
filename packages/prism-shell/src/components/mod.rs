//! Shell-only components — toolbar / chrome / overlay primitives that
//! must NOT show up in the user-visible document component palette.
//!
//! Every primitive is a [`prism_builder::Block`] impl, registered into a
//! sibling [`ShellComponentRegistry`]. The strategy and rationale are
//! documented in `docs/dev/clay-migration-plan.md` §12.

pub mod icon_button;
pub mod registry;
pub mod section_header;
pub mod toolbar_separator;

pub use icon_button::IconButton;
pub use registry::{register_shell_builtins, ShellComponentRegistry};
pub use section_header::SectionHeader;
pub use toolbar_separator::ToolbarSeparator;
