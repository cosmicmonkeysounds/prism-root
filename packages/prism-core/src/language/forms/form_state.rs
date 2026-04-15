//! `FormState` — the per-edit-session state of a form: values,
//! errors, touched, dirty.
//!
//! Port of `language/forms/form-state.ts`. Pure data + a small set of
//! reducer-style helpers. The legacy TS version returned new
//! immutable objects on every mutation; the Rust port mutates the
//! owned state in place and lets callers clone when they need
//! snapshots, which is idiomatic and plays nicely with the §7
//! hot-reload `AppState` snapshot/restore cycle.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Per-edit-session form state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FormState {
    pub values: IndexMap<String, JsonValue>,
    pub errors: IndexMap<String, Vec<String>>,
    pub touched: IndexMap<String, bool>,
    pub dirty: IndexMap<String, bool>,
    #[serde(rename = "isSubmitting")]
    pub is_submitting: bool,
    #[serde(rename = "isValid")]
    pub is_valid: bool,
}

impl FormState {
    /// Create a fresh [`FormState`] seeded with default values.
    ///
    /// Mirrors the TS `createFormState(defaults)` signature: pass an
    /// empty map when there are no defaults.
    pub fn new(defaults: IndexMap<String, JsonValue>) -> Self {
        Self {
            values: defaults,
            errors: IndexMap::new(),
            touched: IndexMap::new(),
            dirty: IndexMap::new(),
            is_submitting: false,
            is_valid: true,
        }
    }

    /// Empty state with `is_valid = true`.
    pub fn empty() -> Self {
        Self::new(IndexMap::new())
    }

    /// Set a field value. `original_value` is used to compute the
    /// dirty flag — matches the TS semantics where dirty = current
    /// differs from the "as loaded" value.
    pub fn set_field_value(
        &mut self,
        field_id: impl Into<String>,
        value: JsonValue,
        original_value: &JsonValue,
    ) {
        let id = field_id.into();
        self.dirty.insert(id.clone(), value != *original_value);
        self.values.insert(id.clone(), value);
        self.touched.insert(id, true);
    }

    /// Replace the error list for one field. Recomputes `is_valid`.
    pub fn set_field_errors(&mut self, field_id: impl Into<String>, errors: Vec<String>) {
        self.errors.insert(field_id.into(), errors);
        self.is_valid = self.errors.values().all(|e| e.is_empty());
    }

    /// Mark a field as touched without changing its value.
    pub fn touch_field(&mut self, field_id: impl Into<String>) {
        self.touched.insert(field_id.into(), true);
    }

    /// Replace all errors at once. Recomputes `is_valid`.
    pub fn set_all_errors(&mut self, errors: IndexMap<String, Vec<String>>) {
        self.errors = errors;
        self.is_valid = self.errors.values().all(|e| e.is_empty());
    }

    /// Toggle the submitting flag.
    pub fn set_submitting(&mut self, is_submitting: bool) {
        self.is_submitting = is_submitting;
    }

    /// Any field dirty?
    pub fn is_dirty(&self) -> bool {
        self.dirty.values().any(|d| *d)
    }

    /// Returns `true` when every touched field is currently error-free.
    /// Matches the TS `isTouchedValid` predicate used by submit-gate
    /// logic.
    pub fn is_touched_valid(&self) -> bool {
        for (field_id, touched) in &self.touched {
            if *touched {
                let count = self.errors.get(field_id).map(Vec::len).unwrap_or(0);
                if count > 0 {
                    return false;
                }
            }
        }
        true
    }

    /// Errors for one field, or an empty slice if none are recorded.
    pub fn field_errors(&self, field_id: &str) -> &[String] {
        self.errors.get(field_id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// `true` iff the field is both touched and has at least one
    /// error.
    pub fn field_has_visible_error(&self, field_id: &str) -> bool {
        let touched = self.touched.get(field_id).copied().unwrap_or(false);
        touched && !self.field_errors(field_id).is_empty()
    }
}

/// Drop-in replacement for the legacy `resetFormState(defaults)` free
/// function. Returns a fresh state seeded with the given defaults.
pub fn reset_form_state(defaults: IndexMap<String, JsonValue>) -> FormState {
    FormState::new(defaults)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn defaults() -> IndexMap<String, JsonValue> {
        let mut map = IndexMap::new();
        map.insert("name".into(), json!(""));
        map.insert("count".into(), json!(0));
        map
    }

    #[test]
    fn new_seeds_values_and_is_valid() {
        let state = FormState::new(defaults());
        assert_eq!(state.values["name"], json!(""));
        assert!(state.is_valid);
        assert!(!state.is_submitting);
    }

    #[test]
    fn set_field_value_flags_touched_and_dirty_on_change() {
        let mut state = FormState::new(defaults());
        state.set_field_value("name", json!("Alice"), &json!(""));
        assert_eq!(state.values["name"], json!("Alice"));
        assert!(state.touched["name"]);
        assert!(state.dirty["name"]);
    }

    #[test]
    fn set_field_value_leaves_dirty_false_when_same() {
        let mut state = FormState::new(defaults());
        state.set_field_value("name", json!(""), &json!(""));
        assert!(!state.dirty["name"]);
        assert!(state.touched["name"]);
    }

    #[test]
    fn set_field_errors_updates_is_valid() {
        let mut state = FormState::new(defaults());
        state.set_field_errors("name", vec!["Required".into()]);
        assert!(!state.is_valid);
        state.set_field_errors("name", vec![]);
        assert!(state.is_valid);
    }

    #[test]
    fn set_all_errors_recomputes_is_valid() {
        let mut state = FormState::new(defaults());
        let mut errs = IndexMap::new();
        errs.insert("name".into(), vec!["x".into()]);
        errs.insert("count".into(), vec![]);
        state.set_all_errors(errs);
        assert!(!state.is_valid);
    }

    #[test]
    fn is_dirty_reports_any_dirty_field() {
        let mut state = FormState::new(defaults());
        assert!(!state.is_dirty());
        state.set_field_value("name", json!("Alice"), &json!(""));
        assert!(state.is_dirty());
    }

    #[test]
    fn is_touched_valid_short_circuits_on_touched_error() {
        let mut state = FormState::new(defaults());
        state.touch_field("name");
        state.set_field_errors("name", vec!["bad".into()]);
        assert!(!state.is_touched_valid());
    }

    #[test]
    fn field_has_visible_error_requires_both() {
        let mut state = FormState::new(defaults());
        state.set_field_errors("name", vec!["bad".into()]);
        assert!(!state.field_has_visible_error("name")); // not touched
        state.touch_field("name");
        assert!(state.field_has_visible_error("name"));
    }

    #[test]
    fn reset_form_state_replaces_everything() {
        let mut state = FormState::new(defaults());
        state.set_field_value("name", json!("Alice"), &json!(""));
        state.set_field_errors("name", vec!["bad".into()]);

        let fresh = reset_form_state(defaults());
        assert!(fresh.errors.is_empty());
        assert!(fresh.touched.is_empty());
        assert!(fresh.is_valid);
    }
}
