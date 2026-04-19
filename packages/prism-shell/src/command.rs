use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub id: String,
    pub label: String,
    pub category: String,
    pub shortcut: Option<String>,
}

impl CommandEntry {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        category: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            category: category.into(),
            shortcut: None,
        }
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn display_label(&self) -> String {
        if self.category.is_empty() {
            self.label.clone()
        } else {
            format!("{}: {}", self.category, self.label)
        }
    }
}

pub struct CommandRegistry {
    commands: HashMap<String, CommandEntry>,
    order: Vec<String>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        reg.register(
            CommandEntry::new("command_palette.toggle", "Command Palette", "View")
                .with_shortcut("Ctrl+Shift+P"),
        );
        reg.register(CommandEntry::new("edit.undo", "Undo", "Edit").with_shortcut("Ctrl+Z"));
        reg.register(CommandEntry::new("edit.redo", "Redo", "Edit").with_shortcut("Ctrl+Shift+Z"));
        reg.register(CommandEntry::new("file.save", "Save", "File").with_shortcut("Ctrl+S"));
        reg.register(CommandEntry::new("search.focus", "Search", "View").with_shortcut("Ctrl+F"));
        reg.register(CommandEntry::new("edit.copy", "Copy", "Edit").with_shortcut("Ctrl+C"));
        reg.register(CommandEntry::new("edit.paste", "Paste", "Edit").with_shortcut("Ctrl+V"));
        reg.register(CommandEntry::new("edit.cut", "Cut", "Edit").with_shortcut("Ctrl+X"));
        reg.register(
            CommandEntry::new("edit.duplicate", "Duplicate", "Edit").with_shortcut("Ctrl+D"),
        );
        reg.register(CommandEntry::new(
            "selection.delete",
            "Delete Selected",
            "Edit",
        ));
        reg.register(
            CommandEntry::new("selection.all", "Select All", "Edit").with_shortcut("Ctrl+A"),
        );
        reg.register(CommandEntry::new(
            "panel.identity",
            "Go to Identity",
            "Panel",
        ));
        reg.register(CommandEntry::new("panel.builder", "Go to Builder", "Panel"));
        reg.register(CommandEntry::new(
            "panel.inspector",
            "Go to Inspector",
            "Panel",
        ));
        reg.register(CommandEntry::new(
            "panel.properties",
            "Go to Properties",
            "Panel",
        ));
        reg.register(CommandEntry::new(
            "notification.dismiss_all",
            "Dismiss All Notifications",
            "View",
        ));
        reg
    }

    pub fn register(&mut self, entry: CommandEntry) {
        let id = entry.id.clone();
        if !self.commands.contains_key(&id) {
            self.order.push(id.clone());
        }
        self.commands.insert(id, entry);
    }

    pub fn get(&self, id: &str) -> Option<&CommandEntry> {
        self.commands.get(id)
    }

    pub fn list(&self) -> Vec<&CommandEntry> {
        self.order
            .iter()
            .filter_map(|id| self.commands.get(id))
            .collect()
    }

    pub fn filter(&self, query: &str) -> Vec<&CommandEntry> {
        if query.is_empty() {
            return self.list();
        }
        let query_lower = query.to_lowercase();
        let terms: Vec<&str> = query_lower.split_whitespace().collect();
        let mut scored: Vec<(&CommandEntry, usize)> = self
            .list()
            .into_iter()
            .filter_map(|entry| {
                let label = entry.display_label().to_lowercase();
                let id_lower = entry.id.to_lowercase();
                let all_match = terms
                    .iter()
                    .all(|term| label.contains(term) || id_lower.contains(term));
                if all_match {
                    let score = if label.starts_with(&query_lower) {
                        2
                    } else {
                        1
                    };
                    Some((entry, score))
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(entry, _)| entry).collect()
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_get() {
        let mut reg = CommandRegistry::new();
        reg.register(CommandEntry::new("test.cmd", "Test", "Testing"));
        assert!(reg.get("test.cmd").is_some());
        assert_eq!(reg.get("test.cmd").unwrap().label, "Test");
    }

    #[test]
    fn list_preserves_order() {
        let mut reg = CommandRegistry::new();
        reg.register(CommandEntry::new("b", "B", "Cat"));
        reg.register(CommandEntry::new("a", "A", "Cat"));
        let list = reg.list();
        assert_eq!(list[0].id, "b");
        assert_eq!(list[1].id, "a");
    }

    #[test]
    fn filter_matches_label() {
        let reg = CommandRegistry::with_builtins();
        let results = reg.filter("undo");
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "edit.undo");
    }

    #[test]
    fn filter_matches_category() {
        let reg = CommandRegistry::with_builtins();
        let results = reg.filter("edit");
        assert!(results.len() >= 2);
    }

    #[test]
    fn filter_matches_multi_word() {
        let reg = CommandRegistry::with_builtins();
        let results = reg.filter("edit undo");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "edit.undo");
    }

    #[test]
    fn filter_empty_returns_all() {
        let reg = CommandRegistry::with_builtins();
        let all = reg.filter("");
        assert_eq!(all.len(), reg.len());
    }

    #[test]
    fn filter_no_match() {
        let reg = CommandRegistry::with_builtins();
        let results = reg.filter("xyznonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn display_label_with_category() {
        let cmd = CommandEntry::new("x", "Do Thing", "My Cat");
        assert_eq!(cmd.display_label(), "My Cat: Do Thing");
    }

    #[test]
    fn display_label_without_category() {
        let cmd = CommandEntry::new("x", "Do Thing", "");
        assert_eq!(cmd.display_label(), "Do Thing");
    }

    #[test]
    fn with_builtins_has_standard_commands() {
        let reg = CommandRegistry::with_builtins();
        assert!(reg.get("edit.undo").is_some());
        assert!(reg.get("edit.redo").is_some());
        assert!(reg.get("command_palette.toggle").is_some());
    }

    #[test]
    fn with_builtins_has_clipboard_commands() {
        let reg = CommandRegistry::with_builtins();
        assert!(reg.get("edit.copy").is_some());
        assert!(reg.get("edit.paste").is_some());
        assert!(reg.get("edit.cut").is_some());
        assert!(reg.get("edit.duplicate").is_some());
        assert_eq!(
            reg.get("edit.copy").unwrap().shortcut,
            Some("Ctrl+C".to_string())
        );
    }

    #[test]
    fn re_register_updates_entry() {
        let mut reg = CommandRegistry::new();
        reg.register(CommandEntry::new("x", "Old", ""));
        reg.register(CommandEntry::new("x", "New", ""));
        assert_eq!(reg.get("x").unwrap().label, "New");
        assert_eq!(reg.len(), 1);
    }
}
