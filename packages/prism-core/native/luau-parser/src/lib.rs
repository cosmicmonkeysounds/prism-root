//! Prism Luau parser — wasm-bindgen wrapper around full-moon.
//!
//! Four exports, all pure functions over a source string:
//!
//! - `parse(source)` — full AST as a Unist-compatible tree (`RootNode`)
//!   matching @prism/core/syntax `ast-types.ts`. Used by `LanguageDefinition.parse`.
//! - `findUiCalls(source)` — extracts every top-level `ui.<kind>(...)` call
//!   with its string/number args. Replaces the 270-line hand-rolled parser
//!   in `luau-facet-panel.tsx`.
//! - `findStatementLines(source)` — 1-based line numbers where each statement
//!   *starts*. Used by `luau-debugger.ts` to inject `__prism_trace(n)` correctly
//!   on multi-line statements and skip string/comment interiors.
//! - `validate(source)` — lightweight parse-only diagnostic list. Returns an
//!   empty array on success; each entry is `{ message, line, column }`.

use full_moon::{
    ast::{
        Ast, Block, Call, Expression, Field, FunctionCall, FunctionArgs, If, Index, LastStmt,
        Prefix, Stmt, Suffix, Var,
    },
    node::Node,
    tokenizer::{TokenReference, TokenType},
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

// ── Unist-compatible AST types (mirror ast-types.ts) ─────────────────────────

#[derive(Serialize)]
struct JsPosition {
    offset: u32,
    line: u32,
    column: u32,
}

#[derive(Serialize)]
struct JsSourceRange {
    start: JsPosition,
    end: JsPosition,
}

#[derive(Serialize)]
struct JsSyntaxNode {
    #[serde(rename = "type")]
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<JsSourceRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    children: Vec<JsSyntaxNode>,
}

impl JsSyntaxNode {
    fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            position: None,
            value: None,
            children: Vec::new(),
        }
    }

    fn with_position<N: Node>(mut self, node: &N) -> Self {
        if let Some((start, end)) = node.range() {
            self.position = Some(JsSourceRange {
                start: JsPosition {
                    offset: start.bytes() as u32,
                    line: start.line() as u32,
                    column: start.character().saturating_sub(1) as u32,
                },
                end: JsPosition {
                    offset: end.bytes() as u32,
                    line: end.line() as u32,
                    column: end.character().saturating_sub(1) as u32,
                },
            });
        }
        self
    }

    #[allow(dead_code)]
    fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    fn child(mut self, child: JsSyntaxNode) -> Self {
        self.children.push(child);
        self
    }
}

// ── Public JS API ────────────────────────────────────────────────────────────

/// Parse Luau source into a Unist-compatible RootNode.
#[wasm_bindgen]
pub fn parse(source: &str) -> Result<JsValue, JsValue> {
    let ast = full_moon::parse(source).map_err(stringify_errors)?;
    let root = build_root(&ast);
    serde_wasm_bindgen::to_value(&root).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Extract every `ui.<kind>(...)` call from the source, with literal args.
#[wasm_bindgen(js_name = findUiCalls)]
pub fn find_ui_calls(source: &str) -> Result<JsValue, JsValue> {
    let ast = full_moon::parse(source).map_err(stringify_errors)?;
    let mut out: Vec<UiCall> = Vec::new();
    collect_ui_calls(ast.nodes(), &mut out);
    serde_wasm_bindgen::to_value(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// For every statement in the source, return the 1-based line where it
/// *starts*. Used by luau-debugger to inject `__prism_trace(n)` without
/// breaking multi-line strings or statements.
#[wasm_bindgen(js_name = findStatementLines)]
pub fn find_statement_lines(source: &str) -> Result<Vec<u32>, JsValue> {
    let ast = full_moon::parse(source).map_err(stringify_errors)?;
    let mut out: Vec<u32> = Vec::new();
    collect_statement_lines(ast.nodes(), &mut out);
    Ok(out)
}

/// Lightweight parse-only diagnostics. Empty array on success.
#[wasm_bindgen]
pub fn validate(source: &str) -> Result<JsValue, JsValue> {
    let diagnostics: Vec<Diagnostic> = match full_moon::parse(source) {
        Ok(_) => Vec::new(),
        Err(errors) => errors
            .iter()
            .map(|e| {
                let (line, column) = error_position(e);
                Diagnostic {
                    message: e.to_string(),
                    line,
                    column,
                }
            })
            .collect(),
    };
    serde_wasm_bindgen::to_value(&diagnostics).map_err(|e| JsValue::from_str(&e.to_string()))
}

fn stringify_errors(errors: Vec<full_moon::Error>) -> JsValue {
    let msg = errors
        .iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("; ");
    JsValue::from_str(&msg)
}

// ── Serialization types exposed to JS ────────────────────────────────────────

#[derive(Serialize)]
struct Diagnostic {
    message: String,
    line: u32,
    column: u32,
}

#[derive(Serialize)]
struct UiCall {
    kind: String,
    args: Vec<UiArg>,
    line: u32,
    column: u32,
    /// Only populated for section/container calls that take a trailing
    /// table argument containing child `ui.*` calls.
    children: Vec<UiCall>,
}

#[derive(Serialize)]
struct UiArg {
    /// `None` for positional args, `Some("title")` for `{ title = "..." }` entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    value: String,
    #[serde(rename = "valueKind")]
    value_kind: UiArgKind,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum UiArgKind {
    String,
    Number,
    Bool,
    Identifier,
    Other,
}

// ── Internal: AST → Unist tree ───────────────────────────────────────────────

fn build_root(ast: &Ast) -> JsSyntaxNode {
    let mut root = JsSyntaxNode::new("root").with_position(ast.nodes());
    for stmt in ast.nodes().stmts() {
        root = root.child(build_stmt(stmt));
    }
    if let Some(last) = ast.nodes().last_stmt() {
        root = root.child(build_last_stmt(last));
    }
    root
}

fn build_stmt(stmt: &Stmt) -> JsSyntaxNode {
    let kind = stmt_kind(stmt);
    JsSyntaxNode::new(kind).with_position(stmt)
}

fn build_last_stmt(last: &LastStmt) -> JsSyntaxNode {
    let kind = match last {
        LastStmt::Break(_) => "BreakStatement",
        LastStmt::Return(_) => "ReturnStatement",
        LastStmt::Continue(_) => "ContinueStatement",
        _ => "LastStatement",
    };
    JsSyntaxNode::new(kind).with_position(last)
}

fn stmt_kind(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Assignment(_) => "Assignment",
        Stmt::Do(_) => "DoBlock",
        Stmt::FunctionCall(_) => "FunctionCallStatement",
        Stmt::FunctionDeclaration(_) => "FunctionDeclaration",
        Stmt::GenericFor(_) => "GenericFor",
        Stmt::If(_) => "IfStatement",
        Stmt::LocalAssignment(_) => "LocalAssignment",
        Stmt::LocalFunction(_) => "LocalFunction",
        Stmt::NumericFor(_) => "NumericFor",
        Stmt::Repeat(_) => "RepeatUntil",
        Stmt::While(_) => "WhileLoop",
        Stmt::CompoundAssignment(_) => "CompoundAssignment",
        Stmt::ExportedTypeDeclaration(_) => "ExportedTypeDeclaration",
        Stmt::TypeDeclaration(_) => "TypeDeclaration",
        _ => "Statement",
    }
}

// ── Internal: collect 1-based statement starting lines ──────────────────────

fn collect_statement_lines(block: &Block, out: &mut Vec<u32>) {
    for stmt in block.stmts() {
        if let Some((start, _)) = stmt.range() {
            out.push(start.line() as u32);
        }
        walk_stmt_for_lines(stmt, out);
    }
    if let Some(last) = block.last_stmt() {
        if let Some((start, _)) = last.range() {
            out.push(start.line() as u32);
        }
    }
}

fn walk_stmt_for_lines(stmt: &Stmt, out: &mut Vec<u32>) {
    match stmt {
        Stmt::Do(do_block) => collect_statement_lines(do_block.block(), out),
        Stmt::While(w) => collect_statement_lines(w.block(), out),
        Stmt::Repeat(r) => collect_statement_lines(r.block(), out),
        Stmt::If(if_stmt) => walk_if_for_lines(if_stmt, out),
        Stmt::NumericFor(f) => collect_statement_lines(f.block(), out),
        Stmt::GenericFor(f) => collect_statement_lines(f.block(), out),
        Stmt::FunctionDeclaration(f) => collect_statement_lines(f.body().block(), out),
        Stmt::LocalFunction(f) => collect_statement_lines(f.body().block(), out),
        _ => {}
    }
}

fn walk_if_for_lines(if_stmt: &If, out: &mut Vec<u32>) {
    collect_statement_lines(if_stmt.block(), out);
    if let Some(elseifs) = if_stmt.else_if() {
        for elseif in elseifs {
            collect_statement_lines(elseif.block(), out);
        }
    }
    if let Some(else_block) = if_stmt.else_block() {
        collect_statement_lines(else_block, out);
    }
}

// ── Internal: collect ui.* calls ─────────────────────────────────────────────

fn collect_ui_calls(block: &Block, out: &mut Vec<UiCall>) {
    for stmt in block.stmts() {
        if let Stmt::FunctionCall(call) = stmt {
            if let Some(ui_call) = try_extract_ui_call(call) {
                out.push(ui_call);
                continue;
            }
        }
        walk_stmt_for_ui_calls(stmt, out);
    }
}

fn walk_stmt_for_ui_calls(stmt: &Stmt, out: &mut Vec<UiCall>) {
    match stmt {
        Stmt::Do(b) => collect_ui_calls(b.block(), out),
        Stmt::While(w) => collect_ui_calls(w.block(), out),
        Stmt::Repeat(r) => collect_ui_calls(r.block(), out),
        Stmt::If(if_stmt) => walk_if_for_ui_calls(if_stmt, out),
        Stmt::NumericFor(f) => collect_ui_calls(f.block(), out),
        Stmt::GenericFor(f) => collect_ui_calls(f.block(), out),
        Stmt::FunctionDeclaration(f) => collect_ui_calls(f.body().block(), out),
        Stmt::LocalFunction(f) => collect_ui_calls(f.body().block(), out),
        _ => {}
    }
}

fn walk_if_for_ui_calls(if_stmt: &If, out: &mut Vec<UiCall>) {
    collect_ui_calls(if_stmt.block(), out);
    if let Some(elseifs) = if_stmt.else_if() {
        for elseif in elseifs {
            collect_ui_calls(elseif.block(), out);
        }
    }
    if let Some(else_block) = if_stmt.else_block() {
        collect_ui_calls(else_block, out);
    }
}

fn try_extract_ui_call(call: &FunctionCall) -> Option<UiCall> {
    // Prefix must be the identifier `ui`.
    let Prefix::Name(name_token) = call.prefix() else {
        return None;
    };
    if token_identifier(name_token)? != "ui" {
        return None;
    }

    // Walk the suffix chain looking for `.kind(...)`.
    let mut suffixes = call.suffixes();
    let first = suffixes.next()?;
    let kind = match first {
        Suffix::Index(idx) => extract_dot_name(idx)?,
        _ => return None,
    };

    let second = suffixes.next()?;
    let args = match second {
        Suffix::Call(Call::AnonymousCall(args)) => args,
        _ => return None,
    };

    let (line, column) = call
        .range()
        .map(|(s, _)| (s.line() as u32, s.character().saturating_sub(1) as u32))
        .unwrap_or((0, 0));

    let (parsed_args, nested_children) = parse_function_args(args);

    Some(UiCall {
        kind,
        args: parsed_args,
        line,
        column,
        children: nested_children,
    })
}

fn token_identifier(tr: &TokenReference) -> Option<String> {
    match tr.token_type() {
        TokenType::Identifier { identifier } => Some(identifier.to_string()),
        _ => None,
    }
}

fn extract_dot_name(idx: &Index) -> Option<String> {
    if let Index::Dot { name, .. } = idx {
        token_identifier(name)
    } else {
        None
    }
}

fn parse_function_args(args: &FunctionArgs) -> (Vec<UiArg>, Vec<UiCall>) {
    let mut parsed: Vec<UiArg> = Vec::new();
    let mut children: Vec<UiCall> = Vec::new();

    match args {
        FunctionArgs::Parentheses { arguments, .. } => {
            for expr in arguments.iter() {
                if let Some(arg) = expr_to_ui_arg(expr) {
                    parsed.push(arg);
                } else if let Expression::TableConstructor(table) = expr {
                    collect_table_fields(table, &mut parsed, &mut children);
                } else if let Expression::FunctionCall(call) = expr {
                    if let Some(nested) = try_extract_ui_call(call) {
                        children.push(nested);
                    }
                }
            }
        }
        FunctionArgs::String(tr) => {
            if let Some(val) = string_token_value(tr) {
                parsed.push(UiArg {
                    key: None,
                    value: val,
                    value_kind: UiArgKind::String,
                });
            }
        }
        FunctionArgs::TableConstructor(table) => {
            collect_table_fields(table, &mut parsed, &mut children);
        }
        _ => {}
    }

    (parsed, children)
}

fn collect_table_fields(
    table: &full_moon::ast::TableConstructor,
    args: &mut Vec<UiArg>,
    children: &mut Vec<UiCall>,
) {
    for field in table.fields() {
        match field {
            Field::NameKey { key, value, .. } => {
                let key_name = token_identifier(key);
                if let Some(arg) = expr_to_ui_arg_with_key(value, key_name.clone()) {
                    args.push(arg);
                } else if let Expression::FunctionCall(call) = value {
                    if let Some(nested) = try_extract_ui_call(call) {
                        children.push(nested);
                    }
                }
            }
            Field::NoKey(expr) => {
                if let Expression::FunctionCall(call) = expr {
                    if let Some(nested) = try_extract_ui_call(call) {
                        children.push(nested);
                        continue;
                    }
                }
                if let Some(arg) = expr_to_ui_arg(expr) {
                    args.push(arg);
                }
            }
            _ => {}
        }
    }
}

fn expr_to_ui_arg(expr: &Expression) -> Option<UiArg> {
    expr_to_ui_arg_with_key(expr, None)
}

fn expr_to_ui_arg_with_key(expr: &Expression, key: Option<String>) -> Option<UiArg> {
    match expr {
        Expression::String(tr) => string_token_value(tr).map(|v| UiArg {
            key,
            value: v,
            value_kind: UiArgKind::String,
        }),
        Expression::Number(tr) => Some(UiArg {
            key,
            value: tr.token().to_string(),
            value_kind: UiArgKind::Number,
        }),
        Expression::Symbol(tr) => {
            let text = tr.token().to_string();
            if text == "true" || text == "false" {
                Some(UiArg {
                    key,
                    value: text,
                    value_kind: UiArgKind::Bool,
                })
            } else {
                None
            }
        }
        Expression::Var(var) => match var {
            Var::Name(tr) => token_identifier(tr).map(|name| UiArg {
                key,
                value: name,
                value_kind: UiArgKind::Identifier,
            }),
            _ => Some(UiArg {
                key,
                value: var.to_string(),
                value_kind: UiArgKind::Other,
            }),
        },
        _ => None,
    }
}

fn string_token_value(tr: &TokenReference) -> Option<String> {
    if let TokenType::StringLiteral { literal, .. } = tr.token_type() {
        Some(literal.to_string())
    } else {
        None
    }
}

// ── Internal: error position extraction ──────────────────────────────────────

fn error_position(err: &full_moon::Error) -> (u32, u32) {
    match err {
        full_moon::Error::AstError(ast_err) => {
            let tr = ast_err.token();
            let pos = tr.start_position();
            (pos.line() as u32, pos.character().saturating_sub(1) as u32)
        }
        full_moon::Error::TokenizerError(tok_err) => {
            let pos = tok_err.position();
            (pos.line() as u32, pos.character().saturating_sub(1) as u32)
        }
    }
}

// ── Native tests (no wasm target needed) ─────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_native(source: &str) -> Ast {
        full_moon::parse(source).expect("parse should succeed")
    }

    #[test]
    fn finds_statement_lines_flat() {
        let ast = parse_native("local a = 1\nlocal b = 2\nprint(a + b)\n");
        let mut lines = Vec::new();
        collect_statement_lines(ast.nodes(), &mut lines);
        assert_eq!(lines, vec![1, 2, 3]);
    }

    #[test]
    fn finds_statement_lines_in_if_block() {
        let source = "if true then\n  local x = 1\nelse\n  local y = 2\nend\n";
        let ast = parse_native(source);
        let mut lines = Vec::new();
        collect_statement_lines(ast.nodes(), &mut lines);
        // outer `if` on line 1, inner `local x` on line 2, inner `local y` on line 4
        assert!(lines.contains(&1));
        assert!(lines.contains(&2));
        assert!(lines.contains(&4));
    }

    #[test]
    fn finds_statement_lines_across_multiline_string() {
        let source = "local s = [[\nhello\nworld\n]]\nprint(s)\n";
        let ast = parse_native(source);
        let mut lines = Vec::new();
        collect_statement_lines(ast.nodes(), &mut lines);
        // Exactly two statements — the multi-line string doesn't create phantom lines.
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], 1);
        assert_eq!(lines[1], 5);
    }

    #[test]
    fn extracts_ui_label_call() {
        let ast = parse_native(r#"ui.label("hello")"#);
        let mut out = Vec::new();
        collect_ui_calls(ast.nodes(), &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "label");
        assert_eq!(out[0].args.len(), 1);
        assert_eq!(out[0].args[0].value, "hello");
    }

    #[test]
    fn extracts_ui_call_with_table_props() {
        let source = r#"ui.button({ label = "Go", disabled = false })"#;
        let ast = parse_native(source);
        let mut out = Vec::new();
        collect_ui_calls(ast.nodes(), &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "button");
        let label = out[0]
            .args
            .iter()
            .find(|a| a.key.as_deref() == Some("label"));
        assert!(label.is_some());
        assert_eq!(label.unwrap().value, "Go");
    }

    #[test]
    fn extracts_ui_call_with_nested_children() {
        let source = r#"ui.section({ title = "Settings", ui.label("Hi"), ui.button("OK") })"#;
        let ast = parse_native(source);
        let mut out = Vec::new();
        collect_ui_calls(ast.nodes(), &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "section");
        assert_eq!(out[0].children.len(), 2);
        assert_eq!(out[0].children[0].kind, "label");
        assert_eq!(out[0].children[1].kind, "button");
    }

    #[test]
    fn validates_clean_source() {
        assert!(full_moon::parse("local x = 1").is_ok());
    }

    #[test]
    fn validates_syntax_error() {
        assert!(full_moon::parse("local x =").is_err());
    }
}
