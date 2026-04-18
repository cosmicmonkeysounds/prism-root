//! `LuauVisualLanguage` — bidirectional Luau ↔ ScriptGraph bridge.
//!
//! Implements the `VisualLanguage` trait for Luau, enabling both
//! `decompile` (Luau source → ScriptGraph) and `compile`
//! (ScriptGraph → Luau source).

use crate::language::syntax::{Diagnostic, DiagnosticSeverity, TextRange};
use crate::language::visual::bridge::{VisualLanguage, VisualLanguageError};
use crate::language::visual::graph::*;

use full_moon::ast::{self, Block, LastStmt, Stmt};

fn diag_error(message: String) -> Diagnostic {
    Diagnostic {
        message,
        severity: DiagnosticSeverity::Error,
        range: TextRange { start: 0, end: 0 },
        code: None,
    }
}

pub struct LuauVisualLanguage;

impl LuauVisualLanguage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LuauVisualLanguage {
    fn default() -> Self {
        Self::new()
    }
}

// ── VisualLanguage impl ───────────────────────────────────────────────

impl VisualLanguage for LuauVisualLanguage {
    fn language_id(&self) -> &str {
        "prism:luau"
    }

    fn decompile(&self, source: &str) -> Result<ScriptGraph, VisualLanguageError> {
        let lua_version = full_moon::LuaVersion::luau();
        let result = full_moon::parse_fallible(source, lua_version);
        let ast = result.into_result().map_err(|errors| VisualLanguageError {
            message: "parse error".into(),
            diagnostics: errors.iter().map(|e| diag_error(e.to_string())).collect(),
        })?;

        let mut ctx = DecompileCtx::new();
        let mut graph = ScriptGraph::new("luau-graph", "Luau Script");

        let entry = ScriptNode::new("entry", ScriptNodeKind::Entry, "Entry")
            .with_port(exec_output("exec_out"));
        graph.add_node(entry);

        let block_ids = ctx.decompile_block(&mut graph, ast.nodes());

        if let Some(first_id) = block_ids.first() {
            graph.add_edge(ScriptEdge {
                id: ctx.next_id("edge"),
                from_node: "entry".into(),
                from_port: "exec_out".into(),
                to_node: first_id.clone(),
                to_port: "exec_in".into(),
            });
        }

        for window in block_ids.windows(2) {
            graph.add_edge(ScriptEdge {
                id: ctx.next_id("edge"),
                from_node: window[0].clone(),
                from_port: "exec_out".into(),
                to_node: window[1].clone(),
                to_port: "exec_in".into(),
            });
        }

        auto_layout(&mut graph);
        Ok(graph)
    }

    fn compile(&self, graph: &ScriptGraph) -> Result<String, VisualLanguageError> {
        let sorted = graph.topological_sort();
        let mut lines = Vec::new();

        for node_id in &sorted {
            if let Some(node) = graph.get_node(node_id) {
                if let Some(line) = compile_node(graph, node) {
                    lines.push(line);
                }
            }
        }

        Ok(lines.join("\n"))
    }

    fn node_palette(&self) -> Vec<NodeKindDef> {
        vec![
            palette_entry(
                ScriptNodeKind::LocalAssignment,
                "Local Variable",
                "Declare a local variable",
                "Variables",
            ),
            palette_entry(
                ScriptNodeKind::Assignment,
                "Assignment",
                "Assign a value to a variable",
                "Variables",
            ),
            palette_entry(
                ScriptNodeKind::FunctionCall,
                "Function Call",
                "Call a function",
                "Actions",
            ),
            palette_entry(
                ScriptNodeKind::Branch,
                "If / Else",
                "Conditional branch",
                "Control Flow",
            ),
            palette_entry(
                ScriptNodeKind::Loop,
                "Loop",
                "Repeat a block",
                "Control Flow",
            ),
            palette_entry(
                ScriptNodeKind::Return,
                "Return",
                "Return a value",
                "Control Flow",
            ),
            palette_entry(
                ScriptNodeKind::FunctionDef,
                "Function",
                "Define a function",
                "Declarations",
            ),
            palette_entry(
                ScriptNodeKind::Literal,
                "Literal",
                "A constant value (number, string, boolean)",
                "Data",
            ),
            palette_entry(
                ScriptNodeKind::BinaryOp,
                "Binary Op",
                "Math or comparison operator",
                "Data",
            ),
            palette_entry(
                ScriptNodeKind::TableConstructor,
                "Table",
                "Create a table",
                "Data",
            ),
            palette_entry(
                ScriptNodeKind::DaemonCommand,
                "Daemon Command",
                "Invoke a daemon command",
                "Prism",
            ),
            palette_entry(
                ScriptNodeKind::CrdtRead,
                "CRDT Read",
                "Read from CRDT store",
                "Prism",
            ),
            palette_entry(
                ScriptNodeKind::CrdtWrite,
                "CRDT Write",
                "Write to CRDT store",
                "Prism",
            ),
        ]
    }

    fn validate(&self, graph: &ScriptGraph) -> Vec<Diagnostic> {
        let mut diags = Vec::new();

        let structural = graph.validate();
        for msg in structural {
            diags.push(diag_error(msg));
        }

        let has_entry = graph.nodes.iter().any(|n| n.kind == ScriptNodeKind::Entry);
        if !has_entry {
            diags.push(diag_error("graph has no Entry node".into()));
        }

        for node in &graph.nodes {
            if node.kind.is_statement() && node.kind != ScriptNodeKind::Entry {
                let has_exec_in = graph.edges.iter().any(|e| {
                    e.to_node == node.id
                        && node
                            .ports
                            .iter()
                            .any(|p| p.id == e.to_port && p.kind == PortKind::Execution)
                });
                if !has_exec_in && node.exec_in().is_some() {
                    diags.push(diag_error(format!(
                        "node '{}' has no incoming execution edge",
                        node.label
                    )));
                }
            }
        }

        diags
    }
}

// ── Decompile (Luau → Graph) ──────────────────────────────────────────

struct DecompileCtx {
    counter: u64,
}

impl DecompileCtx {
    fn new() -> Self {
        Self { counter: 0 }
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}_{}", self.counter)
    }

    fn decompile_block(&mut self, graph: &mut ScriptGraph, block: &Block) -> Vec<String> {
        let mut node_ids = Vec::new();

        for stmt in block.stmts() {
            if let Some(id) = self.decompile_stmt(graph, stmt) {
                node_ids.push(id);
            }
        }

        if let Some(last) = block.last_stmt() {
            if let Some(id) = self.decompile_last_stmt(graph, last) {
                node_ids.push(id);
            }
        }

        node_ids
    }

    fn decompile_stmt(&mut self, graph: &mut ScriptGraph, stmt: &Stmt) -> Option<String> {
        match stmt {
            Stmt::LocalAssignment(local) => {
                let id = self.next_id("local");
                let names: Vec<String> = local
                    .names()
                    .iter()
                    .map(|n| n.to_string().trim().to_string())
                    .collect();
                let label = format!("local {}", names.join(", "));
                let mut node = ScriptNode::new(&id, ScriptNodeKind::LocalAssignment, &label)
                    .with_port(exec_input("exec_in"))
                    .with_port(exec_output("exec_out"));

                for (i, expr) in local.expressions().iter().enumerate() {
                    let expr_id = self.decompile_expression(graph, expr);
                    node = node.with_port(data_input(format!("value_{i}"), "value", DataType::Any));
                    if let Some(eid) = expr_id {
                        graph.add_edge(ScriptEdge {
                            id: self.next_id("edge"),
                            from_node: eid,
                            from_port: "result".into(),
                            to_node: id.clone(),
                            to_port: format!("value_{i}"),
                        });
                    }
                }

                for (i, name) in names.iter().enumerate() {
                    node = node.with_port(data_output(
                        format!("out_{i}"),
                        name.as_str(),
                        DataType::Any,
                    ));
                }

                graph.add_node(node);
                Some(id)
            }
            Stmt::Assignment(assign) => {
                let id = self.next_id("assign");
                let vars: Vec<String> = assign
                    .variables()
                    .iter()
                    .map(|v| v.to_string().trim().to_string())
                    .collect();
                let label = format!("{} = ...", vars.join(", "));
                let mut node = ScriptNode::new(&id, ScriptNodeKind::Assignment, &label)
                    .with_port(exec_input("exec_in"))
                    .with_port(exec_output("exec_out"));

                for (i, expr) in assign.expressions().iter().enumerate() {
                    let expr_id = self.decompile_expression(graph, expr);
                    node = node.with_port(data_input(format!("value_{i}"), "value", DataType::Any));
                    if let Some(eid) = expr_id {
                        graph.add_edge(ScriptEdge {
                            id: self.next_id("edge"),
                            from_node: eid,
                            from_port: "result".into(),
                            to_node: id.clone(),
                            to_port: format!("value_{i}"),
                        });
                    }
                }

                graph.add_node(node);
                Some(id)
            }
            Stmt::FunctionCall(call) => {
                let id = self.next_id("call");
                let name = prefix_name(call.prefix());
                let node = ScriptNode::new(&id, ScriptNodeKind::FunctionCall, &name)
                    .with_port(exec_input("exec_in"))
                    .with_port(exec_output("exec_out"))
                    .with_port(data_output("result", "result", DataType::Any))
                    .with_property(
                        "source",
                        serde_json::Value::String(call.to_string().trim().to_string()),
                    );
                graph.add_node(node);
                Some(id)
            }
            Stmt::FunctionDeclaration(decl) => {
                let id = self.next_id("fn");
                let name = decl.name().to_string().trim().to_string();
                let node =
                    ScriptNode::new(&id, ScriptNodeKind::FunctionDef, format!("function {name}"))
                        .with_port(exec_input("exec_in"))
                        .with_port(exec_output("exec_out"))
                        .with_property("name", serde_json::Value::String(name))
                        .with_property(
                            "body",
                            serde_json::Value::String(
                                decl.body().block().to_string().trim().to_string(),
                            ),
                        );
                graph.add_node(node);
                Some(id)
            }
            Stmt::LocalFunction(local_fn) => {
                let id = self.next_id("local_fn");
                let name = local_fn.name().to_string().trim().to_string();
                let node = ScriptNode::new(
                    &id,
                    ScriptNodeKind::FunctionDef,
                    format!("local function {name}"),
                )
                .with_port(exec_input("exec_in"))
                .with_port(exec_output("exec_out"))
                .with_property("name", serde_json::Value::String(name))
                .with_property("local", serde_json::Value::Bool(true))
                .with_property(
                    "body",
                    serde_json::Value::String(
                        local_fn.body().block().to_string().trim().to_string(),
                    ),
                );
                graph.add_node(node);
                Some(id)
            }
            Stmt::If(if_stmt) => {
                let id = self.next_id("if");
                let cond_id = self.decompile_expression(graph, if_stmt.condition());
                let mut node = ScriptNode::new(&id, ScriptNodeKind::Branch, "if")
                    .with_port(exec_input("exec_in"))
                    .with_port(exec_output("exec_out"))
                    .with_port(data_input("condition", "condition", DataType::Boolean))
                    .with_port(exec_output("then_out"))
                    .with_port(exec_output("else_out"));

                if let Some(cid) = cond_id {
                    graph.add_edge(ScriptEdge {
                        id: self.next_id("edge"),
                        from_node: cid,
                        from_port: "result".into(),
                        to_node: id.clone(),
                        to_port: "condition".into(),
                    });
                }

                node = node.with_property(
                    "source",
                    serde_json::Value::String(if_stmt.to_string().trim().to_string()),
                );

                graph.add_node(node);
                Some(id)
            }
            Stmt::While(while_stmt) => {
                let id = self.next_id("while");
                let cond_id = self.decompile_expression(graph, while_stmt.condition());
                let mut node = ScriptNode::new(&id, ScriptNodeKind::Loop, "while")
                    .with_port(exec_input("exec_in"))
                    .with_port(exec_output("exec_out"))
                    .with_port(data_input("condition", "condition", DataType::Boolean))
                    .with_port(exec_output("body_out"));

                if let Some(cid) = cond_id {
                    graph.add_edge(ScriptEdge {
                        id: self.next_id("edge"),
                        from_node: cid,
                        from_port: "result".into(),
                        to_node: id.clone(),
                        to_port: "condition".into(),
                    });
                }

                node = node.with_property(
                    "source",
                    serde_json::Value::String(while_stmt.to_string().trim().to_string()),
                );

                graph.add_node(node);
                Some(id)
            }
            Stmt::NumericFor(nfor) => {
                let id = self.next_id("for");
                let var_name = nfor.index_variable().to_string().trim().to_string();
                let node = ScriptNode::new(&id, ScriptNodeKind::Loop, format!("for {var_name}"))
                    .with_port(exec_input("exec_in"))
                    .with_port(exec_output("exec_out"))
                    .with_port(exec_output("body_out"))
                    .with_port(data_output("index", &var_name, DataType::Number))
                    .with_property(
                        "source",
                        serde_json::Value::String(nfor.to_string().trim().to_string()),
                    );
                graph.add_node(node);
                Some(id)
            }
            _ => {
                let id = self.next_id("stmt");
                let source = stmt.to_string().trim().to_string();
                let label = if source.len() > 40 {
                    format!("{}...", &source[..37])
                } else {
                    source.clone()
                };
                let node = ScriptNode::new(&id, ScriptNodeKind::Custom("statement".into()), &label)
                    .with_port(exec_input("exec_in"))
                    .with_port(exec_output("exec_out"))
                    .with_property("source", serde_json::Value::String(source));
                graph.add_node(node);
                Some(id)
            }
        }
    }

    fn decompile_last_stmt(&mut self, graph: &mut ScriptGraph, last: &LastStmt) -> Option<String> {
        match last {
            LastStmt::Return(ret) => {
                let id = self.next_id("return");
                let mut node = ScriptNode::new(&id, ScriptNodeKind::Return, "return")
                    .with_port(exec_input("exec_in"));

                for (i, expr) in ret.returns().iter().enumerate() {
                    let expr_id = self.decompile_expression(graph, expr);
                    node = node.with_port(data_input(format!("value_{i}"), "value", DataType::Any));
                    if let Some(eid) = expr_id {
                        graph.add_edge(ScriptEdge {
                            id: self.next_id("edge"),
                            from_node: eid,
                            from_port: "result".into(),
                            to_node: id.clone(),
                            to_port: format!("value_{i}"),
                        });
                    }
                }

                graph.add_node(node);
                Some(id)
            }
            LastStmt::Break(_) => {
                let id = self.next_id("break");
                let node = ScriptNode::new(&id, ScriptNodeKind::Custom("break".into()), "break")
                    .with_port(exec_input("exec_in"));
                graph.add_node(node);
                Some(id)
            }
            LastStmt::Continue(_) => {
                let id = self.next_id("continue");
                let node =
                    ScriptNode::new(&id, ScriptNodeKind::Custom("continue".into()), "continue")
                        .with_port(exec_input("exec_in"));
                graph.add_node(node);
                Some(id)
            }
            _ => None,
        }
    }

    fn decompile_expression(
        &mut self,
        graph: &mut ScriptGraph,
        expr: &ast::Expression,
    ) -> Option<String> {
        match expr {
            ast::Expression::Number(token) => {
                let id = self.next_id("num");
                let value = token.to_string().trim().to_string();
                let node = ScriptNode::new(&id, ScriptNodeKind::Literal, &value)
                    .with_port(data_output("result", "value", DataType::Number))
                    .with_property("value", serde_json::Value::String(value));
                graph.add_node(node);
                Some(id)
            }
            ast::Expression::String(token) => {
                let id = self.next_id("str");
                let value = token.to_string().trim().to_string();
                let node = ScriptNode::new(&id, ScriptNodeKind::Literal, &value)
                    .with_port(data_output("result", "value", DataType::String))
                    .with_property("value", serde_json::Value::String(value));
                graph.add_node(node);
                Some(id)
            }
            ast::Expression::Symbol(token) => {
                let id = self.next_id("sym");
                let value = token.to_string().trim().to_string();
                let dt = match value.as_str() {
                    "true" | "false" => DataType::Boolean,
                    "nil" => DataType::Nil,
                    _ => DataType::Any,
                };
                let node = ScriptNode::new(&id, ScriptNodeKind::Literal, &value)
                    .with_port(data_output("result", "value", dt))
                    .with_property("value", serde_json::Value::String(value));
                graph.add_node(node);
                Some(id)
            }
            ast::Expression::Var(var) => {
                let id = self.next_id("var");
                let name = match var {
                    ast::Var::Name(token) => token.to_string().trim().to_string(),
                    ast::Var::Expression(expr) => expr.to_string().trim().to_string(),
                    _ => "?".into(),
                };
                let node = ScriptNode::new(&id, ScriptNodeKind::Variable, &name)
                    .with_port(data_output("result", &name, DataType::Any))
                    .with_property("name", serde_json::Value::String(name));
                graph.add_node(node);
                Some(id)
            }
            ast::Expression::BinaryOperator { lhs, binop, rhs } => {
                let id = self.next_id("binop");
                let op = binop.to_string().trim().to_string();
                let lhs_id = self.decompile_expression(graph, lhs);
                let rhs_id = self.decompile_expression(graph, rhs);

                let node = ScriptNode::new(&id, ScriptNodeKind::BinaryOp, &op)
                    .with_port(data_input("lhs", "left", DataType::Any))
                    .with_port(data_input("rhs", "right", DataType::Any))
                    .with_port(data_output("result", "result", DataType::Any))
                    .with_property("operator", serde_json::Value::String(op));

                graph.add_node(node);

                if let Some(lid) = lhs_id {
                    graph.add_edge(ScriptEdge {
                        id: self.next_id("edge"),
                        from_node: lid,
                        from_port: "result".into(),
                        to_node: id.clone(),
                        to_port: "lhs".into(),
                    });
                }
                if let Some(rid) = rhs_id {
                    graph.add_edge(ScriptEdge {
                        id: self.next_id("edge"),
                        from_node: rid,
                        from_port: "result".into(),
                        to_node: id.clone(),
                        to_port: "rhs".into(),
                    });
                }

                Some(id)
            }
            ast::Expression::UnaryOperator { unop, expression } => {
                let id = self.next_id("unop");
                let op = unop.to_string().trim().to_string();
                let inner_id = self.decompile_expression(graph, expression);

                let node = ScriptNode::new(&id, ScriptNodeKind::UnaryOp, &op)
                    .with_port(data_input("operand", "operand", DataType::Any))
                    .with_port(data_output("result", "result", DataType::Any))
                    .with_property("operator", serde_json::Value::String(op));

                graph.add_node(node);

                if let Some(iid) = inner_id {
                    graph.add_edge(ScriptEdge {
                        id: self.next_id("edge"),
                        from_node: iid,
                        from_port: "result".into(),
                        to_node: id.clone(),
                        to_port: "operand".into(),
                    });
                }

                Some(id)
            }
            ast::Expression::FunctionCall(call) => {
                let id = self.next_id("call_expr");
                let name = prefix_name(call.prefix());
                let node = ScriptNode::new(&id, ScriptNodeKind::FunctionCall, &name)
                    .with_port(data_output("result", "result", DataType::Any))
                    .with_property(
                        "source",
                        serde_json::Value::String(call.to_string().trim().to_string()),
                    );
                graph.add_node(node);
                Some(id)
            }
            ast::Expression::Parentheses { expression, .. } => {
                self.decompile_expression(graph, expression)
            }
            ast::Expression::TableConstructor(_) => {
                let id = self.next_id("table");
                let node = ScriptNode::new(&id, ScriptNodeKind::TableConstructor, "{ ... }")
                    .with_port(data_output("result", "table", DataType::Table))
                    .with_property(
                        "source",
                        serde_json::Value::String(expr.to_string().trim().to_string()),
                    );
                graph.add_node(node);
                Some(id)
            }
            ast::Expression::Function(_) => {
                let id = self.next_id("anon_fn");
                let node = ScriptNode::new(&id, ScriptNodeKind::FunctionDef, "function(...)")
                    .with_port(data_output("result", "function", DataType::Function))
                    .with_property(
                        "source",
                        serde_json::Value::String(expr.to_string().trim().to_string()),
                    );
                graph.add_node(node);
                Some(id)
            }
            _ => {
                let id = self.next_id("expr");
                let source = expr.to_string().trim().to_string();
                let node =
                    ScriptNode::new(&id, ScriptNodeKind::Custom("expression".into()), &source)
                        .with_port(data_output("result", "result", DataType::Any))
                        .with_property("source", serde_json::Value::String(source));
                graph.add_node(node);
                Some(id)
            }
        }
    }
}

fn prefix_name(prefix: &ast::Prefix) -> String {
    match prefix {
        ast::Prefix::Name(token) => token.to_string().trim().to_string(),
        ast::Prefix::Expression(expr) => expr.to_string().trim().to_string(),
        _ => "?".into(),
    }
}

// ── Compile (Graph → Luau) ────────────────────────────────────────────

fn compile_node(graph: &ScriptGraph, node: &ScriptNode) -> Option<String> {
    match &node.kind {
        ScriptNodeKind::Entry => None,
        ScriptNodeKind::LocalAssignment => {
            let label = &node.label;
            let values = compile_data_inputs(graph, node);
            if values.is_empty() {
                Some(label.to_string())
            } else {
                let name_part = label.strip_prefix("local ").unwrap_or(label);
                Some(format!("local {} = {}", name_part, values.join(", ")))
            }
        }
        ScriptNodeKind::Assignment => {
            let label = &node.label;
            let values = compile_data_inputs(graph, node);
            let var_part = label.strip_suffix(" = ...").unwrap_or(label);
            if values.is_empty() {
                Some(format!("{var_part} = nil"))
            } else {
                Some(format!("{var_part} = {}", values.join(", ")))
            }
        }
        ScriptNodeKind::Return => {
            let values = compile_data_inputs(graph, node);
            if values.is_empty() {
                Some("return".into())
            } else {
                Some(format!("return {}", values.join(", ")))
            }
        }
        ScriptNodeKind::FunctionCall => {
            if let Some(source) = node.properties.get("source") {
                Some(source.as_str().unwrap_or(&node.label).to_string())
            } else {
                Some(format!("{}()", node.label))
            }
        }
        ScriptNodeKind::FunctionDef => {
            let name = node
                .properties
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("f");
            let is_local = node
                .properties
                .get("local")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let body = node
                .properties
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let prefix = if is_local {
                "local function"
            } else {
                "function"
            };
            Some(format!("{prefix} {name}()\n{body}\nend"))
        }
        ScriptNodeKind::Branch => {
            if let Some(source) = node.properties.get("source") {
                Some(source.as_str().unwrap_or("if ... then\nend").to_string())
            } else {
                let cond = compile_data_inputs(graph, node);
                let cond_str = cond.first().map(|s| s.as_str()).unwrap_or("true");
                Some(format!("if {cond_str} then\nend"))
            }
        }
        ScriptNodeKind::Loop => {
            if let Some(source) = node.properties.get("source") {
                Some(source.as_str().unwrap_or("while true do\nend").to_string())
            } else {
                Some("while true do\nend".into())
            }
        }
        ScriptNodeKind::Custom(kind) => match kind.as_str() {
            "break" => Some("break".into()),
            "continue" => Some("continue".into()),
            "statement" | "expression" => node
                .properties
                .get("source")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            _ => node
                .properties
                .get("source")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        },
        ScriptNodeKind::Literal
        | ScriptNodeKind::Variable
        | ScriptNodeKind::BinaryOp
        | ScriptNodeKind::UnaryOp
        | ScriptNodeKind::TableConstructor => None,
        _ => node
            .properties
            .get("source")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

fn compile_data_inputs(graph: &ScriptGraph, node: &ScriptNode) -> Vec<String> {
    let mut values = Vec::new();
    let data_inputs: Vec<&PortDef> = node
        .ports
        .iter()
        .filter(|p| p.kind == PortKind::Data && p.direction == PortDirection::Input)
        .collect();

    for port in data_inputs {
        let edge = graph
            .edges
            .iter()
            .find(|e| e.to_node == node.id && e.to_port == port.id);
        if let Some(edge) = edge {
            if let Some(source_node) = graph.get_node(&edge.from_node) {
                values.push(compile_expression(graph, source_node));
            }
        }
    }
    values
}

fn compile_expression(graph: &ScriptGraph, node: &ScriptNode) -> String {
    match &node.kind {
        ScriptNodeKind::Literal => node
            .properties
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("nil")
            .to_string(),
        ScriptNodeKind::Variable => node
            .properties
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("x")
            .to_string(),
        ScriptNodeKind::BinaryOp => {
            let op = node
                .properties
                .get("operator")
                .and_then(|v| v.as_str())
                .unwrap_or("+");
            let inputs = compile_data_inputs(graph, node);
            let lhs = inputs.first().map(|s| s.as_str()).unwrap_or("0");
            let rhs = inputs.get(1).map(|s| s.as_str()).unwrap_or("0");
            format!("{lhs} {op} {rhs}")
        }
        ScriptNodeKind::UnaryOp => {
            let op = node
                .properties
                .get("operator")
                .and_then(|v| v.as_str())
                .unwrap_or("-");
            let inputs = compile_data_inputs(graph, node);
            let operand = inputs.first().map(|s| s.as_str()).unwrap_or("0");
            format!("{op}{operand}")
        }
        ScriptNodeKind::FunctionCall => node
            .properties
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or(&node.label)
            .to_string(),
        ScriptNodeKind::TableConstructor => node
            .properties
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("{ }")
            .to_string(),
        _ => node
            .properties
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("nil")
            .to_string(),
    }
}

// ── Palette Helper ────────────────────────────────────────────────────

fn palette_entry(
    kind: ScriptNodeKind,
    label: &str,
    description: &str,
    category: &str,
) -> NodeKindDef {
    NodeKindDef {
        kind,
        label: label.into(),
        description: description.into(),
        category: category.into(),
        default_ports: Vec::new(),
    }
}

// ── Auto Layout ───────────────────────────────────────────────────────

fn auto_layout(graph: &mut ScriptGraph) {
    let sorted = graph.topological_sort();
    let sorted_owned: Vec<String> = sorted.into_iter().map(|s| s.to_string()).collect();
    for (i, id) in sorted_owned.iter().enumerate() {
        if let Some(node) = graph.get_node_mut(id) {
            let is_expr = node.kind.is_expression() && !node.kind.is_statement();
            if is_expr {
                node.position = [300.0, i as f64 * 120.0];
            } else {
                node.position = [0.0, i as f64 * 120.0];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompile_simple_return() {
        let lang = LuauVisualLanguage::new();
        let graph = lang.decompile("return 1 + 2").unwrap();
        assert!(graph.nodes.iter().any(|n| n.kind == ScriptNodeKind::Entry));
        assert!(graph.nodes.iter().any(|n| n.kind == ScriptNodeKind::Return));
        assert!(graph
            .nodes
            .iter()
            .any(|n| n.kind == ScriptNodeKind::BinaryOp));
    }

    #[test]
    fn decompile_local_assignment() {
        let lang = LuauVisualLanguage::new();
        let graph = lang.decompile("local x = 42").unwrap();
        let local = graph
            .nodes
            .iter()
            .find(|n| n.kind == ScriptNodeKind::LocalAssignment)
            .unwrap();
        assert!(local.label.contains("x"));
    }

    #[test]
    fn decompile_function_call() {
        let lang = LuauVisualLanguage::new();
        let graph = lang.decompile("print(42)").unwrap();
        assert!(graph
            .nodes
            .iter()
            .any(|n| n.kind == ScriptNodeKind::FunctionCall));
    }

    #[test]
    fn decompile_if_statement() {
        let lang = LuauVisualLanguage::new();
        let graph = lang.decompile("if x > 0 then\n  return 1\nend").unwrap();
        assert!(graph.nodes.iter().any(|n| n.kind == ScriptNodeKind::Branch));
    }

    #[test]
    fn compile_roundtrip_local() {
        let lang = LuauVisualLanguage::new();
        let graph = lang.decompile("local x = 42").unwrap();
        let source = lang.compile(&graph).unwrap();
        assert!(source.contains("local x = 42"));
    }

    #[test]
    fn compile_roundtrip_return() {
        let lang = LuauVisualLanguage::new();
        let graph = lang.decompile("return 1 + 2").unwrap();
        let source = lang.compile(&graph).unwrap();
        assert!(source.contains("return 1 + 2"));
    }

    #[test]
    fn compile_roundtrip_function_call() {
        let lang = LuauVisualLanguage::new();
        let graph = lang.decompile("print(42)").unwrap();
        let source = lang.compile(&graph).unwrap();
        assert!(source.contains("print(42)"));
    }

    #[test]
    fn palette_has_entries() {
        let lang = LuauVisualLanguage::new();
        let palette = lang.node_palette();
        assert!(!palette.is_empty());
        assert!(palette.iter().any(|p| p.category == "Control Flow"));
        assert!(palette.iter().any(|p| p.category == "Prism"));
    }

    #[test]
    fn validate_missing_entry() {
        let lang = LuauVisualLanguage::new();
        let graph = ScriptGraph::new("empty", "Empty");
        let diags = lang.validate(&graph);
        assert!(diags.iter().any(|d| d.message.contains("Entry")));
    }

    #[test]
    fn decompile_error_on_bad_syntax() {
        let lang = LuauVisualLanguage::new();
        let result = lang.decompile("if then end");
        assert!(result.is_err());
    }

    #[test]
    fn graph_edges_connect_properly() {
        let lang = LuauVisualLanguage::new();
        let graph = lang.decompile("local x = 1 + 2").unwrap();
        assert!(!graph.edges.is_empty());
        for edge in &graph.edges {
            let from = graph.get_node(&edge.from_node);
            let to = graph.get_node(&edge.to_node);
            assert!(from.is_some(), "edge source {} exists", edge.from_node);
            assert!(to.is_some(), "edge target {} exists", edge.to_node);
        }
    }
}
