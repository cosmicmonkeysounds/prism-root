use prism_builder::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionModel {
    items: Vec<NodeId>,
    focus_index: Option<usize>,
    depth: usize,
}

impl SelectionModel {
    pub fn single(id: NodeId) -> Self {
        Self {
            items: vec![id],
            focus_index: Some(0),
            depth: 0,
        }
    }

    pub fn primary(&self) -> Option<&NodeId> {
        self.focus_index
            .and_then(|i| self.items.get(i))
            .or_else(|| self.items.first())
    }

    pub fn items(&self) -> &[NodeId] {
        &self.items
    }

    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn is_multi(&self) -> bool {
        self.items.len() > 1
    }

    pub fn contains(&self, id: &str) -> bool {
        self.items.iter().any(|item| item == id)
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn select(&mut self, id: NodeId) {
        self.items.clear();
        self.items.push(id);
        self.focus_index = Some(0);
        self.depth = 0;
    }

    pub fn toggle(&mut self, id: NodeId) {
        if let Some(pos) = self.items.iter().position(|item| *item == id) {
            self.items.remove(pos);
            self.focus_index = if self.items.is_empty() {
                None
            } else {
                Some(pos.min(self.items.len() - 1))
            };
        } else {
            self.items.push(id);
            self.focus_index = Some(self.items.len() - 1);
        }
    }

    pub fn extend(&mut self, id: NodeId) {
        if !self.contains(&id) {
            self.items.push(id);
            self.focus_index = Some(self.items.len() - 1);
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.focus_index = None;
        self.depth = 0;
    }

    pub fn set_depth(&mut self, depth: usize) {
        self.depth = depth;
    }

    pub fn deepen(&mut self) {
        self.depth += 1;
    }

    pub fn shallow(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    pub fn as_option(&self) -> Option<NodeId> {
        self.primary().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        let sel = SelectionModel::default();
        assert!(sel.is_empty());
        assert_eq!(sel.count(), 0);
        assert!(sel.primary().is_none());
        assert!(!sel.is_multi());
    }

    #[test]
    fn single_selects_one_node() {
        let sel = SelectionModel::single("hero".into());
        assert_eq!(sel.count(), 1);
        assert_eq!(sel.primary(), Some(&"hero".to_string()));
        assert!(!sel.is_multi());
    }

    #[test]
    fn select_replaces_selection() {
        let mut sel = SelectionModel::single("a".into());
        sel.select("b".into());
        assert_eq!(sel.count(), 1);
        assert_eq!(sel.primary(), Some(&"b".to_string()));
    }

    #[test]
    fn toggle_adds_and_removes() {
        let mut sel = SelectionModel::single("a".into());
        sel.toggle("b".into());
        assert_eq!(sel.count(), 2);
        assert!(sel.is_multi());
        assert!(sel.contains("a"));
        assert!(sel.contains("b"));

        sel.toggle("a".into());
        assert_eq!(sel.count(), 1);
        assert_eq!(sel.primary(), Some(&"b".to_string()));
    }

    #[test]
    fn extend_prevents_duplicates() {
        let mut sel = SelectionModel::single("a".into());
        sel.extend("a".into());
        assert_eq!(sel.count(), 1);
        sel.extend("b".into());
        assert_eq!(sel.count(), 2);
    }

    #[test]
    fn clear_empties_selection() {
        let mut sel = SelectionModel::single("a".into());
        sel.clear();
        assert!(sel.is_empty());
        assert!(sel.primary().is_none());
    }

    #[test]
    fn depth_controls() {
        let mut sel = SelectionModel::single("a".into());
        assert_eq!(sel.depth(), 0);
        sel.deepen();
        assert_eq!(sel.depth(), 1);
        sel.deepen();
        assert_eq!(sel.depth(), 2);
        sel.shallow();
        assert_eq!(sel.depth(), 1);
        sel.shallow();
        assert_eq!(sel.depth(), 0);
        sel.shallow();
        assert_eq!(sel.depth(), 0);
    }

    #[test]
    fn as_option_returns_primary() {
        let sel = SelectionModel::single("x".into());
        assert_eq!(sel.as_option(), Some("x".to_string()));
        let empty = SelectionModel::default();
        assert_eq!(empty.as_option(), None);
    }

    #[test]
    fn toggle_last_item_empties_selection() {
        let mut sel = SelectionModel::single("a".into());
        sel.toggle("a".into());
        assert!(sel.is_empty());
        assert!(sel.primary().is_none());
    }

    #[test]
    fn select_resets_depth() {
        let mut sel = SelectionModel::single("a".into());
        sel.set_depth(3);
        sel.select("b".into());
        assert_eq!(sel.depth(), 0);
    }
}
