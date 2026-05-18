//! `language::symbol_index` — a flat, path-keyed symbol table for the
//! in-shell IDE (IDE-mode plan Phase 2).
//!
//! Walks Luau source via the workspace `full_moon` parser and emits a
//! flat [`Symbol`] list — top-level functions, `local`s, and
//! module-table function fields (`M.foo = function() … end`) — each
//! carrying the byte offset + line/column of its **name** so the host
//! can place a caret on jump-to-definition. A [`SymbolIndex`] holds the
//! per-file vectors so the shell can rebuild one file on save without
//! re-walking the whole project.
//!
//! Luau is the only language wired today; every [`Symbol`] carries a
//! `language` discriminator so PRUI/PRSS indices can fold in later
//! (ide-mode-plan.md open question) without a breaking shape change.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::language::syntax::pos_at;
use full_moon::ast::{self, Stmt};
use full_moon::node::Node;

/// What a [`Symbol`] binds. Deliberately coarse — the IDE only needs
/// enough to pick an icon and rank palette results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// `function f()`, `local function f()`, `function M.f()`.
    Function,
    /// `local x = …` where the value is not a function.
    Local,
    /// `M.x = <fn>` — a function assigned onto an existing table
    /// (the canonical Luau module pattern).
    Field,
}

/// One named definition site discovered in a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// The bound name. Dotted/`:`-qualified for `function M.f` /
    /// `function M:f` so the palette shows the call path.
    pub name: String,
    pub kind: SymbolKind,
    /// File the symbol lives in (project-relative or absolute — the
    /// index is agnostic, it just round-trips what callers pass).
    pub path: PathBuf,
    /// Byte offset of the name token in the file's source.
    pub offset: usize,
    /// 1-based line of the name token.
    pub line: usize,
    /// 0-based character column of the name token.
    pub column: usize,
    /// Source language discriminator (`"luau"` today).
    pub language: String,
}

fn symbol_at(
    source: &str,
    name: impl Into<String>,
    kind: SymbolKind,
    path: &Path,
    offset: usize,
) -> Symbol {
    let p = pos_at(source, offset);
    Symbol {
        name: name.into(),
        kind,
        path: path.to_path_buf(),
        offset,
        line: p.line,
        column: p.column,
        language: "luau".to_string(),
    }
}

/// Byte offset of a node's first token, or `0` if full_moon can't
/// position it (a degenerate AST — the file still indexes, just at the
/// top, which is harmless for jump-to-def).
fn node_offset(n: &impl Node) -> usize {
    n.start_position().map(|p| p.bytes()).unwrap_or(0)
}

fn is_fn_expr(expr: &ast::Expression) -> bool {
    matches!(expr, ast::Expression::Function(_))
}

/// Extract every top-level symbol from one Luau file. A parse failure
/// yields an empty vec — a broken file simply contributes no symbols
/// (its error surfaces through the diagnostics path, not here),
/// mirroring [`crate::language::luau::top_level_locals`].
pub fn index_luau_source(path: &Path, source: &str) -> Vec<Symbol> {
    let lua_version = full_moon::LuaVersion::luau();
    let Ok(ast) = full_moon::parse_fallible(source, lua_version).into_result() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for stmt in ast.nodes().stmts() {
        match stmt {
            Stmt::FunctionDeclaration(decl) => {
                let fname = decl.name();
                let dotted = fname.to_string().trim().to_string();
                let offset = fname
                    .names()
                    .iter()
                    .next()
                    .map(node_offset)
                    .unwrap_or_else(|| node_offset(decl));
                out.push(symbol_at(
                    source,
                    dotted,
                    SymbolKind::Function,
                    path,
                    offset,
                ));
            }
            Stmt::LocalFunction(local_fn) => {
                let name_tok = local_fn.name();
                out.push(symbol_at(
                    source,
                    name_tok.token().to_string(),
                    SymbolKind::Function,
                    path,
                    node_offset(name_tok),
                ));
            }
            Stmt::LocalAssignment(local) => {
                let exprs: Vec<&ast::Expression> = local.expressions().iter().collect();
                for (i, name) in local.names().iter().enumerate() {
                    let kind = match exprs.get(i) {
                        Some(e) if is_fn_expr(e) => SymbolKind::Function,
                        _ => SymbolKind::Local,
                    };
                    out.push(symbol_at(
                        source,
                        name.token().to_string(),
                        kind,
                        path,
                        node_offset(name),
                    ));
                }
            }
            Stmt::Assignment(assign) => {
                // Only surface `M.foo = function() … end` — a function
                // hung onto an existing table. Plain reassignments
                // aren't definitions worth navigating to.
                let exprs: Vec<&ast::Expression> = assign.expressions().iter().collect();
                for (i, var) in assign.variables().iter().enumerate() {
                    if exprs.get(i).is_some_and(|e| is_fn_expr(e)) {
                        out.push(symbol_at(
                            source,
                            var.to_string().trim().to_string(),
                            SymbolKind::Field,
                            path,
                            node_offset(var),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Lowercase subsequence score: every query char must appear in order.
/// Higher is better; `None` = no match. Rewards contiguity, a prefix
/// hit, and shorter candidates so palette ordering feels right.
fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let cand = candidate.to_lowercase();
    let cand_bytes = cand.as_bytes();
    let mut qi = 0;
    let q: Vec<u8> = query.to_lowercase().into_bytes();
    let mut score = 0i32;
    let mut last_hit: Option<usize> = None;
    for (ci, &cb) in cand_bytes.iter().enumerate() {
        if qi < q.len() && cb == q[qi] {
            if qi == 0 && ci == 0 {
                score += 8; // prefix match
            }
            if let Some(prev) = last_hit {
                if prev + 1 == ci {
                    score += 5; // contiguous run
                }
            }
            score += 1;
            last_hit = Some(ci);
            qi += 1;
        }
    }
    if qi == q.len() {
        // Shorter candidates rank above longer ones at equal overlap.
        Some(score - (cand_bytes.len() as i32) / 8)
    } else {
        None
    }
}

/// Flat symbol table keyed by file path so one file rebuilds in
/// isolation on save.
#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    by_path: BTreeMap<PathBuf, Vec<Symbol>>,
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Re-index a single file from its current source, replacing any
    /// prior entry. A file that yields no symbols is dropped from the
    /// map so `files()` stays honest.
    pub fn rebuild_file(&mut self, path: impl Into<PathBuf>, source: &str) {
        let path = path.into();
        let syms = index_luau_source(&path, source);
        if syms.is_empty() {
            self.by_path.remove(&path);
        } else {
            self.by_path.insert(path, syms);
        }
    }

    pub fn remove_file(&mut self, path: &Path) {
        self.by_path.remove(path);
    }

    pub fn clear(&mut self) {
        self.by_path.clear();
    }

    pub fn files(&self) -> impl Iterator<Item = &Path> {
        self.by_path.keys().map(|p| p.as_path())
    }

    pub fn all(&self) -> impl Iterator<Item = &Symbol> {
        self.by_path.values().flatten()
    }

    pub fn len(&self) -> usize {
        self.by_path.values().map(|v| v.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_path.values().all(|v| v.is_empty())
    }

    /// Exact-name matches (the last `.`/`:` segment also counts so
    /// `foo` resolves `M.foo`). Drives jump-to-definition.
    pub fn lookup(&self, name: &str) -> Vec<&Symbol> {
        self.all()
            .filter(|s| {
                s.name == name
                    || s.name
                        .rsplit([':', '.'])
                        .next()
                        .is_some_and(|seg| seg == name)
            })
            .collect()
    }

    /// Fuzzy-ranked matches for the Ctrl+P / Ctrl+Shift+O palette.
    /// Best score first; ties broken by name then path for stable
    /// ordering across rebuilds.
    pub fn fuzzy(&self, query: &str, limit: usize) -> Vec<&Symbol> {
        let mut scored: Vec<(i32, &Symbol)> = self
            .all()
            .filter_map(|s| fuzzy_score(query, &s.name).map(|sc| (sc, s)))
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.name.cmp(&b.1.name))
                .then_with(|| a.1.path.cmp(&b.1.path))
        });
        scored.into_iter().take(limit).map(|(_, s)| s).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> PathBuf {
        PathBuf::from("main.luau")
    }

    #[test]
    fn indexes_functions_locals_and_fields() {
        let src = "\
local count = 0
local function helper(x)
  return x + 1
end
function Greeter.greet(name)
  return name
end
local M = {}
M.run = function() return 1 end
";
        let syms = index_luau_source(&p(), src);
        let names: Vec<(&str, SymbolKind)> =
            syms.iter().map(|s| (s.name.as_str(), s.kind)).collect();
        assert!(names.contains(&("count", SymbolKind::Local)));
        assert!(names.contains(&("helper", SymbolKind::Function)));
        assert!(names.contains(&("Greeter.greet", SymbolKind::Function)));
        assert!(names.contains(&("M", SymbolKind::Local)));
        assert!(names.contains(&("M.run", SymbolKind::Field)));
    }

    #[test]
    fn local_assigned_function_is_a_function() {
        let syms = index_luau_source(&p(), "local f = function() return 2 end");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn name_offset_points_at_the_identifier() {
        let src = "local foo = 1\nlocal function bar() end";
        let syms = index_luau_source(&p(), src);
        let foo = syms.iter().find(|s| s.name == "foo").unwrap();
        assert_eq!(&src[foo.offset..foo.offset + 3], "foo");
        assert_eq!(foo.line, 1);
        let bar = syms.iter().find(|s| s.name == "bar").unwrap();
        assert_eq!(&src[bar.offset..bar.offset + 3], "bar");
        assert_eq!(bar.line, 2);
    }

    #[test]
    fn parse_error_yields_no_symbols() {
        assert!(index_luau_source(&p(), "local x = = =").is_empty());
    }

    #[test]
    fn rebuild_and_remove_are_isolated() {
        let mut idx = SymbolIndex::new();
        idx.rebuild_file("a.luau", "local a = 1");
        idx.rebuild_file("b.luau", "local b = 2");
        assert_eq!(idx.len(), 2);
        idx.rebuild_file("a.luau", "local a = 1\nlocal a2 = 2");
        assert_eq!(idx.len(), 3);
        idx.remove_file(Path::new("b.luau"));
        assert_eq!(idx.len(), 2);
        assert!(idx.all().all(|s| s.path == Path::new("a.luau")));
    }

    #[test]
    fn empty_file_drops_from_index() {
        let mut idx = SymbolIndex::new();
        idx.rebuild_file("a.luau", "local a = 1");
        idx.rebuild_file("a.luau", "-- just a comment\n");
        assert!(idx.is_empty());
        assert_eq!(idx.files().count(), 0);
    }

    #[test]
    fn lookup_matches_bare_and_qualified() {
        let mut idx = SymbolIndex::new();
        idx.rebuild_file("m.luau", "function M.greet() end\nlocal greet = 1");
        assert_eq!(idx.lookup("M.greet").len(), 1);
        // bare `greet` resolves both the field's last segment and the local
        assert_eq!(idx.lookup("greet").len(), 2);
        assert!(idx.lookup("missing").is_empty());
    }

    #[test]
    fn fuzzy_ranks_prefix_and_contiguous_first() {
        let mut idx = SymbolIndex::new();
        idx.rebuild_file(
            "f.luau",
            "local function openProject() end\nlocal function reopenPanel() end\nlocal op = 1",
        );
        let hits = idx.fuzzy("op", 10);
        // `op` (exact, shortest) and `openProject` (prefix) outrank `reopenPanel`.
        assert_eq!(hits[0].name, "op");
        assert!(hits.iter().any(|s| s.name == "openProject"));
        let none = idx.fuzzy("zzz", 10);
        assert!(none.is_empty());
    }
}
