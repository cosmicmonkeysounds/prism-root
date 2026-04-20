use serde::{Deserialize, Serialize};

use crate::panel::PanelId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitPosition {
    Before,
    After,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveTarget {
    TabGroup(NodeAddress),
    SplitEdge {
        addr: NodeAddress,
        axis: Axis,
        position: SplitPosition,
    },
}

/// Address of a node in the binary tree. Empty = root, then each
/// `false` = first child, `true` = second child.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeAddress(pub Vec<bool>);

impl NodeAddress {
    pub fn root() -> Self {
        Self(Vec::new())
    }

    pub fn first(&self) -> Self {
        let mut v = self.0.clone();
        v.push(false);
        Self(v)
    }

    pub fn second(&self) -> Self {
        let mut v = self.0.clone();
        v.push(true);
        Self(v)
    }

    pub fn parent(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            let mut v = self.0.clone();
            v.pop();
            Some(Self(v))
        }
    }

    pub fn is_second_child(&self) -> Option<bool> {
        self.0.last().copied()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum DockNode {
    Split {
        axis: Axis,
        ratio: f32,
        first: Box<DockNode>,
        second: Box<DockNode>,
    },
    TabGroup {
        tabs: Vec<PanelId>,
        active: usize,
    },
}

impl DockNode {
    pub fn tab(panel: PanelId) -> Self {
        Self::TabGroup {
            tabs: vec![panel],
            active: 0,
        }
    }

    pub fn tabs(panels: Vec<PanelId>) -> Self {
        Self::TabGroup {
            tabs: panels,
            active: 0,
        }
    }

    pub fn hsplit(ratio: f32, first: DockNode, second: DockNode) -> Self {
        Self::Split {
            axis: Axis::Horizontal,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    pub fn vsplit(ratio: f32, first: DockNode, second: DockNode) -> Self {
        Self::Split {
            axis: Axis::Vertical,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    pub fn is_tab_group(&self) -> bool {
        matches!(self, Self::TabGroup { .. })
    }

    pub fn is_split(&self) -> bool {
        matches!(self, Self::Split { .. })
    }

    pub fn panel_ids(&self) -> Vec<&PanelId> {
        match self {
            Self::TabGroup { tabs, .. } => tabs.iter().collect(),
            Self::Split { first, second, .. } => {
                let mut ids = first.panel_ids();
                ids.extend(second.panel_ids());
                ids
            }
        }
    }

    pub fn contains_panel(&self, panel_id: &str) -> bool {
        match self {
            Self::TabGroup { tabs, .. } => tabs.iter().any(|t| t == panel_id),
            Self::Split { first, second, .. } => {
                first.contains_panel(panel_id) || second.contains_panel(panel_id)
            }
        }
    }

    pub fn find_panel(&self, panel_id: &str) -> Option<NodeAddress> {
        self.find_panel_inner(panel_id, &NodeAddress::root())
    }

    fn find_panel_inner(&self, panel_id: &str, addr: &NodeAddress) -> Option<NodeAddress> {
        match self {
            Self::TabGroup { tabs, .. } => {
                if tabs.iter().any(|t| t == panel_id) {
                    Some(addr.clone())
                } else {
                    None
                }
            }
            Self::Split { first, second, .. } => first
                .find_panel_inner(panel_id, &addr.first())
                .or_else(|| second.find_panel_inner(panel_id, &addr.second())),
        }
    }

    pub fn node_at(&self, addr: &NodeAddress) -> Option<&DockNode> {
        let mut current = self;
        for &step in &addr.0 {
            match current {
                Self::Split { first, second, .. } => {
                    current = if step { second } else { first };
                }
                Self::TabGroup { .. } => return None,
            }
        }
        Some(current)
    }

    pub fn node_at_mut(&mut self, addr: &NodeAddress) -> Option<&mut DockNode> {
        let mut current = self;
        for &step in &addr.0 {
            match current {
                Self::Split { first, second, .. } => {
                    current = if step { second } else { first };
                }
                Self::TabGroup { .. } => return None,
            }
        }
        Some(current)
    }

    pub fn activate_tab(&mut self, addr: &NodeAddress, index: usize) -> bool {
        if let Some(Self::TabGroup { tabs, active }) = self.node_at_mut(addr) {
            if index < tabs.len() {
                *active = index;
                return true;
            }
        }
        false
    }

    pub fn set_ratio(&mut self, addr: &NodeAddress, new_ratio: f32) -> bool {
        let clamped = new_ratio.clamp(0.05, 0.95);
        if let Some(Self::Split { ratio, .. }) = self.node_at_mut(addr) {
            *ratio = clamped;
            return true;
        }
        false
    }

    pub fn add_tab(&mut self, addr: &NodeAddress, panel: PanelId) -> bool {
        if let Some(Self::TabGroup { tabs, active }) = self.node_at_mut(addr) {
            tabs.push(panel);
            *active = tabs.len() - 1;
            return true;
        }
        false
    }

    pub fn close_tab(&mut self, addr: &NodeAddress, tab_index: usize) -> Option<PanelId> {
        if let Some(Self::TabGroup { tabs, active }) = self.node_at_mut(addr) {
            if tab_index < tabs.len() && tabs.len() > 1 {
                let removed = tabs.remove(tab_index);
                if *active >= tabs.len() {
                    *active = tabs.len() - 1;
                }
                return Some(removed);
            }
        }
        None
    }

    /// Split a tab group at `addr`, placing `panel` before or after along `axis`.
    pub fn split_at(
        &mut self,
        addr: &NodeAddress,
        axis: Axis,
        panel: PanelId,
        position: SplitPosition,
    ) -> bool {
        if let Some(node) = self.node_at_mut(addr) {
            if !node.is_tab_group() {
                return false;
            }
            let existing = std::mem::replace(node, DockNode::tab(String::new()));
            let new_tab = DockNode::tab(panel);
            let (first, second) = match position {
                SplitPosition::Before => (new_tab, existing),
                SplitPosition::After => (existing, new_tab),
            };
            *node = DockNode::Split {
                axis,
                ratio: 0.5,
                first: Box::new(first),
                second: Box::new(second),
            };
            true
        } else {
            false
        }
    }

    /// Remove a tab group that has been emptied (or after `close_tab`
    /// leaves it with 0 tabs). Collapses the parent split so the
    /// sibling takes its place.
    pub fn collapse_at(&mut self, addr: &NodeAddress) -> bool {
        if addr.0.is_empty() {
            return false;
        }
        let parent_addr = addr.parent().unwrap();
        let is_second = addr.is_second_child().unwrap();

        if let Some(parent) = self.node_at_mut(&parent_addr) {
            if let DockNode::Split { first, second, .. } = parent {
                let survivor = if is_second {
                    *first.clone()
                } else {
                    *second.clone()
                };
                *parent = survivor;
                return true;
            }
        }
        false
    }

    /// Remove a panel from wherever it is in the tree. If the tab group
    /// becomes empty, collapse the parent split. Returns the removed
    /// panel ID if found.
    pub fn remove_panel(&mut self, panel_id: &str) -> Option<PanelId> {
        let addr = self.find_panel(panel_id)?;
        let node = self.node_at_mut(&addr)?;
        if let DockNode::TabGroup { tabs, active } = node {
            let idx = tabs.iter().position(|t| t == panel_id)?;
            if tabs.len() > 1 {
                let removed = tabs.remove(idx);
                if *active >= tabs.len() {
                    *active = tabs.len() - 1;
                }
                return Some(removed);
            }
            // Last tab in the group — need to collapse the parent split.
            let removed = tabs[0].clone();
            if addr.0.is_empty() {
                return None;
            }
            self.collapse_at(&addr);
            return Some(removed);
        }
        None
    }

    /// Move a panel to a target: either into an existing tab group or
    /// to the edge of a tab group (creating a new split).
    pub fn move_panel(&mut self, panel_id: &str, target: &MoveTarget) -> bool {
        let anchor_panel = match target {
            MoveTarget::TabGroup(addr) | MoveTarget::SplitEdge { addr, .. } => {
                self.node_at(addr).and_then(|n| match n {
                    DockNode::TabGroup { tabs, .. } => tabs.first().cloned(),
                    _ => None,
                })
            }
        };

        let Some(removed) = self.remove_panel(panel_id) else {
            return false;
        };

        let resolved = anchor_panel
            .as_deref()
            .and_then(|id| self.find_panel(id))
            .unwrap_or_else(|| match target {
                MoveTarget::TabGroup(a) | MoveTarget::SplitEdge { addr: a, .. } => a.clone(),
            });

        match target {
            MoveTarget::TabGroup(_) => self.add_tab(&resolved, removed),
            MoveTarget::SplitEdge { axis, position, .. } => {
                self.split_at(&resolved, *axis, removed, *position)
            }
        }
    }

    pub fn tab_count(&self) -> usize {
        match self {
            Self::TabGroup { tabs, .. } => tabs.len(),
            Self::Split { first, second, .. } => first.tab_count() + second.tab_count(),
        }
    }

    pub fn leaf_count(&self) -> usize {
        match self {
            Self::TabGroup { .. } => 1,
            Self::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> DockNode {
        // [Explorer | [Builder | Inspector]]
        DockNode::hsplit(
            0.25,
            DockNode::tab("explorer".into()),
            DockNode::hsplit(
                0.7,
                DockNode::tab("builder".into()),
                DockNode::tabs(vec!["inspector".into(), "properties".into()]),
            ),
        )
    }

    #[test]
    fn panel_ids() {
        let tree = sample_tree();
        let ids = tree.panel_ids();
        assert_eq!(ids.len(), 4);
        assert!(ids.iter().any(|p| p.as_str() == "builder"));
    }

    #[test]
    fn find_panel() {
        let tree = sample_tree();
        let addr = tree.find_panel("inspector").unwrap();
        // root -> second -> second
        assert_eq!(addr, NodeAddress(vec![true, true]));
    }

    #[test]
    fn find_panel_not_found() {
        let tree = sample_tree();
        assert!(tree.find_panel("timeline").is_none());
    }

    #[test]
    fn node_at() {
        let tree = sample_tree();
        let node = tree.node_at(&NodeAddress(vec![false])).unwrap();
        assert!(node.is_tab_group());
        if let DockNode::TabGroup { tabs, .. } = node {
            assert_eq!(tabs[0], "explorer");
        }
    }

    #[test]
    fn activate_tab() {
        let mut tree = sample_tree();
        let addr = NodeAddress(vec![true, true]); // inspector+properties
        assert!(tree.activate_tab(&addr, 1));
        if let Some(DockNode::TabGroup { active, .. }) = tree.node_at(&addr) {
            assert_eq!(*active, 1);
        }
    }

    #[test]
    fn activate_tab_out_of_bounds() {
        let mut tree = sample_tree();
        let addr = NodeAddress(vec![true, true]);
        assert!(!tree.activate_tab(&addr, 5));
    }

    #[test]
    fn set_ratio() {
        let mut tree = sample_tree();
        let addr = NodeAddress::root();
        assert!(tree.set_ratio(&addr, 0.4));
        if let DockNode::Split { ratio, .. } = &tree {
            assert!((ratio - 0.4).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn set_ratio_clamped() {
        let mut tree = sample_tree();
        assert!(tree.set_ratio(&NodeAddress::root(), -0.5));
        if let DockNode::Split { ratio, .. } = &tree {
            assert!((ratio - 0.05).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn add_tab() {
        let mut tree = sample_tree();
        let addr = NodeAddress(vec![true, true]);
        assert!(tree.add_tab(&addr, "timeline".into()));
        if let Some(DockNode::TabGroup { tabs, active }) = tree.node_at(&addr) {
            assert_eq!(tabs.len(), 3);
            assert_eq!(tabs[2], "timeline");
            assert_eq!(*active, 2);
        }
    }

    #[test]
    fn close_tab() {
        let mut tree = sample_tree();
        let addr = NodeAddress(vec![true, true]);
        let removed = tree.close_tab(&addr, 0);
        assert_eq!(removed.as_deref(), Some("inspector"));
        if let Some(DockNode::TabGroup { tabs, .. }) = tree.node_at(&addr) {
            assert_eq!(tabs.len(), 1);
            assert_eq!(tabs[0], "properties");
        }
    }

    #[test]
    fn close_last_tab_fails() {
        let mut tree = sample_tree();
        let addr = NodeAddress(vec![false]); // explorer (single tab)
        assert!(tree.close_tab(&addr, 0).is_none());
    }

    #[test]
    fn split_at() {
        let mut tree = sample_tree();
        let addr = NodeAddress(vec![false]); // explorer
        assert!(tree.split_at(
            &addr,
            Axis::Vertical,
            "console".into(),
            SplitPosition::After
        ));
        let node = tree.node_at(&addr).unwrap();
        assert!(node.is_split());
        if let DockNode::Split {
            axis,
            first,
            second,
            ..
        } = node
        {
            assert_eq!(*axis, Axis::Vertical);
            assert!(first.contains_panel("explorer"));
            assert!(second.contains_panel("console"));
        }
    }

    #[test]
    fn collapse_at() {
        let mut tree = DockNode::hsplit(0.5, DockNode::tab("a".into()), DockNode::tab("b".into()));
        // Collapse the second child — "a" should become the root.
        assert!(tree.collapse_at(&NodeAddress(vec![true])));
        assert!(tree.is_tab_group());
        assert!(tree.contains_panel("a"));
        assert!(!tree.contains_panel("b"));
    }

    #[test]
    fn remove_panel_from_multi_tab() {
        let mut tree = sample_tree();
        let removed = tree.remove_panel("inspector");
        assert_eq!(removed.as_deref(), Some("inspector"));
        assert!(!tree.contains_panel("inspector"));
        assert!(tree.contains_panel("properties"));
    }

    #[test]
    fn remove_panel_collapses_parent() {
        let mut tree = DockNode::hsplit(0.5, DockNode::tab("a".into()), DockNode::tab("b".into()));
        let removed = tree.remove_panel("b");
        assert_eq!(removed.as_deref(), Some("b"));
        assert!(tree.is_tab_group());
        assert!(tree.contains_panel("a"));
    }

    #[test]
    fn tab_count() {
        let tree = sample_tree();
        assert_eq!(tree.tab_count(), 4);
    }

    #[test]
    fn leaf_count() {
        let tree = sample_tree();
        assert_eq!(tree.leaf_count(), 3);
    }

    #[test]
    fn serde_roundtrip() {
        let tree = sample_tree();
        let json = serde_json::to_string_pretty(&tree).unwrap();
        let tree2: DockNode = serde_json::from_str(&json).unwrap();
        assert_eq!(tree.tab_count(), tree2.tab_count());
        assert_eq!(tree.leaf_count(), tree2.leaf_count());
        assert!(tree2.contains_panel("builder"));
        assert!(tree2.contains_panel("inspector"));
    }

    #[test]
    fn node_address_navigation() {
        let addr = NodeAddress::root();
        assert!(addr.parent().is_none());
        let child = addr.first();
        assert_eq!(child.0, vec![false]);
        let grandchild = child.second();
        assert_eq!(grandchild.0, vec![false, true]);
        let back = grandchild.parent().unwrap();
        assert_eq!(back, child);
    }

    #[test]
    fn move_panel_to_tab_group() {
        let mut tree = sample_tree();
        let target = MoveTarget::TabGroup(NodeAddress(vec![false]));
        assert!(tree.move_panel("inspector", &target));
        assert!(tree.contains_panel("inspector"));
        let addr = tree.find_panel("inspector").unwrap();
        let node = tree.node_at(&addr).unwrap();
        if let DockNode::TabGroup { tabs, .. } = node {
            assert!(tabs.contains(&"inspector".to_string()));
            assert!(tabs.contains(&"explorer".to_string()));
        }
    }

    #[test]
    fn move_panel_to_split_edge() {
        let mut tree = DockNode::hsplit(
            0.5,
            DockNode::tab("a".into()),
            DockNode::tabs(vec!["b".into(), "c".into()]),
        );
        let target = MoveTarget::SplitEdge {
            addr: NodeAddress(vec![false]),
            axis: Axis::Vertical,
            position: SplitPosition::After,
        };
        assert!(tree.move_panel("c", &target));
        assert!(tree.contains_panel("c"));
        // "a" should now be a vsplit with c below it
        let left = tree.node_at(&NodeAddress(vec![false])).unwrap();
        assert!(left.is_split());
        if let DockNode::Split { axis, .. } = left {
            assert_eq!(*axis, Axis::Vertical);
        }
    }

    #[test]
    fn move_panel_nonexistent_fails() {
        let mut tree = sample_tree();
        let target = MoveTarget::TabGroup(NodeAddress(vec![false]));
        assert!(!tree.move_panel("nonexistent", &target));
    }

    #[test]
    fn move_panel_preserves_tree_integrity() {
        let mut tree = DockNode::hsplit(
            0.3,
            DockNode::tab("explorer".into()),
            DockNode::hsplit(
                0.7,
                DockNode::tab("builder".into()),
                DockNode::tab("inspector".into()),
            ),
        );
        let original_count = tree.tab_count();
        let target = MoveTarget::TabGroup(NodeAddress(vec![true, false]));
        assert!(tree.move_panel("explorer", &target));
        assert_eq!(tree.tab_count(), original_count);
        assert!(tree.contains_panel("explorer"));
        assert!(tree.contains_panel("builder"));
        assert!(tree.contains_panel("inspector"));
    }

    #[test]
    fn partial_eq_works() {
        let a = DockNode::tab("builder".into());
        let b = DockNode::tab("builder".into());
        assert_eq!(a, b);
        let c = DockNode::tab("inspector".into());
        assert_ne!(a, c);
    }
}
