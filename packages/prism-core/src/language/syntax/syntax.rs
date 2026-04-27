//! Syntax Engine — LSP-like intelligence for the expression
//! layer.
//!
//! Port of `language/syntax/syntax.ts`. Provides diagnostics,
//! completions, hover info, and `.d.luau` type-def generation.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::super::expression::{
    parse, tokenize, AnyExprNode, BinaryOp, ExprError, ExprType, Token, TokenKind, UnaryOp,
};
use super::syntax_types::{
    CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity, FieldTypeMapping,
    FunctionParam, FunctionSignature, HoverInfo, LuauTypeDef, SchemaContext, SyntaxEngine,
    SyntaxEngineOptions, SyntaxProvider, TextRange,
};
use crate::foundation::object_model::types::{EntityFieldDef, EntityFieldType};

// ── Constants ─────────────────────────────────────────────────────

/// `EntityFieldType → ExprType` map, lazily built to dodge the
/// no-const-HashMap restriction.
pub fn field_type_map() -> &'static FieldTypeMapping {
    static MAP: OnceLock<FieldTypeMapping> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(EntityFieldType::Bool, ExprType::Boolean);
        m.insert(EntityFieldType::Int, ExprType::Number);
        m.insert(EntityFieldType::Float, ExprType::Number);
        m.insert(EntityFieldType::String, ExprType::String);
        m.insert(EntityFieldType::Text, ExprType::String);
        m.insert(EntityFieldType::Color, ExprType::String);
        m.insert(EntityFieldType::Enum, ExprType::String);
        m.insert(EntityFieldType::ObjectRef, ExprType::String);
        m.insert(EntityFieldType::Date, ExprType::String);
        m.insert(EntityFieldType::Datetime, ExprType::String);
        m.insert(EntityFieldType::Url, ExprType::String);
        m.insert(EntityFieldType::Lookup, ExprType::Unknown);
        m.insert(EntityFieldType::Rollup, ExprType::Number);
        m
    })
}

/// Back-compat alias so callers can say `FIELD_TYPE_MAP()`.
#[allow(non_snake_case)]
pub fn FIELD_TYPE_MAP() -> &'static FieldTypeMapping {
    field_type_map()
}

fn luau_type_name(field_type: EntityFieldType) -> &'static str {
    match field_type {
        EntityFieldType::Bool => "boolean",
        EntityFieldType::Int => "integer",
        EntityFieldType::Float => "number",
        EntityFieldType::String
        | EntityFieldType::Text
        | EntityFieldType::Color
        | EntityFieldType::Enum
        | EntityFieldType::ObjectRef
        | EntityFieldType::Date
        | EntityFieldType::Datetime
        | EntityFieldType::Url => "string",
        EntityFieldType::Lookup => "any",
        EntityFieldType::Rollup => "number",
        EntityFieldType::File => "string",
    }
}

/// Built-in function signatures, matching the legacy TS list.
pub fn builtin_functions() -> &'static [FunctionSignature] {
    static FNS: OnceLock<Vec<FunctionSignature>> = OnceLock::new();
    FNS.get_or_init(|| {
        let n = ExprType::Number;
        let s = ExprType::String;
        let u = ExprType::Unknown;
        let p = |name: &'static str, t: ExprType| FunctionParam { name, expr_type: t };
        vec![
            // Math
            FunctionSignature {
                name: "abs",
                params: vec![p("x", n)],
                return_type: n,
                description: "Absolute value",
            },
            FunctionSignature {
                name: "ceil",
                params: vec![p("x", n)],
                return_type: n,
                description: "Round up to nearest integer",
            },
            FunctionSignature {
                name: "floor",
                params: vec![p("x", n)],
                return_type: n,
                description: "Round down to nearest integer",
            },
            FunctionSignature {
                name: "round",
                params: vec![p("x", n)],
                return_type: n,
                description: "Round to nearest integer",
            },
            FunctionSignature {
                name: "sqrt",
                params: vec![p("x", n)],
                return_type: n,
                description: "Square root",
            },
            FunctionSignature {
                name: "pow",
                params: vec![p("base", n), p("exp", n)],
                return_type: n,
                description: "Raise base to exponent",
            },
            FunctionSignature {
                name: "min",
                params: vec![p("a", n), p("b", n)],
                return_type: n,
                description: "Minimum of two values",
            },
            FunctionSignature {
                name: "max",
                params: vec![p("a", n), p("b", n)],
                return_type: n,
                description: "Maximum of two values",
            },
            FunctionSignature {
                name: "clamp",
                params: vec![p("value", n), p("low", n), p("high", n)],
                return_type: n,
                description: "Clamp value between low and high",
            },
            // String
            FunctionSignature {
                name: "len",
                params: vec![p("s", s)],
                return_type: n,
                description: "Length of a string",
            },
            FunctionSignature {
                name: "lower",
                params: vec![p("s", s)],
                return_type: s,
                description: "Lowercase a string",
            },
            FunctionSignature {
                name: "upper",
                params: vec![p("s", s)],
                return_type: s,
                description: "Uppercase a string",
            },
            FunctionSignature {
                name: "trim",
                params: vec![p("s", s)],
                return_type: s,
                description: "Trim leading/trailing whitespace",
            },
            FunctionSignature {
                name: "concat",
                params: vec![p("...parts", s)],
                return_type: s,
                description: "Concatenate strings",
            },
            FunctionSignature {
                name: "left",
                params: vec![p("s", s), p("n", n)],
                return_type: s,
                description: "Leftmost n characters",
            },
            FunctionSignature {
                name: "right",
                params: vec![p("s", s), p("n", n)],
                return_type: s,
                description: "Rightmost n characters",
            },
            FunctionSignature {
                name: "mid",
                params: vec![p("s", s), p("start", n), p("len", n)],
                return_type: s,
                description: "Substring from start of length len",
            },
            FunctionSignature {
                name: "substitute",
                params: vec![p("s", s), p("old", s), p("new", s)],
                return_type: s,
                description: "Replace all occurrences of old with new",
            },
            // Date
            FunctionSignature {
                name: "today",
                params: vec![],
                return_type: s,
                description: "Today's date as YYYY-MM-DD",
            },
            FunctionSignature {
                name: "now",
                params: vec![],
                return_type: s,
                description: "Current ISO timestamp",
            },
            FunctionSignature {
                name: "year",
                params: vec![p("iso", s)],
                return_type: n,
                description: "Year component of an ISO date",
            },
            FunctionSignature {
                name: "month",
                params: vec![p("iso", s)],
                return_type: n,
                description: "Month component (1-12)",
            },
            FunctionSignature {
                name: "day",
                params: vec![p("iso", s)],
                return_type: n,
                description: "Day of month (1-31)",
            },
            FunctionSignature {
                name: "datediff",
                params: vec![p("a", s), p("b", s), p("unit", s)],
                return_type: n,
                description: "Difference between two dates (days/months/years)",
            },
            // Aggregate
            FunctionSignature {
                name: "sum",
                params: vec![p("...values", n)],
                return_type: n,
                description: "Sum of values",
            },
            FunctionSignature {
                name: "avg",
                params: vec![p("...values", n)],
                return_type: n,
                description: "Average of values",
            },
            FunctionSignature {
                name: "count",
                params: vec![p("...values", u)],
                return_type: n,
                description: "Count of arguments",
            },
        ]
    })
}

/// Back-compat alias so `BUILTIN_FUNCTIONS()` reads like the old
/// const.
#[allow(non_snake_case)]
pub fn BUILTIN_FUNCTIONS() -> &'static [FunctionSignature] {
    builtin_functions()
}

fn builtin_fn_map() -> &'static HashMap<&'static str, &'static FunctionSignature> {
    static MAP: OnceLock<HashMap<&'static str, &'static FunctionSignature>> = OnceLock::new();
    MAP.get_or_init(|| builtin_functions().iter().map(|f| (f.name, f)).collect())
}

const KEYWORDS: &[&str] = &["true", "false", "and", "or", "not"];

fn operator_completions() -> Vec<CompletionItem> {
    let make = |label: &str, detail: &str| CompletionItem {
        label: label.to_string(),
        kind: CompletionKind::Operator,
        detail: Some(detail.to_string()),
        documentation: None,
        sort_order: Some(10),
        replace_range: None,
        insert_text: None,
    };
    vec![
        make("+", "Addition / string concatenation"),
        make("-", "Subtraction"),
        make("*", "Multiplication"),
        make("/", "Division"),
        make("^", "Exponentiation"),
        make("%", "Modulo"),
        make("==", "Equal"),
        make("!=", "Not equal"),
        make("<", "Less than"),
        make("<=", "Less than or equal"),
        make(">", "Greater than"),
        make(">=", "Greater than or equal"),
        make("and", "Logical AND"),
        make("or", "Logical OR"),
    ]
}

// ── Helpers ───────────────────────────────────────────────────────

fn expr_error_to_diagnostic(err: &ExprError, source: &str) -> Diagnostic {
    let start = err.offset.unwrap_or(0);
    let end = (start + 1).min(source.len());
    Diagnostic {
        message: err.message.clone(),
        severity: DiagnosticSeverity::Error,
        range: TextRange { start, end },
        code: Some("parse-error".to_string()),
    }
}

fn field_to_expr_type(field_type: EntityFieldType) -> ExprType {
    field_type_map()
        .get(&field_type)
        .copied()
        .unwrap_or(ExprType::Unknown)
}

fn find_field_def<'a>(
    field_id: &str,
    context: Option<&'a SchemaContext>,
) -> Option<&'a EntityFieldDef> {
    context?.fields.iter().find(|f| f.id == field_id)
}

fn token_at_offset(tokens: &[Token], offset: usize) -> Option<&Token> {
    for t in tokens {
        if t.kind == TokenKind::Eof {
            continue;
        }
        if offset >= t.offset && offset < t.offset + t.raw.len() {
            return Some(t);
        }
    }
    None
}

fn token_touching_offset(tokens: &[Token], offset: usize) -> Option<&Token> {
    if let Some(t) = token_at_offset(tokens, offset) {
        return Some(t);
    }
    for t in tokens {
        if t.kind == TokenKind::Eof {
            continue;
        }
        if offset == t.offset + t.raw.len() {
            return Some(t);
        }
    }
    None
}

fn token_before_offset(tokens: &[Token], offset: usize) -> Option<&Token> {
    let mut prev: Option<&Token> = None;
    for t in tokens {
        if t.kind == TokenKind::Eof {
            break;
        }
        if t.offset + t.raw.len() > offset {
            break;
        }
        prev = Some(t);
    }
    prev
}

// ── Type inference ────────────────────────────────────────────────

pub fn infer_node_type(node: &AnyExprNode, context: Option<&SchemaContext>) -> ExprType {
    match node {
        AnyExprNode::Literal { expr_type, .. } => *expr_type,
        AnyExprNode::Operand { id, .. } => {
            if let Some(f) = find_field_def(id, context) {
                field_to_expr_type(f.field_type)
            } else {
                ExprType::Unknown
            }
        }
        AnyExprNode::Unary { op, .. } => match op {
            UnaryOp::Not => ExprType::Boolean,
            UnaryOp::Neg => ExprType::Number,
        },
        AnyExprNode::Binary { op, left, right } => match op {
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Lte
            | BinaryOp::Gt
            | BinaryOp::Gte
            | BinaryOp::And
            | BinaryOp::Or => ExprType::Boolean,
            BinaryOp::Add => {
                let lt = infer_node_type(left, context);
                let rt = infer_node_type(right, context);
                if lt == ExprType::String || rt == ExprType::String {
                    ExprType::String
                } else {
                    ExprType::Number
                }
            }
            _ => ExprType::Number,
        },
        AnyExprNode::Call { name, .. } => builtin_fn_map()
            .get(name.to_ascii_lowercase().as_str())
            .map(|s| s.return_type)
            .unwrap_or(ExprType::Unknown),
    }
}

// ── Type validation ───────────────────────────────────────────────

fn validate_node_types(
    node: &AnyExprNode,
    context: Option<&SchemaContext>,
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
) {
    match node {
        AnyExprNode::Operand { id, .. } => {
            let Some(ctx) = context else { return };
            if find_field_def(id, Some(ctx)).is_none() {
                let bracketed = format!("[field:{id}]");
                let idx = source.find(bracketed.as_str());
                let (start, length) = match idx {
                    Some(i) => (i, bracketed.len()),
                    None => (source.find(id.as_str()).unwrap_or(0), id.len()),
                };
                diagnostics.push(Diagnostic {
                    message: format!("Unknown field \"{id}\" for type \"{}\"", ctx.object_type),
                    severity: DiagnosticSeverity::Warning,
                    range: TextRange {
                        start,
                        end: start + length,
                    },
                    code: Some("unknown-field".to_string()),
                });
            }
        }
        AnyExprNode::Call { name, args } => {
            let lower = name.to_ascii_lowercase();
            let sig = builtin_fn_map().get(lower.as_str()).copied();
            if sig.is_none() {
                let idx = source.to_ascii_lowercase().find(&lower).unwrap_or(0);
                diagnostics.push(Diagnostic {
                    message: format!("Unknown function \"{name}\""),
                    severity: DiagnosticSeverity::Error,
                    range: TextRange {
                        start: idx,
                        end: idx + name.len(),
                    },
                    code: Some("unknown-function".to_string()),
                });
            } else if let Some(sig) = sig {
                if args.len() != sig.params.len() {
                    let idx = source.to_ascii_lowercase().find(&lower).unwrap_or(0);
                    diagnostics.push(Diagnostic {
                        message: format!(
                            "Function \"{name}\" expects {} argument(s), got {}",
                            sig.params.len(),
                            args.len()
                        ),
                        severity: DiagnosticSeverity::Error,
                        range: TextRange {
                            start: idx,
                            end: idx + name.len(),
                        },
                        code: Some("wrong-arity".to_string()),
                    });
                }
            }
            for arg in args {
                validate_node_types(arg, context, diagnostics, source);
            }
        }
        AnyExprNode::Binary { op, left, right } => {
            let lt = infer_node_type(left, context);
            let rt = infer_node_type(right, context);
            let arith = matches!(
                op,
                BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Pow | BinaryOp::Mod
            );
            if arith
                && lt != ExprType::Unknown
                && rt != ExprType::Unknown
                && (lt != ExprType::Number || rt != ExprType::Number)
            {
                diagnostics.push(Diagnostic {
                    message: format!(
                        "Operator \"{}\" expects number operands, got {:?} and {:?}",
                        op.symbol(),
                        lt,
                        rt
                    ),
                    severity: DiagnosticSeverity::Warning,
                    range: TextRange {
                        start: 0,
                        end: source.len(),
                    },
                    code: Some("type-mismatch".to_string()),
                });
            }
            validate_node_types(left, context, diagnostics, source);
            validate_node_types(right, context, diagnostics, source);
        }
        AnyExprNode::Unary { operand, .. } => {
            validate_node_types(operand, context, diagnostics, source);
        }
        AnyExprNode::Literal { .. } => {}
    }
}

// ── Expression Provider ───────────────────────────────────────────

pub struct ExpressionProvider;

pub fn create_expression_provider() -> Box<dyn SyntaxProvider> {
    Box::new(ExpressionProvider)
}

impl SyntaxProvider for ExpressionProvider {
    fn name(&self) -> &str {
        "expression"
    }

    fn diagnose(&self, source: &str, context: Option<&SchemaContext>) -> Vec<Diagnostic> {
        let result = parse(source);
        let mut diagnostics: Vec<Diagnostic> = result
            .errors
            .iter()
            .map(|e| expr_error_to_diagnostic(e, source))
            .collect();
        if let Some(node) = result.node {
            validate_node_types(&node, context, &mut diagnostics, source);
        }
        diagnostics
    }

    fn complete(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Vec<CompletionItem> {
        let tokens = tokenize(source);
        let mut items: Vec<CompletionItem> = Vec::new();

        let token_at = token_touching_offset(&tokens, offset);
        let token_before = token_before_offset(&tokens, offset);

        let prefix = match token_at {
            Some(t) if t.kind == TokenKind::Ident => source[t.offset..offset].to_ascii_lowercase(),
            Some(t) if t.kind == TokenKind::Operand => t
                .operand_data
                .as_ref()
                .map(|d| d.id.to_ascii_lowercase())
                .unwrap_or_default(),
            _ => String::new(),
        };

        let after_operator = matches!(
            token_before.map(|t| t.kind),
            None | Some(TokenKind::Plus)
                | Some(TokenKind::Minus)
                | Some(TokenKind::Star)
                | Some(TokenKind::Slash)
                | Some(TokenKind::Caret)
                | Some(TokenKind::Percent)
                | Some(TokenKind::Eq)
                | Some(TokenKind::Neq)
                | Some(TokenKind::Lt)
                | Some(TokenKind::Lte)
                | Some(TokenKind::Gt)
                | Some(TokenKind::Gte)
                | Some(TokenKind::And)
                | Some(TokenKind::Or)
                | Some(TokenKind::Not)
                | Some(TokenKind::LParen)
                | Some(TokenKind::Comma)
        );
        let after_value = matches!(
            token_before.map(|t| t.kind),
            Some(TokenKind::Number)
                | Some(TokenKind::String)
                | Some(TokenKind::Bool)
                | Some(TokenKind::RParen)
                | Some(TokenKind::Operand)
                | Some(TokenKind::Ident)
        );

        let at_kind = token_at.map(|t| t.kind);

        // Field completions
        if let Some(ctx) = context {
            if after_operator
                || at_kind == Some(TokenKind::Ident)
                || at_kind == Some(TokenKind::Operand)
            {
                for field in &ctx.fields {
                    if !prefix.is_empty() && !field.id.to_ascii_lowercase().starts_with(&prefix) {
                        continue;
                    }
                    let expr_type = field_to_expr_type(field.field_type);
                    items.push(CompletionItem {
                        label: format!("[field:{}]", field.id),
                        kind: CompletionKind::Field,
                        detail: Some(format!("{:?} → {:?}", field.field_type, expr_type)),
                        documentation: field
                            .description
                            .clone()
                            .or_else(|| field.label.clone())
                            .or_else(|| Some(field.id.clone())),
                        sort_order: Some(1),
                        replace_range: None,
                        insert_text: Some(format!("[field:{}]", field.id)),
                    });
                }
            }
        }

        // Function completions
        if after_operator || at_kind == Some(TokenKind::Ident) {
            for fn_ in builtin_functions() {
                if !prefix.is_empty() && !fn_.name.starts_with(prefix.as_str()) {
                    continue;
                }
                let param_str = fn_
                    .params
                    .iter()
                    .map(|p| format!("{}: {:?}", p.name, p.expr_type))
                    .collect::<Vec<_>>()
                    .join(", ");
                items.push(CompletionItem {
                    label: fn_.name.to_string(),
                    kind: CompletionKind::Function,
                    detail: Some(format!("({param_str}) → {:?}", fn_.return_type)),
                    documentation: Some(fn_.description.to_string()),
                    sort_order: Some(5),
                    replace_range: None,
                    insert_text: Some(format!("{}(", fn_.name)),
                });
            }
        }

        // Keywords
        if after_operator || after_value || at_kind == Some(TokenKind::Ident) {
            for kw in KEYWORDS {
                if !prefix.is_empty() && !kw.starts_with(prefix.as_str()) {
                    continue;
                }
                let kind = if *kw == "and" || *kw == "or" || *kw == "not" {
                    CompletionKind::Keyword
                } else {
                    CompletionKind::Value
                };
                let detail = if *kw == "true" || *kw == "false" {
                    "boolean literal".to_string()
                } else {
                    format!("logical {}", kw.to_ascii_uppercase())
                };
                items.push(CompletionItem {
                    label: (*kw).to_string(),
                    kind,
                    detail: Some(detail),
                    documentation: None,
                    sort_order: Some(if *kw == "and" || *kw == "or" || *kw == "not" {
                        8
                    } else {
                        6
                    }),
                    replace_range: None,
                    insert_text: None,
                });
            }
        }

        // Operators after a value
        if after_value {
            for op in operator_completions() {
                items.push(op);
            }
        }

        items
    }

    fn hover(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Option<HoverInfo> {
        let tokens = tokenize(source);
        let token = token_at_offset(&tokens, offset)?;
        let range = TextRange {
            start: token.offset,
            end: token.offset + token.raw.len(),
        };

        // Operand
        if token.kind == TokenKind::Operand {
            if let Some(data) = token.operand_data.as_ref() {
                let field_id = &data.id;
                if let Some(field) = find_field_def(field_id, context) {
                    let expr_type = field_to_expr_type(field.field_type);
                    let mut lines = vec![format!(
                        "**{}** ({:?} → {:?})",
                        field.label.as_deref().unwrap_or(&field.id),
                        field.field_type,
                        expr_type
                    )];
                    if let Some(d) = &field.description {
                        lines.push(d.clone());
                    }
                    if field.required.unwrap_or(false) {
                        lines.push("Required".to_string());
                    }
                    if let Some(expr) = &field.expression {
                        lines.push(format!("Computed: `{expr}`"));
                    }
                    if let Some(opts) = &field.enum_options {
                        lines.push(format!(
                            "Values: {}",
                            opts.iter()
                                .map(|o| o.value.clone())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    return Some(HoverInfo {
                        range,
                        contents: lines.join("\n"),
                    });
                }
                return Some(HoverInfo {
                    range,
                    contents: format!("Field: {field_id} (unknown)"),
                });
            }
        }

        if token.kind == TokenKind::Ident {
            let name = &token.raw;
            let lower = name.to_ascii_lowercase();

            // Function-call?
            let next = tokens
                .iter()
                .skip_while(|t| !std::ptr::eq(*t, token))
                .nth(1);
            if let Some(n) = next {
                if n.kind == TokenKind::LParen {
                    if let Some(sig) = builtin_fn_map().get(lower.as_str()) {
                        let param_str = sig
                            .params
                            .iter()
                            .map(|p| format!("{}: {:?}", p.name, p.expr_type))
                            .collect::<Vec<_>>()
                            .join(", ");
                        return Some(HoverInfo {
                            range,
                            contents: format!(
                                "**{}**({param_str}) → {:?}\n{}",
                                sig.name, sig.return_type, sig.description
                            ),
                        });
                    }
                    return Some(HoverInfo {
                        range,
                        contents: format!("Unknown function: {name}"),
                    });
                }
            }

            // Bare field?
            if let Some(field) = find_field_def(name, context) {
                let expr_type = field_to_expr_type(field.field_type);
                let mut contents = format!(
                    "**{}** ({:?} → {:?})",
                    field.label.as_deref().unwrap_or(&field.id),
                    field.field_type,
                    expr_type
                );
                if let Some(d) = &field.description {
                    contents.push('\n');
                    contents.push_str(d);
                }
                return Some(HoverInfo { range, contents });
            }

            if lower == "true" || lower == "false" {
                return Some(HoverInfo {
                    range,
                    contents: format!("Boolean literal: {lower}"),
                });
            }
            if lower == "and" || lower == "or" || lower == "not" {
                return Some(HoverInfo {
                    range,
                    contents: format!("Logical operator: {}", lower.to_ascii_uppercase()),
                });
            }
            return None;
        }

        if token.kind == TokenKind::Number {
            return Some(HoverInfo {
                range,
                contents: format!("Number literal: {}", token.number_value.unwrap_or(0.0)),
            });
        }
        if token.kind == TokenKind::String {
            return Some(HoverInfo {
                range,
                contents: format!("String literal: {}", token.raw),
            });
        }
        if token.kind == TokenKind::Bool {
            return Some(HoverInfo {
                range,
                contents: format!("Boolean literal: {}", token.bool_value.unwrap_or(false)),
            });
        }
        if token.kind == TokenKind::And {
            return Some(HoverInfo {
                range,
                contents: "Logical operator: AND\nShort-circuit: returns left if falsy, else right"
                    .to_string(),
            });
        }
        if token.kind == TokenKind::Or {
            return Some(HoverInfo {
                range,
                contents: "Logical operator: OR\nShort-circuit: returns left if truthy, else right"
                    .to_string(),
            });
        }
        if token.kind == TokenKind::Not {
            return Some(HoverInfo {
                range,
                contents: "Logical operator: NOT\nNegates a boolean value".to_string(),
            });
        }

        None
    }
}

// ── Luau typedef generator ────────────────────────────────────────

pub fn generate_luau_type_def(context: &SchemaContext) -> LuauTypeDef {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("--- Type definitions for {}", context.object_type));
    lines.push("--- Generated by Prism Syntax Engine".to_string());
    lines.push(String::new());
    lines.push(format!("---@class {}", context.object_type));

    for field in &context.fields {
        let luau_type = luau_type_name(field.field_type);
        let desc = field
            .description
            .as_deref()
            .or(field.label.as_deref())
            .unwrap_or(field.id.as_str());
        if let Some(opts) = &field.enum_options {
            let values = opts
                .iter()
                .map(|o| format!("\"{}\"", o.value))
                .collect::<Vec<_>>()
                .join("|");
            lines.push(format!("---@field {} {values} {desc}", field.id));
        } else {
            let optional = if field.required.unwrap_or(false) {
                ""
            } else {
                "?"
            };
            lines.push(format!(
                "---@field {}{optional} {luau_type} {desc}",
                field.id
            ));
        }
    }

    lines.push(String::new());
    lines.push("--- Standard GraphObject fields available on all objects".to_string());
    lines.push("---@field id string Unique object ID".to_string());
    lines.push("---@field type string Object type name".to_string());
    lines.push("---@field name string Display name".to_string());
    lines.push("---@field parentId string|nil Parent object ID".to_string());
    lines.push("---@field status string|nil Current status".to_string());
    lines.push("---@field tags string[] Tags".to_string());
    lines.push("---@field description string Description text".to_string());
    lines.push("---@field createdAt string ISO-8601 creation timestamp".to_string());
    lines.push("---@field updatedAt string ISO-8601 last update timestamp".to_string());
    lines.push(String::new());

    lines.push("--- Built-in expression functions".to_string());
    for fn_ in builtin_functions() {
        for p in &fn_.params {
            lines.push(format!("---@param {} number", p.name));
        }
        lines.push("---@return number".to_string());
        let param_names = fn_
            .params
            .iter()
            .map(|p| p.name.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("function {}({param_names}) end", fn_.name));
        lines.push(String::new());
    }

    LuauTypeDef {
        object_type: context.object_type.clone(),
        content: lines.join("\n"),
    }
}

// ── Syntax Engine implementation ──────────────────────────────────

pub struct DefaultSyntaxEngine {
    providers: HashMap<String, Box<dyn SyntaxProvider>>,
}

pub fn create_syntax_engine(options: SyntaxEngineOptions) -> Box<dyn SyntaxEngine> {
    let mut engine = DefaultSyntaxEngine {
        providers: HashMap::new(),
    };
    let expr = create_expression_provider();
    engine.providers.insert(expr.name().to_string(), expr);
    for p in options.providers {
        engine.providers.insert(p.name().to_string(), p);
    }
    Box::new(engine)
}

impl SyntaxEngine for DefaultSyntaxEngine {
    fn get_provider(&self, name: &str) -> Option<&dyn SyntaxProvider> {
        self.providers.get(name).map(|p| p.as_ref())
    }

    fn list_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    fn register_provider(&mut self, provider: Box<dyn SyntaxProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    fn diagnose(&self, source: &str, context: Option<&SchemaContext>) -> Vec<Diagnostic> {
        match self.providers.get("expression") {
            Some(p) => p.diagnose(source, context),
            None => Vec::new(),
        }
    }

    fn complete(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Vec<CompletionItem> {
        match self.providers.get("expression") {
            Some(p) => p.complete(source, offset, context),
            None => Vec::new(),
        }
    }

    fn hover(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Option<HoverInfo> {
        self.providers
            .get("expression")?
            .hover(source, offset, context)
    }

    fn infer_type(&self, node: &AnyExprNode, context: Option<&SchemaContext>) -> ExprType {
        infer_node_type(node, context)
    }

    fn validate_types(&self, source: &str, context: &SchemaContext) -> Vec<Diagnostic> {
        let result = parse(source);
        let Some(node) = result.node else {
            return Vec::new();
        };
        let mut diagnostics = Vec::new();
        validate_node_types(&node, Some(context), &mut diagnostics, source);
        diagnostics
    }

    fn generate_luau_type_def(&self, context: &SchemaContext) -> LuauTypeDef {
        generate_luau_type_def(context)
    }

    fn generate_luau_type_defs(&self, contexts: &[SchemaContext]) -> Vec<LuauTypeDef> {
        contexts.iter().map(generate_luau_type_def).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::{EntityFieldDef, EntityFieldType};

    fn field(id: &str, field_type: EntityFieldType) -> EntityFieldDef {
        EntityFieldDef {
            id: id.to_string(),
            field_type,
            label: None,
            description: None,
            required: None,
            default: None,
            expression: None,
            enum_options: None,
            ref_types: None,
            lookup_relation: None,
            lookup_field: None,
            rollup_relation: None,
            rollup_field: None,
            rollup_function: None,
            ui: None,
        }
    }

    #[test]
    fn diagnose_reports_parse_errors() {
        let engine = create_syntax_engine(SyntaxEngineOptions::default());
        let diags = engine.diagnose("(1 +", None);
        assert!(!diags.is_empty());
    }

    #[test]
    fn diagnose_flags_unknown_field() {
        let engine = create_syntax_engine(SyntaxEngineOptions::default());
        let ctx = SchemaContext {
            object_type: "Task".into(),
            fields: vec![field("priority", EntityFieldType::Int)],
            signals: vec![],
        };
        let diags = engine.diagnose("[field:missing] + 1", Some(&ctx));
        assert!(diags
            .iter()
            .any(|d| d.code.as_deref() == Some("unknown-field")));
    }

    #[test]
    fn diagnose_flags_wrong_arity() {
        let engine = create_syntax_engine(SyntaxEngineOptions::default());
        let diags = engine.diagnose("abs(1, 2)", None);
        assert!(diags
            .iter()
            .any(|d| d.code.as_deref() == Some("wrong-arity")));
    }

    #[test]
    fn diagnose_flags_unknown_function() {
        let engine = create_syntax_engine(SyntaxEngineOptions::default());
        let diags = engine.diagnose("frobnicate(1)", None);
        assert!(diags
            .iter()
            .any(|d| d.code.as_deref() == Some("unknown-function")));
    }

    #[test]
    fn generate_luau_type_def_contains_fields_and_funcs() {
        let ctx = SchemaContext {
            object_type: "Task".into(),
            fields: vec![field("priority", EntityFieldType::Int)],
            signals: vec![],
        };
        let def = generate_luau_type_def(&ctx);
        assert!(def.content.contains("---@class Task"));
        assert!(def.content.contains("---@field priority? integer"));
        assert!(def.content.contains("function abs"));
    }

    #[test]
    fn infer_type_on_binary_string_concat() {
        let engine = create_syntax_engine(SyntaxEngineOptions::default());
        let res = parse("'a' + 'b'");
        let node = res.node.unwrap();
        assert_eq!(engine.infer_type(&node, None), ExprType::String);
    }
}
