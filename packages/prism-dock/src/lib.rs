//! `prism-dock` — renderer-agnostic dockable panel layout engine (ADR-005).
//!
//! Three layers:
//!
//! * **Data model** — [`DockState`], [`DockNode`], [`PanelId`]. A strict
//!   binary tree where internal nodes are [`Split`](DockNode::Split)s
//!   (axis + ratio) and leaves are [`TabGroup`](DockNode::TabGroup)s
//!   (one or more panels, one active). [`MoveTarget`] enables DaVinci/
//!   Unity-style drag to tab group or split edge. Pure Rust, no UI dep.
//!
//! * **Layout & hit-testing** — [`compute_layout`] resolves the tree into
//!   pixel [`Rect`]s. [`compute_drop_zones`] produces five [`DropZone`]s
//!   per tab group (center + four edges) for drag-and-drop targeting.
//!   [`constrain_ratio`] enforces panel minimum sizes during resize.
//!
//! * **Workspace** — [`DockWorkspace`] manages [`WorkflowPage`] presets
//!   (Edit, Design, Code, Fusion) with per-page customization, cross-page
//!   panel navigation, and serializable state.

pub mod drop_zone;
pub mod layout;
pub mod node;
pub mod page;
pub mod panel;
pub mod state;
pub mod workspace;

pub use drop_zone::{compute_drop_zones, hit_test_drop_zone, DropZone};
pub use layout::{
    compute_layout, constrain_ratio, find_divider_at, find_tab_group_at, LayoutNodeKind,
    LayoutRect, Rect,
};
pub use node::{Axis, DockNode, MoveTarget, NodeAddress, SplitPosition};
pub use page::WorkflowPage;
pub use panel::{PanelId, PanelKind, PanelMeta};
pub use state::DockState;
pub use workspace::DockWorkspace;
