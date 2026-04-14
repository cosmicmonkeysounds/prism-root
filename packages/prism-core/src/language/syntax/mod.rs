//! `language/syntax` — parser plumbing and LSP-like intelligence.
//!
//! Leaf pieces (AST types, scanner, token stream, case utils,
//! spell-check consumer types) land first; the expression-aware
//! [`syntax`] engine layer depends on `language/expression` but
//! only upward, so the import graph stays acyclic.

pub mod ast_types;
pub mod case_utils;
pub mod scanner;
pub mod spell_check_types;
#[allow(clippy::module_inception)]
pub mod syntax;
pub mod syntax_types;
pub mod token_stream;

pub use ast_types::{pos_at, range, Position, RootKind, RootNode, SourceRange, SyntaxNode};
pub use case_utils::{
    safe_identifier, to_camel_case, to_camel_ident, to_pascal_case, to_pascal_ident,
    to_screaming_snake, to_screaming_snake_ident, to_snake_case,
};
pub use scanner::{is_digit, is_ident_char, is_ident_start, ScanError, Scanner, ScannerState};
pub use spell_check_types::{
    PersonalDictionary, SpellCheckDiagnostic, SpellCheckOptions, SpellChecker, TokenContext,
    TokenFilter,
};
pub use syntax::{
    builtin_functions, create_expression_provider, create_syntax_engine, field_type_map,
    generate_luau_type_def, infer_node_type, DefaultSyntaxEngine, ExpressionProvider,
    BUILTIN_FUNCTIONS, FIELD_TYPE_MAP,
};
pub use syntax_types::{
    CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity, FieldTypeMapping,
    FunctionParam, FunctionSignature, HoverInfo, LuauTypeDef, SchemaContext, SyntaxEngine,
    SyntaxEngineOptions, SyntaxProvider, TextRange, TypeInfo,
};
pub use token_stream::{BaseToken, TokenError, TokenStream};
