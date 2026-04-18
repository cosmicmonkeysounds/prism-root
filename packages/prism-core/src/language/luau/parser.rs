//! Luau parser — `full_moon` AST → prism-core `RootNode` conversion.
//!
//! Parses Luau source via `full_moon::parse` and converts the lossless
//! AST into the shared `SyntaxNode` / `RootNode` tree that the unified
//! language registry consumes.

use crate::language::syntax::{pos_at, RootNode, SourceRange, SyntaxNode};
use full_moon::ast::{self, Block, LastStmt, Stmt};
use full_moon::tokenizer::TokenReference;
use serde_json::Value as JsonValue;

pub fn parse_luau(source: &str) -> RootNode {
    let lua_version = full_moon::LuaVersion::luau();
    let result = full_moon::parse_fallible(source, lua_version);
    let ast = result.into_result();

    match ast {
        Ok(ast) => {
            let children = convert_block(source, ast.nodes());
            RootNode {
                kind: Default::default(),
                position: Some(SourceRange {
                    start: pos_at(source, 0),
                    end: pos_at(source, source.len()),
                }),
                children,
            }
        }
        Err(errors) => {
            let children = errors
                .into_iter()
                .map(|e| SyntaxNode {
                    kind: "error".into(),
                    value: Some(e.to_string()),
                    ..Default::default()
                })
                .collect();
            RootNode {
                kind: Default::default(),
                position: Some(SourceRange {
                    start: pos_at(source, 0),
                    end: pos_at(source, source.len()),
                }),
                children,
            }
        }
    }
}

fn convert_block(source: &str, block: &Block) -> Vec<SyntaxNode> {
    let mut children = Vec::new();
    for stmt in block.stmts() {
        children.push(convert_stmt(source, stmt));
    }
    if let Some(last) = block.last_stmt() {
        children.push(convert_last_stmt(source, last));
    }
    children
}

fn convert_stmt(source: &str, stmt: &Stmt) -> SyntaxNode {
    match stmt {
        Stmt::Assignment(assign) => {
            let mut node = node_from_tokens(source, "assignment", stmt);
            for var in assign.variables().iter() {
                node.children.push(SyntaxNode {
                    kind: "variable".into(),
                    value: Some(var.to_string().trim().to_string()),
                    ..Default::default()
                });
            }
            for expr in assign.expressions().iter() {
                node.children.push(convert_expression(source, expr));
            }
            node
        }
        Stmt::LocalAssignment(local) => {
            let mut node = node_from_tokens(source, "local_assignment", stmt);
            for name in local.names().iter() {
                node.children.push(SyntaxNode {
                    kind: "name".into(),
                    value: Some(name.to_string().trim().to_string()),
                    ..Default::default()
                });
            }
            for expr in local.expressions().iter() {
                node.children.push(convert_expression(source, expr));
            }
            node
        }
        Stmt::FunctionCall(call) => {
            let mut node = node_from_tokens(source, "function_call", stmt);
            node.value = Some(prefix_name(call.prefix()));
            node
        }
        Stmt::FunctionDeclaration(decl) => {
            let mut node = node_from_tokens(source, "function_declaration", stmt);
            node.value = Some(decl.name().to_string().trim().to_string());
            node.children = convert_block(source, decl.body().block());
            node
        }
        Stmt::LocalFunction(local_fn) => {
            let mut node = node_from_tokens(source, "local_function", stmt);
            node.value = Some(local_fn.name().to_string().trim().to_string());
            node.children = convert_block(source, local_fn.body().block());
            node
        }
        Stmt::If(if_stmt) => {
            let mut node = node_from_tokens(source, "if", stmt);
            node.children
                .push(convert_expression(source, if_stmt.condition()));
            let mut then_block = SyntaxNode {
                kind: "then_block".into(),
                children: convert_block(source, if_stmt.block()),
                ..Default::default()
            };
            node.children.push(then_block);
            if let Some(else_ifs) = if_stmt.else_if() {
                for clause in else_ifs {
                    let mut elif = SyntaxNode {
                        kind: "elseif_block".into(),
                        ..Default::default()
                    };
                    elif.children
                        .push(convert_expression(source, clause.condition()));
                    elif.children.extend(convert_block(source, clause.block()));
                    node.children.push(elif);
                }
            }
            if let Some(else_block) = if_stmt.else_block() {
                then_block = SyntaxNode {
                    kind: "else_block".into(),
                    children: convert_block(source, else_block),
                    ..Default::default()
                };
                node.children.push(then_block);
            }
            node
        }
        Stmt::While(while_stmt) => {
            let mut node = node_from_tokens(source, "while", stmt);
            node.children
                .push(convert_expression(source, while_stmt.condition()));
            node.children
                .extend(convert_block(source, while_stmt.block()));
            node
        }
        Stmt::Repeat(repeat) => {
            let mut node = node_from_tokens(source, "repeat", stmt);
            node.children.extend(convert_block(source, repeat.block()));
            node.children
                .push(convert_expression(source, repeat.until()));
            node
        }
        Stmt::NumericFor(nfor) => {
            let mut node = node_from_tokens(source, "numeric_for", stmt);
            node.value = Some(nfor.index_variable().to_string().trim().to_string());
            node.children.push(convert_expression(source, nfor.start()));
            node.children.push(convert_expression(source, nfor.end()));
            if let Some(step) = nfor.step() {
                node.children.push(convert_expression(source, step));
            }
            node.children.extend(convert_block(source, nfor.block()));
            node
        }
        Stmt::GenericFor(gfor) => {
            let mut node = node_from_tokens(source, "generic_for", stmt);
            let names: Vec<String> = gfor
                .names()
                .iter()
                .map(|n| n.to_string().trim().to_string())
                .collect();
            node.data.insert("names".into(), JsonValue::from(names));
            for expr in gfor.expressions().iter() {
                node.children.push(convert_expression(source, expr));
            }
            node.children.extend(convert_block(source, gfor.block()));
            node
        }
        Stmt::Do(do_stmt) => {
            let mut node = node_from_tokens(source, "do", stmt);
            node.children.extend(convert_block(source, do_stmt.block()));
            node
        }
        Stmt::CompoundAssignment(compound) => {
            let mut node = node_from_tokens(source, "compound_assignment", stmt);
            node.value = Some(compound.lhs().to_string().trim().to_string());
            node.data.insert(
                "operator".into(),
                JsonValue::String(compound.compound_operator().to_string().trim().to_string()),
            );
            node.children
                .push(convert_expression(source, compound.rhs()));
            node
        }
        Stmt::TypeDeclaration(type_decl) => {
            let mut node = node_from_tokens(source, "type_declaration", stmt);
            node.value = Some(type_decl.type_name().to_string().trim().to_string());
            node
        }
        _ => node_from_tokens(source, "unknown_statement", stmt),
    }
}

fn convert_last_stmt(source: &str, last: &LastStmt) -> SyntaxNode {
    match last {
        LastStmt::Return(ret) => {
            let mut node = SyntaxNode {
                kind: "return".into(),
                ..Default::default()
            };
            for expr in ret.returns().iter() {
                node.children.push(convert_expression(source, expr));
            }
            node
        }
        LastStmt::Break(token) => SyntaxNode {
            kind: "break".into(),
            position: token_position(source, token),
            ..Default::default()
        },
        LastStmt::Continue(token) => SyntaxNode {
            kind: "continue".into(),
            position: token_position(source, token),
            ..Default::default()
        },
        _ => SyntaxNode {
            kind: "unknown_last".into(),
            ..Default::default()
        },
    }
}

fn convert_expression(source: &str, expr: &ast::Expression) -> SyntaxNode {
    match expr {
        ast::Expression::Number(token) => SyntaxNode {
            kind: "number".into(),
            value: Some(token.to_string().trim().to_string()),
            position: token_position(source, token),
            ..Default::default()
        },
        ast::Expression::String(token) => SyntaxNode {
            kind: "string".into(),
            value: Some(token.to_string().trim().to_string()),
            position: token_position(source, token),
            ..Default::default()
        },
        ast::Expression::Symbol(token) => {
            let text = token.to_string().trim().to_string();
            SyntaxNode {
                kind: match text.as_str() {
                    "true" | "false" => "boolean",
                    "nil" => "nil",
                    _ => "symbol",
                }
                .into(),
                value: Some(text),
                position: token_position(source, token),
                ..Default::default()
            }
        }
        ast::Expression::Var(var) => {
            let name = match var {
                ast::Var::Name(token) => token.to_string().trim().to_string(),
                ast::Var::Expression(expr) => expr.to_string().trim().to_string(),
                _ => "?".into(),
            };
            SyntaxNode {
                kind: "variable_ref".into(),
                value: Some(name),
                ..Default::default()
            }
        }
        ast::Expression::BinaryOperator { lhs, binop, rhs } => {
            let mut node = SyntaxNode {
                kind: "binary_op".into(),
                ..Default::default()
            };
            node.data.insert(
                "operator".into(),
                JsonValue::String(binop.to_string().trim().to_string()),
            );
            node.children.push(convert_expression(source, lhs));
            node.children.push(convert_expression(source, rhs));
            node
        }
        ast::Expression::UnaryOperator { unop, expression } => {
            let mut node = SyntaxNode {
                kind: "unary_op".into(),
                ..Default::default()
            };
            node.data.insert(
                "operator".into(),
                JsonValue::String(unop.to_string().trim().to_string()),
            );
            node.children.push(convert_expression(source, expression));
            node
        }
        ast::Expression::Parentheses { expression, .. } => {
            let mut node = SyntaxNode {
                kind: "parenthesized".into(),
                ..Default::default()
            };
            node.children.push(convert_expression(source, expression));
            node
        }
        ast::Expression::FunctionCall(call) => {
            let mut node = SyntaxNode {
                kind: "call_expression".into(),
                ..Default::default()
            };
            node.value = Some(prefix_name(call.prefix()));
            node
        }
        ast::Expression::TableConstructor(table) => {
            let mut node = SyntaxNode {
                kind: "table_constructor".into(),
                ..Default::default()
            };
            for field in table.fields().iter() {
                node.children.push(convert_table_field(source, field));
            }
            node
        }
        ast::Expression::Function(anon_fn) => {
            let mut node = SyntaxNode {
                kind: "anonymous_function".into(),
                ..Default::default()
            };
            node.children = convert_block(source, anon_fn.body().block());
            node
        }
        ast::Expression::IfExpression(if_expr) => {
            let mut node = SyntaxNode {
                kind: "if_expression".into(),
                ..Default::default()
            };
            node.children
                .push(convert_expression(source, if_expr.condition()));
            node.children
                .push(convert_expression(source, if_expr.if_expression()));
            node.children
                .push(convert_expression(source, if_expr.else_expression()));
            node
        }
        ast::Expression::InterpolatedString(interp) => {
            let mut node = SyntaxNode {
                kind: "interpolated_string".into(),
                ..Default::default()
            };
            for segment in interp.segments() {
                node.children
                    .push(convert_expression(source, &segment.expression));
            }
            node
        }
        ast::Expression::TypeAssertion { expression, .. } => {
            let mut node = SyntaxNode {
                kind: "type_assertion".into(),
                ..Default::default()
            };
            node.children.push(convert_expression(source, expression));
            node
        }
        _ => SyntaxNode {
            kind: "unknown_expression".into(),
            ..Default::default()
        },
    }
}

fn convert_table_field(source: &str, field: &ast::Field) -> SyntaxNode {
    match field {
        ast::Field::NameKey { key, value, .. } => {
            let mut node = SyntaxNode {
                kind: "table_field_name".into(),
                value: Some(key.to_string().trim().to_string()),
                ..Default::default()
            };
            node.children.push(convert_expression(source, value));
            node
        }
        ast::Field::ExpressionKey { key, value, .. } => {
            let mut node = SyntaxNode {
                kind: "table_field_expr".into(),
                ..Default::default()
            };
            node.children.push(convert_expression(source, key));
            node.children.push(convert_expression(source, value));
            node
        }
        ast::Field::NoKey(expr) => {
            let mut node = SyntaxNode {
                kind: "table_field_seq".into(),
                ..Default::default()
            };
            node.children.push(convert_expression(source, expr));
            node
        }
        _ => SyntaxNode {
            kind: "unknown_field".into(),
            ..Default::default()
        },
    }
}

fn prefix_name(prefix: &ast::Prefix) -> String {
    match prefix {
        ast::Prefix::Name(token) => token.to_string().trim().to_string(),
        ast::Prefix::Expression(expr) => expr.to_string().trim().to_string(),
        _ => "?".into(),
    }
}

fn token_position(source: &str, token: &TokenReference) -> Option<SourceRange> {
    let start = token.start_position();
    let end = token.end_position();
    let start_offset = offset_from_position(source, start.line(), start.character());
    let end_offset = offset_from_position(source, end.line(), end.character());
    Some(SourceRange {
        start: pos_at(source, start_offset),
        end: pos_at(source, end_offset),
    })
}

fn offset_from_position(source: &str, line: usize, col: usize) -> usize {
    let mut current_line = 1;
    let mut offset = 0;
    for (i, ch) in source.char_indices() {
        if current_line == line {
            let col_offset = i + col.saturating_sub(1);
            return col_offset.min(source.len());
        }
        if ch == '\n' {
            current_line += 1;
        }
        offset = i + ch.len_utf8();
    }
    offset.min(source.len())
}

fn node_from_tokens(_source: &str, kind: &str, stmt: &Stmt) -> SyntaxNode {
    let text = stmt.to_string();
    SyntaxNode {
        kind: kind.into(),
        value: if text.len() <= 120 {
            Some(text.trim().to_string())
        } else {
            None
        },
        ..Default::default()
    }
}

pub fn parse_errors(source: &str) -> Vec<String> {
    let lua_version = full_moon::LuaVersion::luau();
    let result = full_moon::parse_fallible(source, lua_version);
    match result.into_result() {
        Ok(_) => Vec::new(),
        Err(errors) => errors.into_iter().map(|e| e.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_return() {
        let root = parse_luau("return 1 + 2");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].kind, "return");
        assert_eq!(root.children[0].children.len(), 1);
        assert_eq!(root.children[0].children[0].kind, "binary_op");
    }

    #[test]
    fn parse_local_assignment() {
        let root = parse_luau("local x = 42");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].kind, "local_assignment");
    }

    #[test]
    fn parse_function_declaration() {
        let root = parse_luau("function greet(name)\n  return name\nend");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].kind, "function_declaration");
        assert_eq!(root.children[0].value.as_deref(), Some("greet"));
        assert!(!root.children[0].children.is_empty());
    }

    #[test]
    fn parse_if_statement() {
        let root = parse_luau("if x > 0 then\n  return 1\nelse\n  return 0\nend");
        assert_eq!(root.children[0].kind, "if");
        let if_node = &root.children[0];
        assert!(if_node.children.iter().any(|c| c.kind == "then_block"));
        assert!(if_node.children.iter().any(|c| c.kind == "else_block"));
    }

    #[test]
    fn parse_for_loop() {
        let root = parse_luau("for i = 1, 10 do\n  print(i)\nend");
        assert_eq!(root.children[0].kind, "numeric_for");
        assert_eq!(root.children[0].value.as_deref(), Some("i"));
    }

    #[test]
    fn parse_table_constructor() {
        let root = parse_luau("local t = { a = 1, b = 2 }");
        let assign = &root.children[0];
        assert_eq!(assign.kind, "local_assignment");
        let table = assign
            .children
            .iter()
            .find(|c| c.kind == "table_constructor");
        assert!(table.is_some());
    }

    #[test]
    fn parse_errors_reported() {
        let root = parse_luau("if then end");
        assert!(root.children.iter().any(|c| c.kind == "error"));
    }

    #[test]
    fn parse_errors_fn_returns_messages() {
        let errors = parse_errors("if then end");
        assert!(!errors.is_empty());
    }

    #[test]
    fn parse_errors_fn_clean_source() {
        let errors = parse_errors("return 42");
        assert!(errors.is_empty());
    }

    #[test]
    fn parse_luau_type_annotation() {
        let root = parse_luau("local x: number = 42");
        assert_eq!(root.children[0].kind, "local_assignment");
    }

    #[test]
    fn parse_compound_assignment() {
        let root = parse_luau("x += 1");
        assert_eq!(root.children[0].kind, "compound_assignment");
    }

    #[test]
    fn parse_interpolated_string() {
        let root = parse_luau("local s = `hello {name}`");
        let assign = &root.children[0];
        let interp = assign
            .children
            .iter()
            .find(|c| c.kind == "interpolated_string");
        assert!(interp.is_some());
    }

    #[test]
    fn parse_anonymous_function() {
        let root = parse_luau("local f = function(x) return x end");
        let assign = &root.children[0];
        let anon = assign
            .children
            .iter()
            .find(|c| c.kind == "anonymous_function");
        assert!(anon.is_some());
    }

    #[test]
    fn parse_method_call() {
        let root = parse_luau("obj:method(1, 2)");
        assert_eq!(root.children[0].kind, "function_call");
    }
}
