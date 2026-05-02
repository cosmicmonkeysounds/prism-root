//! `interaction::quick_create` — rapid object creation model.
//!
//! Defines the Quick Create flow: entity type selection, default
//! field population, and minimal-friction object creation for the
//! Cmd+N shortcut.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Quick Create Entry ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickCreateEntry {
    pub entity_type: String,
    pub label: String,
    pub icon: Option<String>,
    pub default_fields: Vec<QuickCreateField>,
    pub shortcut: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickCreateField {
    pub key: String,
    pub label: String,
    pub field_type: QuickCreateFieldType,
    pub required: bool,
    pub default_value: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuickCreateFieldType {
    Text,
    Date,
    Select,
    Number,
    Boolean,
}

// ── Quick Create Result ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickCreateResult {
    pub entity_type: String,
    pub name: String,
    pub fields: serde_json::Map<String, Value>,
}

// ── Registry ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct QuickCreateRegistry {
    entries: Vec<QuickCreateEntry>,
}

impl QuickCreateRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, entry: QuickCreateEntry) {
        self.entries.retain(|e| e.entity_type != entry.entity_type);
        self.entries.push(entry);
    }

    pub fn get(&self, entity_type: &str) -> Option<&QuickCreateEntry> {
        self.entries.iter().find(|e| e.entity_type == entity_type)
    }

    pub fn list(&self) -> &[QuickCreateEntry] {
        &self.entries
    }

    pub fn search(&self, query: &str) -> Vec<&QuickCreateEntry> {
        let q = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.label.to_lowercase().contains(&q)
                    || e.entity_type.to_lowercase().contains(&q)
            })
            .collect()
    }
}

// ── Default entries for Flux entity types ────────────────────────

pub fn flux_quick_create_entries() -> Vec<QuickCreateEntry> {
    vec![
        QuickCreateEntry {
            entity_type: "flux:task".into(),
            label: "Task".into(),
            icon: Some("check-square".into()),
            shortcut: Some("Cmd+Shift+T".into()),
            default_fields: vec![
                QuickCreateField {
                    key: "name".into(),
                    label: "Title".into(),
                    field_type: QuickCreateFieldType::Text,
                    required: true,
                    default_value: None,
                },
                QuickCreateField {
                    key: "status".into(),
                    label: "Status".into(),
                    field_type: QuickCreateFieldType::Select,
                    required: false,
                    default_value: Some(Value::String("todo".into())),
                },
                QuickCreateField {
                    key: "date".into(),
                    label: "Due Date".into(),
                    field_type: QuickCreateFieldType::Date,
                    required: false,
                    default_value: None,
                },
            ],
        },
        QuickCreateEntry {
            entity_type: "flux:project".into(),
            label: "Project".into(),
            icon: Some("folder".into()),
            shortcut: None,
            default_fields: vec![
                QuickCreateField {
                    key: "name".into(),
                    label: "Name".into(),
                    field_type: QuickCreateFieldType::Text,
                    required: true,
                    default_value: None,
                },
                QuickCreateField {
                    key: "status".into(),
                    label: "Status".into(),
                    field_type: QuickCreateFieldType::Select,
                    required: false,
                    default_value: Some(Value::String("planning".into())),
                },
            ],
        },
        QuickCreateEntry {
            entity_type: "flux:contact".into(),
            label: "Contact".into(),
            icon: Some("user".into()),
            shortcut: None,
            default_fields: vec![QuickCreateField {
                key: "name".into(),
                label: "Name".into(),
                field_type: QuickCreateFieldType::Text,
                required: true,
                default_value: None,
            }],
        },
        QuickCreateEntry {
            entity_type: "flux:goal".into(),
            label: "Goal".into(),
            icon: Some("target".into()),
            shortcut: None,
            default_fields: vec![
                QuickCreateField {
                    key: "name".into(),
                    label: "Title".into(),
                    field_type: QuickCreateFieldType::Text,
                    required: true,
                    default_value: None,
                },
                QuickCreateField {
                    key: "date".into(),
                    label: "Target Date".into(),
                    field_type: QuickCreateFieldType::Date,
                    required: false,
                    default_value: None,
                },
            ],
        },
        QuickCreateEntry {
            entity_type: "flux:invoice".into(),
            label: "Invoice".into(),
            icon: Some("file-text".into()),
            shortcut: None,
            default_fields: vec![
                QuickCreateField {
                    key: "name".into(),
                    label: "Title".into(),
                    field_type: QuickCreateFieldType::Text,
                    required: true,
                    default_value: None,
                },
                QuickCreateField {
                    key: "amount".into(),
                    label: "Amount".into(),
                    field_type: QuickCreateFieldType::Number,
                    required: false,
                    default_value: None,
                },
            ],
        },
    ]
}

pub fn create_default_registry() -> QuickCreateRegistry {
    let mut reg = QuickCreateRegistry::new();
    for entry in flux_quick_create_entries() {
        reg.register(entry);
    }
    reg
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_register_and_get() {
        let mut reg = QuickCreateRegistry::new();
        reg.register(QuickCreateEntry {
            entity_type: "test:item".into(),
            label: "Test Item".into(),
            icon: None,
            shortcut: None,
            default_fields: vec![],
        });
        assert!(reg.get("test:item").is_some());
        assert!(reg.get("test:other").is_none());
    }

    #[test]
    fn registry_replace_on_duplicate() {
        let mut reg = QuickCreateRegistry::new();
        reg.register(QuickCreateEntry {
            entity_type: "test:item".into(),
            label: "V1".into(),
            icon: None,
            shortcut: None,
            default_fields: vec![],
        });
        reg.register(QuickCreateEntry {
            entity_type: "test:item".into(),
            label: "V2".into(),
            icon: None,
            shortcut: None,
            default_fields: vec![],
        });
        assert_eq!(reg.list().len(), 1);
        assert_eq!(reg.get("test:item").unwrap().label, "V2");
    }

    #[test]
    fn registry_search() {
        let reg = create_default_registry();
        let results = reg.search("task");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_type, "flux:task");
    }

    #[test]
    fn registry_search_case_insensitive() {
        let reg = create_default_registry();
        assert!(!reg.search("CONTACT").is_empty());
    }

    #[test]
    fn registry_search_empty_query() {
        let reg = create_default_registry();
        let results = reg.search("");
        assert_eq!(results.len(), reg.list().len());
    }

    #[test]
    fn default_registry_has_flux_entries() {
        let reg = create_default_registry();
        assert_eq!(reg.list().len(), 5);
        assert!(reg.get("flux:task").is_some());
        assert!(reg.get("flux:project").is_some());
        assert!(reg.get("flux:contact").is_some());
        assert!(reg.get("flux:goal").is_some());
        assert!(reg.get("flux:invoice").is_some());
    }

    #[test]
    fn task_has_required_name_field() {
        let reg = create_default_registry();
        let task = reg.get("flux:task").unwrap();
        let name_field = task.default_fields.iter().find(|f| f.key == "name").unwrap();
        assert!(name_field.required);
        assert_eq!(name_field.field_type, QuickCreateFieldType::Text);
    }

    #[test]
    fn task_has_default_status() {
        let reg = create_default_registry();
        let task = reg.get("flux:task").unwrap();
        let status = task.default_fields.iter().find(|f| f.key == "status").unwrap();
        assert_eq!(
            status.default_value,
            Some(Value::String("todo".into()))
        );
    }

    #[test]
    fn quick_create_field_type_serde() {
        let t = QuickCreateFieldType::Date;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "\"date\"");
        let back: QuickCreateFieldType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn quick_create_result_serde() {
        let mut fields = serde_json::Map::new();
        fields.insert("status".into(), Value::String("todo".into()));
        let result = QuickCreateResult {
            entity_type: "flux:task".into(),
            name: "My Task".into(),
            fields,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: QuickCreateResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "My Task");
        assert_eq!(back.entity_type, "flux:task");
    }
}
