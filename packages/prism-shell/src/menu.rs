use crate::command::CommandRegistry;

#[derive(Debug, Clone)]
pub struct MenuItemDef {
    pub command_id: String,
    pub menu: String,
    pub group: u32,
    pub order: u32,
}

#[derive(Debug, Clone)]
pub struct ResolvedMenuItem {
    pub label: String,
    pub shortcut: String,
    pub command_id: String,
    pub is_separator: bool,
}

pub struct MenuRegistry {
    items: Vec<MenuItemDef>,
    menu_order: Vec<String>,
}

impl MenuRegistry {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            menu_order: Vec::new(),
        }
    }

    pub fn with_builtins() -> Self {
        let mut reg = Self::new();

        // File
        reg.add("file.new", "File", 1, 1);
        reg.add("file.open", "File", 1, 2);
        reg.add("project.open_folder", "File", 1, 3);
        reg.add("project.close", "File", 1, 4);
        reg.add("file.save", "File", 2, 1);
        reg.add("file.save_as", "File", 2, 2);
        reg.add("file.revert", "File", 2, 3);

        // Edit
        reg.add("edit.undo", "Edit", 1, 1);
        reg.add("edit.redo", "Edit", 1, 2);
        reg.add("edit.cut", "Edit", 2, 1);
        reg.add("edit.copy", "Edit", 2, 2);
        reg.add("edit.paste", "Edit", 2, 3);
        reg.add("edit.duplicate", "Edit", 2, 4);
        reg.add("selection.all", "Edit", 3, 1);
        reg.add("selection.delete", "Edit", 4, 1);

        // View
        reg.add("command_palette.toggle", "View", 1, 1);
        reg.add("search.focus", "View", 1, 2);
        reg.add("view.toggle_activity_bar", "View", 2, 1);
        reg.add("view.toggle_left_sidebar", "View", 2, 2);
        reg.add("view.toggle_right_sidebar", "View", 2, 3);
        reg.add("view.toggle_grid", "View", 3, 1);

        // Window
        reg.add("panel.identity", "Window", 1, 1);
        reg.add("panel.explorer", "Window", 1, 2);
        reg.add("panel.edit", "Window", 1, 3);
        reg.add("panel.code_editor", "Window", 1, 4);
        reg.add("navigate.next_tab", "Window", 2, 1);
        reg.add("navigate.prev_tab", "Window", 2, 2);

        // Help
        reg.add("notification.dismiss_all", "Help", 1, 1);

        reg
    }

    fn add(&mut self, command_id: &str, menu: &str, group: u32, order: u32) {
        if !self.menu_order.iter().any(|m| m == menu) {
            self.menu_order.push(menu.into());
        }
        self.items.push(MenuItemDef {
            command_id: command_id.into(),
            menu: menu.into(),
            group,
            order,
        });
    }

    pub fn menu_names(&self) -> &[String] {
        &self.menu_order
    }

    pub fn items_for_menu(&self, menu: &str, commands: &CommandRegistry) -> Vec<ResolvedMenuItem> {
        let mut defs: Vec<&MenuItemDef> = self.items.iter().filter(|d| d.menu == menu).collect();
        defs.sort_by_key(|d| (d.group, d.order));

        let mut result = Vec::new();
        let mut last_group = None;

        for def in &defs {
            if let Some(lg) = last_group {
                if lg != def.group {
                    result.push(ResolvedMenuItem {
                        label: String::new(),
                        shortcut: String::new(),
                        command_id: String::new(),
                        is_separator: true,
                    });
                }
            }
            last_group = Some(def.group);

            let (label, shortcut) = if let Some(cmd) = commands.get(&def.command_id) {
                (cmd.label.clone(), cmd.shortcut.clone().unwrap_or_default())
            } else {
                (def.command_id.clone(), String::new())
            };

            result.push(ResolvedMenuItem {
                label,
                shortcut,
                command_id: def.command_id.clone(),
                is_separator: false,
            });
        }

        result
    }
}

impl Default for MenuRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandRegistry;

    #[test]
    fn builtins_has_five_menus() {
        let reg = MenuRegistry::with_builtins();
        assert_eq!(reg.menu_names().len(), 5);
        assert_eq!(reg.menu_names()[0], "File");
        assert_eq!(reg.menu_names()[1], "Edit");
        assert_eq!(reg.menu_names()[2], "View");
        assert_eq!(reg.menu_names()[3], "Window");
        assert_eq!(reg.menu_names()[4], "Help");
    }

    #[test]
    fn items_for_menu_resolves_labels() {
        let menu_reg = MenuRegistry::with_builtins();
        let cmd_reg = CommandRegistry::with_builtins();
        let items = menu_reg.items_for_menu("Edit", &cmd_reg);
        assert!(!items.is_empty());
        assert_eq!(items[0].label, "Undo");
        assert_eq!(items[0].shortcut, "Ctrl+Z");
    }

    #[test]
    fn separators_between_groups() {
        let menu_reg = MenuRegistry::with_builtins();
        let cmd_reg = CommandRegistry::with_builtins();
        let items = menu_reg.items_for_menu("Edit", &cmd_reg);
        let sep_count = items.iter().filter(|i| i.is_separator).count();
        assert!(sep_count >= 2);
    }

    #[test]
    fn unknown_menu_returns_empty() {
        let menu_reg = MenuRegistry::with_builtins();
        let cmd_reg = CommandRegistry::with_builtins();
        let items = menu_reg.items_for_menu("Nonexistent", &cmd_reg);
        assert!(items.is_empty());
    }

    #[test]
    fn items_sorted_by_group_then_order() {
        let menu_reg = MenuRegistry::with_builtins();
        let cmd_reg = CommandRegistry::with_builtins();
        let items = menu_reg.items_for_menu("Edit", &cmd_reg);
        let non_sep: Vec<_> = items.iter().filter(|i| !i.is_separator).collect();
        assert_eq!(non_sep[0].label, "Undo");
        assert_eq!(non_sep[1].label, "Redo");
    }

    #[test]
    fn file_menu_has_new_open_save_save_as() {
        let menu_reg = MenuRegistry::with_builtins();
        let cmd_reg = CommandRegistry::with_builtins();
        let items = menu_reg.items_for_menu("File", &cmd_reg);
        let non_sep: Vec<_> = items.iter().filter(|i| !i.is_separator).collect();
        assert_eq!(non_sep[0].label, "New Project");
        assert_eq!(non_sep[1].label, "Open Project");
        assert_eq!(non_sep[2].label, "Open Folder...");
        assert_eq!(non_sep[3].label, "Close Folder");
        assert_eq!(non_sep[4].label, "Save");
        assert_eq!(non_sep[5].label, "Save As...");
        assert!(
            non_sep.len() >= 6,
            "File menu must have at least New/Open/Open Folder/Close Folder/Save/Save As"
        );
    }

    #[test]
    fn file_menu_has_separator_between_groups() {
        let menu_reg = MenuRegistry::with_builtins();
        let cmd_reg = CommandRegistry::with_builtins();
        let items = menu_reg.items_for_menu("File", &cmd_reg);
        let sep_count = items.iter().filter(|i| i.is_separator).count();
        assert_eq!(sep_count, 1);
    }
}
