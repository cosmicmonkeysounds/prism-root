//! Spell-check consumer-side types.
//!
//! Port of `language/syntax/spell-check-types.ts`. The actual
//! spell-checker lives in a separate crate; these types are the
//! contract the editor extension depends on.

use std::collections::HashMap;

/// Context passed to token filters for skip decisions.
#[derive(Debug, Clone)]
pub struct TokenContext<'a> {
    pub line: &'a str,
    pub offset_in_line: usize,
    pub offset_in_doc: usize,
    pub syntax_type: Option<&'a str>,
}

/// Decides whether a word should be skipped during spell
/// checking. The checker evaluates every filter — if ANY filter
/// returns `true`, the word is skipped.
pub trait TokenFilter {
    fn id(&self) -> &str;
    fn label(&self) -> Option<&str> {
        None
    }
    fn should_skip(&self, word: &str, context: &TokenContext<'_>) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellCheckDiagnostic {
    pub word: String,
    pub from: usize,
    pub to: usize,
    pub suggestions: Vec<String>,
}

pub trait PersonalDictionary {
    fn is_known(&self, word: &str) -> bool;
}

#[derive(Default)]
pub struct SpellCheckOptions<'a> {
    pub syntax_types: Option<&'a HashMap<usize, String>>,
    pub filters: Option<&'a [Box<dyn TokenFilter>]>,
}

pub trait SpellChecker {
    fn is_loaded(&self) -> bool;
    fn personal(&self) -> Option<&dyn PersonalDictionary>;
    fn check_text(&self, text: &str, options: SpellCheckOptions<'_>) -> Vec<SpellCheckDiagnostic>;
    fn add_to_personal(&mut self, word: &str);
    fn ignore_word(&mut self, word: &str);
}
