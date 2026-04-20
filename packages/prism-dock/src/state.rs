use serde::{Deserialize, Serialize};

use crate::node::{Axis, DockNode, MoveTarget, NodeAddress, SplitPosition};
use crate::panel::PanelId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockState {
    pub root: DockNode,
}

impl DockState {
    pub fn new(root: DockNode) -> Self {
        Self { root }
    }

    pub fn single(panel: PanelId) -> Self {
        Self {
            root: DockNode::tab(panel),
        }
    }

    pub fn panel_ids(&self) -> Vec<&PanelId> {
        self.root.panel_ids()
    }

    pub fn contains_panel(&self, panel_id: &str) -> bool {
        self.root.contains_panel(panel_id)
    }

    pub fn find_panel(&self, panel_id: &str) -> Option<NodeAddress> {
        self.root.find_panel(panel_id)
    }

    pub fn activate_tab(&mut self, addr: &NodeAddress, index: usize) -> bool {
        self.root.activate_tab(addr, index)
    }

    pub fn set_ratio(&mut self, addr: &NodeAddress, ratio: f32) -> bool {
        self.root.set_ratio(addr, ratio)
    }

    pub fn add_tab(&mut self, addr: &NodeAddress, panel: PanelId) -> bool {
        self.root.add_tab(addr, panel)
    }

    pub fn close_tab(&mut self, addr: &NodeAddress, tab_index: usize) -> Option<PanelId> {
        self.root.close_tab(addr, tab_index)
    }

    pub fn split(
        &mut self,
        addr: &NodeAddress,
        axis: Axis,
        panel: PanelId,
        position: SplitPosition,
    ) -> bool {
        self.root.split_at(addr, axis, panel, position)
    }

    pub fn remove_panel(&mut self, panel_id: &str) -> Option<PanelId> {
        self.root.remove_panel(panel_id)
    }

    pub fn move_panel(&mut self, panel_id: &str, target: &MoveTarget) -> bool {
        self.root.move_panel(panel_id, target)
    }

    pub fn tab_count(&self) -> usize {
        self.root.tab_count()
    }

    pub fn leaf_count(&self) -> usize {
        self.root.leaf_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_panel_state() {
        let state = DockState::single("builder".into());
        assert_eq!(state.tab_count(), 1);
        assert!(state.contains_panel("builder"));
    }

    #[test]
    fn state_delegates_to_root() {
        let mut state = DockState::new(DockNode::hsplit(
            0.3,
            DockNode::tab("a".into()),
            DockNode::tab("b".into()),
        ));
        assert_eq!(state.tab_count(), 2);
        assert!(state.set_ratio(&NodeAddress::root(), 0.6));
        let removed = state.remove_panel("b");
        assert_eq!(removed.as_deref(), Some("b"));
        assert_eq!(state.tab_count(), 1);
    }

    #[test]
    fn serde_roundtrip() {
        let state = DockState::new(DockNode::hsplit(
            0.25,
            DockNode::tab("explorer".into()),
            DockNode::tabs(vec!["builder".into(), "inspector".into()]),
        ));
        let json = serde_json::to_string(&state).unwrap();
        let state2: DockState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.tab_count(), state2.tab_count());
    }
}
