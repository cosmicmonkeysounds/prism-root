//! `language/expression` — formula / lookup / rollup engine.
//!
//! Port of `packages/prism-core/src/language/expression/*`. The
//! scanner and parser are fully standalone; the evaluator depends
//! on them plus `chrono` for ISO-date builtins. The field
//! resolver lives on top and plugs into the object-model stores.

pub mod evaluator;
pub mod expression_types;
pub mod field_resolver;
pub mod parser;
pub mod scanner;

pub use evaluator::{
    evaluate, evaluate_expression, wrap_bare_identifiers, ContextStore, EvaluateResult,
};
pub use expression_types::{
    format_number, AnyExprNode, BinaryOp, ExprError, ExprType, ExprValue, ParseResult, UnaryOp,
    ValueStore,
};
pub use field_resolver::{
    aggregate, build_formula_context, read_object_field, resolve_computed_field,
    resolve_formula_field, resolve_lookup_field, resolve_rollup_field, EdgeLookup,
    FieldResolverStores, ObjectLookup,
};
pub use parser::parse;
pub use scanner::{tokenize, OperandData, Token, TokenKind};
