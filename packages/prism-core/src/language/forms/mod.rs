//! `language/forms` — document / form schema, wiki links, and
//! Prism's narrow markdown dialect.
//!
//! Port of `packages/prism-core/src/language/forms/*.ts` from
//! pre-Rust commit `8426588`. Leaf order:
//!
//! - [`field_schema`] → [`document_schema`] → [`form_schema`]
//! - [`form_state`] (no deps)
//! - [`wiki_link`] (no deps)
//! - [`markdown`] (depends on [`wiki_link`])
//!
//! Everything in this subtree is pure data + pure functions — no
//! runtime, no framework. The registry hooks it up to surfaces in
//! [`crate::language::markdown`] and to files via
//! [`crate::language::document::PrismFile::schema`].

pub mod document_schema;
pub mod field_schema;
pub mod form_schema;
pub mod form_state;
pub mod markdown;
pub mod wiki_link;

pub use document_schema::{
    description_text_section, get_field, is_field_group_section, is_text_section,
    notes_text_section, ordered_field_ids, ordered_fields, DocumentSchema, FieldGroupSection,
    SectionDef, TextSection,
};
pub use field_schema::{FieldSchema, FieldType, SelectOption};
pub use form_schema::{
    ConditionalOperator, ConditionalRule, FieldCondition, FieldValidation, FormSchema,
    ValidationRule, ValidatorType,
};
pub use form_state::{reset_form_state, FormState};
pub use markdown::{
    extract_wiki_ids, inline_to_plain_text, parse_inline, parse_markdown, BlockToken, InlineToken,
};
pub use wiki_link::{
    build_wiki_link, detect_inline_link, extract_linked_ids, parse_wiki_links, render_wiki_links,
    WikiToken,
};
