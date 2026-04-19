use prism_core::help::HelpEntry;

use super::Panel;

#[derive(Default)]
pub struct CodeEditorPanel;

impl CodeEditorPanel {
    pub fn new() -> Self {
        Self
    }
}

impl Panel for CodeEditorPanel {
    fn id(&self) -> i32 {
        2
    }
    fn label(&self) -> &'static str {
        "Code"
    }
    fn title(&self) -> &'static str {
        "Code Editor"
    }
    fn hint(&self) -> &'static str {
        "Edit code and text files with syntax highlighting."
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "shell.panels.code-editor",
            "Code Editor",
            "Text and code editing surface with ropey-backed buffer, cursor management, and selection. Integrates with Loro CRDT for collaborative editing.",
        ))
    }
}
