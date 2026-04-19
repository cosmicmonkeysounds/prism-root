//! `DocumentSchema` — a named collection of fields laid out into
//! ordered sections.
//!
//! Port of `language/forms/document-schema.ts`. A document schema
//! describes the shape of a form-driven file (Flux records, planner
//! docs, CRM entries). Pure data; the render half lives in the shell,
//! the expression-driven subset lives in
//! [`crate::language::expression`].

use serde::{Deserialize, Serialize};

use super::field_schema::FieldSchema;

/// A free-form text block interleaved between field groups. `dynamic`
/// sections expand to a markdown-backed editor; non-dynamic text
/// sections render as static help text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSection {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic: Option<bool>,
}

/// An ordered group of field ids, optionally laid out in multiple
/// columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldGroupSection {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(rename = "fieldIds")]
    pub field_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<u8>,
}

/// One entry in a [`DocumentSchema::sections`] vec. Serialised with a
/// `kind` discriminator so the on-disk shape matches the TS tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SectionDef {
    Text(TextSection),
    FieldGroup(FieldGroupSection),
}

impl SectionDef {
    pub fn id(&self) -> &str {
        match self {
            SectionDef::Text(t) => &t.id,
            SectionDef::FieldGroup(g) => &g.id,
        }
    }

    pub fn label(&self) -> Option<&str> {
        match self {
            SectionDef::Text(t) => t.label.as_deref(),
            SectionDef::FieldGroup(g) => g.label.as_deref(),
        }
    }
}

/// Narrows a [`SectionDef`] to a [`TextSection`] reference.
pub fn is_text_section(section: &SectionDef) -> Option<&TextSection> {
    match section {
        SectionDef::Text(t) => Some(t),
        _ => None,
    }
}

/// Narrows a [`SectionDef`] to a [`FieldGroupSection`] reference.
pub fn is_field_group_section(section: &SectionDef) -> Option<&FieldGroupSection> {
    match section {
        SectionDef::FieldGroup(g) => Some(g),
        _ => None,
    }
}

/// A document-level schema: an ordered field set plus a layout
/// (sections) describing how the fields are grouped for display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSchema {
    pub id: String,
    pub name: String,
    pub fields: Vec<FieldSchema>,
    pub sections: Vec<SectionDef>,
}

impl DocumentSchema {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            fields: Vec::new(),
            sections: Vec::new(),
        }
    }
}

/// Find a field by id inside a schema. Returns `None` if the schema
/// doesn't contain it.
pub fn get_field<'s>(schema: &'s DocumentSchema, field_id: &str) -> Option<&'s FieldSchema> {
    schema.fields.iter().find(|f| f.id == field_id)
}

/// The ids of every field referenced by a field-group section, in
/// section order. Text sections are skipped.
pub fn ordered_field_ids(schema: &DocumentSchema) -> Vec<String> {
    let mut ids = Vec::new();
    for section in &schema.sections {
        if let SectionDef::FieldGroup(g) = section {
            ids.extend(g.field_ids.iter().cloned());
        }
    }
    ids
}

/// Hydrate [`ordered_field_ids`] into [`FieldSchema`] references.
/// Ids that no longer resolve to a field are dropped (matches the TS
/// behaviour where `.filter((f) => f !== undefined)` ran after the
/// map).
pub fn ordered_fields(schema: &DocumentSchema) -> Vec<&FieldSchema> {
    let mut out = Vec::new();
    for section in &schema.sections {
        if let SectionDef::FieldGroup(g) = section {
            for id in &g.field_ids {
                if let Some(f) = get_field(schema, id) {
                    out.push(f);
                }
            }
        }
    }
    out
}

/// Canonical dynamic notes section — matches `NOTES_TEXT_SECTION` in
/// the TS tree byte-for-byte.
pub fn notes_text_section() -> SectionDef {
    SectionDef::Text(TextSection {
        id: "notes".into(),
        label: Some("Notes".into()),
        dynamic: Some(true),
    })
}

/// Canonical static description section.
pub fn description_text_section() -> SectionDef {
    SectionDef::Text(TextSection {
        id: "description".into(),
        label: Some("Description".into()),
        dynamic: Some(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::forms::field_schema::FieldType;

    fn sample_schema() -> DocumentSchema {
        DocumentSchema {
            id: "contact".into(),
            name: "Contact".into(),
            fields: vec![
                FieldSchema::new("name", "Name", FieldType::Text),
                FieldSchema::new("email", "Email", FieldType::Email),
                FieldSchema::new("notes", "Notes", FieldType::Textarea),
            ],
            sections: vec![
                SectionDef::FieldGroup(FieldGroupSection {
                    id: "primary".into(),
                    label: Some("Primary".into()),
                    field_ids: vec!["name".into(), "email".into()],
                    columns: Some(2),
                }),
                notes_text_section(),
            ],
        }
    }

    #[test]
    fn get_field_returns_field_by_id() {
        let schema = sample_schema();
        assert_eq!(
            get_field(&schema, "email").map(|f| f.label.as_str()),
            Some("Email")
        );
        assert!(get_field(&schema, "missing").is_none());
    }

    #[test]
    fn ordered_field_ids_skips_text_sections() {
        let schema = sample_schema();
        assert_eq!(
            ordered_field_ids(&schema),
            vec!["name".to_string(), "email".to_string()]
        );
    }

    #[test]
    fn ordered_fields_drops_unresolved_ids() {
        let mut schema = sample_schema();
        if let SectionDef::FieldGroup(g) = &mut schema.sections[0] {
            g.field_ids.push("ghost".into());
        }
        let fields = ordered_fields(&schema);
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn section_tag_serializes_kebab_case() {
        let section = SectionDef::FieldGroup(FieldGroupSection {
            id: "primary".into(),
            label: None,
            field_ids: vec!["name".into()],
            columns: None,
        });
        let json = serde_json::to_string(&section).unwrap();
        assert!(json.contains("\"kind\":\"field-group\""));
    }

    #[test]
    fn notes_text_section_matches_ts_shape() {
        let section = notes_text_section();
        if let SectionDef::Text(t) = &section {
            assert_eq!(t.id, "notes");
            assert_eq!(t.label.as_deref(), Some("Notes"));
            assert_eq!(t.dynamic, Some(true));
        } else {
            panic!("expected text section");
        }
    }

    #[test]
    fn is_text_section_narrows() {
        let t = notes_text_section();
        assert!(is_text_section(&t).is_some());
        assert!(is_field_group_section(&t).is_none());
    }

    #[test]
    fn round_trip_schema_json() {
        let schema = sample_schema();
        let json = serde_json::to_string(&schema).unwrap();
        let back: DocumentSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(back, schema);
    }
}
