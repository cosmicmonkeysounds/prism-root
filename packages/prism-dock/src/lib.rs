//! `prism-dock` — renderer-agnostic dockable panel layout engine (ADR-005).
//!
//! Two layers:
//!
//! * **Data model** — [`DockState`], [`DockNode`], [`PanelId`]. A strict
//!   binary tree where internal nodes are [`Split`](DockNode::Split)s
//!   (axis + ratio) and leaves are [`TabGroup`](DockNode::TabGroup)s
//!   (one or more panels, one active). Pure Rust, no UI dependency.
//!
//! * **Presets** — [`WorkflowPage`] bundles a name, icon hint, and a
//!   default `DockState` for task-specific layouts (Edit, Design, Code,
//!   Fusion). Users can override and persist via serde.

pub mod node;
pub mod page;
pub mod panel;
pub mod state;

pub use node::{Axis, DockNode, NodeAddress, SplitPosition};
pub use page::WorkflowPage;
pub use panel::{PanelId, PanelKind, PanelMeta};
pub use state::DockState;
