//! `language/syntax` — LSP-like intelligence types.
//!
//! Port of `language/syntax/syntax-types.ts`. Diagnostics,
//! completions, hover info, schema context — all pure data
//! structures consumed by the [`super::syntax`] engine.

use std::collections::HashMap;

use super::super::expression::{AnyExprNode, ExprType};
use crate::foundation::object_model::types::{EntityFieldDef, EntityFieldType};

// ── Diagnostics ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub range: TextRange,
    pub code: Option<String>,
}

// ── Completions ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompletionKind {
    Field,
    Function,
    Keyword,
    Operator,
    Type,
    Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub sort_order: Option<u32>,
    pub replace_range: Option<TextRange>,
    pub insert_text: Option<String>,
}

// ── Hover ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct HoverInfo {
    pub range: TextRange,
    pub contents: String,
}

// ── Type inference ────────────────────────────────────────────────

pub type FieldTypeMapping = HashMap<EntityFieldType, ExprType>;

#[derive(Debug, Clone, PartialEq)]
pub struct TypeInfo {
    pub expr_type: ExprType,
    pub source: String,
}

// ── Schema context ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaContext {
    pub object_type: String,
    pub fields: Vec<EntityFieldDef>,
}

// ── Luau typedef ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct LuauTypeDef {
    pub object_type: String,
    pub content: String,
}

// ── Function signature ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionParam {
    pub name: &'static str,
    pub expr_type: ExprType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSignature {
    pub name: &'static str,
    pub params: Vec<FunctionParam>,
    pub return_type: ExprType,
    pub description: &'static str,
}

// ── SyntaxProvider ────────────────────────────────────────────────

/// A SyntaxProvider produces diagnostics, completions, and hover
/// info for a specific language or expression context.
pub trait SyntaxProvider {
    fn name(&self) -> &str;
    fn diagnose(&self, source: &str, context: Option<&SchemaContext>) -> Vec<Diagnostic>;
    fn complete(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Vec<CompletionItem>;
    fn hover(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Option<HoverInfo>;
}

// ── SyntaxEngine ──────────────────────────────────────────────────

#[derive(Default)]
pub struct SyntaxEngineOptions {
    pub providers: Vec<Box<dyn SyntaxProvider>>,
}

pub trait SyntaxEngine {
    fn get_provider(&self, name: &str) -> Option<&dyn SyntaxProvider>;
    fn list_providers(&self) -> Vec<String>;
    fn register_provider(&mut self, provider: Box<dyn SyntaxProvider>);
    fn diagnose(&self, source: &str, context: Option<&SchemaContext>) -> Vec<Diagnostic>;
    fn complete(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Vec<CompletionItem>;
    fn hover(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Option<HoverInfo>;
    fn infer_type(&self, node: &AnyExprNode, context: Option<&SchemaContext>) -> ExprType;
    fn validate_types(&self, source: &str, context: &SchemaContext) -> Vec<Diagnostic>;
    fn generate_luau_type_def(&self, context: &SchemaContext) -> LuauTypeDef;
    fn generate_luau_type_defs(&self, contexts: &[SchemaContext]) -> Vec<LuauTypeDef>;
}
