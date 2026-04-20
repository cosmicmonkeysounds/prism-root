use serde::{Deserialize, Serialize};

use crate::node::DockNode;
use crate::page::WorkflowPage;
use crate::panel::PanelId;
use crate::state::DockState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockWorkspace {
    pages: Vec<WorkflowPage>,
    active: usize,
    customized: Vec<Option<DockState>>,
}

impl DockWorkspace {
    pub fn new(pages: Vec<WorkflowPage>) -> Self {
        let len = pages.len();
        Self {
            pages,
            active: 0,
            customized: vec![None; len],
        }
    }

    pub fn with_builtins() -> Self {
        Self::new(WorkflowPage::builtins())
    }

    pub fn pages(&self) -> &[WorkflowPage] {
        &self.pages
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active_page(&self) -> &WorkflowPage {
        &self.pages[self.active]
    }

    pub fn active_dock(&self) -> &DockState {
        self.customized[self.active]
            .as_ref()
            .unwrap_or(&self.pages[self.active].dock)
    }

    pub fn active_dock_mut(&mut self) -> &mut DockState {
        if self.customized[self.active].is_none() {
            self.customized[self.active] = Some(self.pages[self.active].dock.clone());
        }
        self.customized[self.active].as_mut().unwrap()
    }

    pub fn switch_page(&mut self, index: usize) -> bool {
        if index < self.pages.len() {
            self.active = index;
            true
        } else {
            false
        }
    }

    pub fn switch_page_by_id(&mut self, id: &str) -> bool {
        if let Some(idx) = self.pages.iter().position(|p| p.id == id) {
            self.active = idx;
            true
        } else {
            false
        }
    }

    pub fn reset_page(&mut self, index: usize) -> bool {
        if index < self.pages.len() {
            self.customized[index] = None;
            true
        } else {
            false
        }
    }

    pub fn reset_active_page(&mut self) {
        self.customized[self.active] = None;
    }

    pub fn is_customized(&self, index: usize) -> bool {
        self.customized.get(index).is_some_and(|c| c.is_some())
    }

    pub fn add_page(&mut self, page: WorkflowPage) {
        self.pages.push(page);
        self.customized.push(None);
    }

    pub fn remove_page(&mut self, index: usize) -> Option<WorkflowPage> {
        if index >= self.pages.len() || self.pages.len() <= 1 {
            return None;
        }
        self.customized.remove(index);
        let page = self.pages.remove(index);
        if self.active >= self.pages.len() {
            self.active = self.pages.len() - 1;
        }
        Some(page)
    }

    pub fn ensure_panel_visible(&mut self, panel_id: &str) -> bool {
        if self.active_dock().contains_panel(panel_id) {
            return true;
        }
        let dock = self.active_dock_mut();
        let last_leaf = Self::find_last_leaf(&dock.root);
        dock.root.add_tab(&last_leaf, panel_id.into())
    }

    fn find_last_leaf(node: &DockNode) -> crate::node::NodeAddress {
        Self::find_last_leaf_inner(node, &crate::node::NodeAddress::root())
    }

    fn find_last_leaf_inner(
        node: &DockNode,
        addr: &crate::node::NodeAddress,
    ) -> crate::node::NodeAddress {
        match node {
            DockNode::TabGroup { .. } => addr.clone(),
            DockNode::Split { second, .. } => Self::find_last_leaf_inner(second, &addr.second()),
        }
    }

    pub fn find_panel(&self, panel_id: &str) -> Option<crate::node::NodeAddress> {
        self.active_dock().find_panel(panel_id)
    }

    pub fn panel_ids(&self) -> Vec<&PanelId> {
        self.active_dock().panel_ids()
    }

    pub fn toggle_panel(&mut self, panel_id: &str) {
        if self.active_dock().contains_panel(panel_id) {
            self.active_dock_mut().remove_panel(panel_id);
        } else {
            self.ensure_panel_visible(panel_id);
        }
    }

    pub fn customize_dock(&mut self, dock: DockState) {
        self.customized[self.active] = Some(dock);
    }

    pub fn save_layout_for_page(&mut self, index: usize) -> Option<DockState> {
        if index < self.pages.len() {
            self.customized
                .get(index)
                .cloned()
                .unwrap_or(None)
                .or_else(|| Some(self.pages[index].dock.clone()))
        } else {
            None
        }
    }

    pub fn restore_layout_for_page(&mut self, index: usize, dock: DockState) -> bool {
        if index < self.pages.len() {
            self.customized[index] = Some(dock);
            true
        } else {
            false
        }
    }

    pub fn cycle_page(&mut self, forward: bool) {
        if self.pages.is_empty() {
            return;
        }
        if forward {
            self.active = (self.active + 1) % self.pages.len();
        } else {
            self.active = if self.active == 0 {
                self.pages.len() - 1
            } else {
                self.active - 1
            };
        }
    }

    pub fn find_page_for_panel(&self, panel_id: &str) -> Option<usize> {
        for (i, page) in self.pages.iter().enumerate() {
            let dock = self.customized[i].as_ref().unwrap_or(&page.dock);
            if dock.contains_panel(panel_id) {
                return Some(i);
            }
        }
        None
    }

    pub fn navigate_to_panel(&mut self, panel_id: &str) -> bool {
        if self.active_dock().contains_panel(panel_id) {
            if let Some(addr) = self.active_dock().find_panel(panel_id) {
                if let Some(DockNode::TabGroup { tabs, .. }) =
                    self.active_dock().root.node_at(&addr)
                {
                    if let Some(tab_idx) = tabs.iter().position(|t| t == panel_id) {
                        return self.active_dock_mut().activate_tab(&addr, tab_idx);
                    }
                }
            }
            return true;
        }
        if let Some(page_idx) = self.find_page_for_panel(panel_id) {
            self.active = page_idx;
            if let Some(addr) = self.active_dock().find_panel(panel_id) {
                if let Some(DockNode::TabGroup { tabs, .. }) =
                    self.active_dock().root.node_at(&addr)
                {
                    if let Some(tab_idx) = tabs.iter().position(|t| t == panel_id) {
                        return self.active_dock_mut().activate_tab(&addr, tab_idx);
                    }
                }
            }
            return true;
        }
        false
    }
}

impl Default for DockWorkspace {
    fn default() -> Self {
        Self::with_builtins()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_starts_on_edit() {
        let ws = DockWorkspace::with_builtins();
        assert_eq!(ws.active_index(), 0);
        assert_eq!(ws.active_page().id, "edit");
    }

    #[test]
    fn switch_page_by_id() {
        let mut ws = DockWorkspace::with_builtins();
        assert!(ws.switch_page_by_id("fusion"));
        assert_eq!(ws.active_page().id, "fusion");
        assert!(ws.active_dock().contains_panel("node-graph"));
    }

    #[test]
    fn switch_page_out_of_bounds() {
        let mut ws = DockWorkspace::with_builtins();
        assert!(!ws.switch_page(99));
        assert_eq!(ws.active_index(), 0);
    }

    #[test]
    fn customization_preserved_across_page_switch() {
        let mut ws = DockWorkspace::with_builtins();
        ws.active_dock_mut().remove_panel("properties");
        assert!(!ws.active_dock().contains_panel("properties"));

        ws.switch_page_by_id("code");
        ws.switch_page_by_id("edit");
        assert!(!ws.active_dock().contains_panel("properties"));
    }

    #[test]
    fn reset_page_restores_default() {
        let mut ws = DockWorkspace::with_builtins();
        ws.active_dock_mut().remove_panel("properties");
        assert!(!ws.active_dock().contains_panel("properties"));

        ws.reset_active_page();
        assert!(ws.active_dock().contains_panel("properties"));
    }

    #[test]
    fn toggle_panel_adds_and_removes() {
        let mut ws = DockWorkspace::with_builtins();
        assert!(!ws.active_dock().contains_panel("timeline"));
        ws.toggle_panel("timeline");
        assert!(ws.active_dock().contains_panel("timeline"));
        ws.toggle_panel("timeline");
        assert!(!ws.active_dock().contains_panel("timeline"));
    }

    #[test]
    fn cycle_page_wraps() {
        let mut ws = DockWorkspace::with_builtins();
        ws.cycle_page(false);
        assert_eq!(ws.active_page().id, "fusion");
        ws.cycle_page(true);
        assert_eq!(ws.active_page().id, "edit");
    }

    #[test]
    fn add_custom_page() {
        let mut ws = DockWorkspace::with_builtins();
        let page = WorkflowPage {
            id: "custom".into(),
            label: "Custom".into(),
            icon_hint: "custom".into(),
            dock: DockState::single("builder".into()),
        };
        ws.add_page(page);
        assert_eq!(ws.pages().len(), 5);
        assert!(ws.switch_page_by_id("custom"));
    }

    #[test]
    fn remove_page() {
        let mut ws = DockWorkspace::with_builtins();
        let removed = ws.remove_page(3);
        assert_eq!(removed.unwrap().id, "fusion");
        assert_eq!(ws.pages().len(), 3);
    }

    #[test]
    fn cannot_remove_last_page() {
        let mut ws = DockWorkspace::new(vec![WorkflowPage::edit()]);
        assert!(ws.remove_page(0).is_none());
    }

    #[test]
    fn find_page_for_panel() {
        let ws = DockWorkspace::with_builtins();
        let idx = ws.find_page_for_panel("node-graph").unwrap();
        assert_eq!(ws.pages()[idx].id, "fusion");
    }

    #[test]
    fn navigate_to_panel_switches_page() {
        let mut ws = DockWorkspace::with_builtins();
        assert!(ws.navigate_to_panel("node-graph"));
        assert_eq!(ws.active_page().id, "fusion");
    }

    #[test]
    fn navigate_to_nonexistent_fails() {
        let mut ws = DockWorkspace::with_builtins();
        assert!(!ws.navigate_to_panel("nonexistent"));
    }

    #[test]
    fn is_customized() {
        let mut ws = DockWorkspace::with_builtins();
        assert!(!ws.is_customized(0));
        ws.active_dock_mut();
        assert!(ws.is_customized(0));
    }

    #[test]
    fn serde_roundtrip() {
        let mut ws = DockWorkspace::with_builtins();
        ws.switch_page_by_id("code");
        ws.active_dock_mut().remove_panel("console");
        let json = serde_json::to_string(&ws).unwrap();
        let ws2: DockWorkspace = serde_json::from_str(&json).unwrap();
        assert_eq!(ws2.active_index(), 2);
        assert!(!ws2.active_dock().contains_panel("console"));
    }

    #[test]
    fn ensure_panel_visible() {
        let mut ws = DockWorkspace::with_builtins();
        assert!(!ws.active_dock().contains_panel("timeline"));
        assert!(ws.ensure_panel_visible("timeline"));
        assert!(ws.active_dock().contains_panel("timeline"));
    }

    #[test]
    fn ensure_already_visible_panel() {
        let mut ws = DockWorkspace::with_builtins();
        assert!(ws.ensure_panel_visible("builder"));
        assert!(ws.active_dock().contains_panel("builder"));
    }

    #[test]
    fn save_and_restore_layout() {
        let mut ws = DockWorkspace::with_builtins();
        ws.active_dock_mut().remove_panel("properties");
        let saved = ws.save_layout_for_page(0).unwrap();
        ws.reset_active_page();
        assert!(ws.active_dock().contains_panel("properties"));
        ws.restore_layout_for_page(0, saved);
        assert!(!ws.active_dock().contains_panel("properties"));
    }
}
