//! `loom-golden` — golden-fixture dump tool for the TypeScript
//! `@loom/core` port.
//!
//! Walks every example project, parses every `.loom` file, and writes
//! self-contained JSON fixtures the vitest parity harness in
//! `packages/loom/core/test` checks the TS engine against:
//!
//! - `sources.json`      — `{ relPath: source }` (so the TS test is
//!                          self-contained; it never reads the examples
//!                          dir at test time).
//! - `diagnostics.json`  — `{ relPath: JsDiagnostic[] }`, the exact
//!                          shape `wasm/src/lib.rs::JsDiagnostic` emits.
//! - `ast.json`          — `{ relPath: <serde LoomFile> }`.
//! - `apply.json`        — `{ relPath: [ {op, args, ok, output} ] }`, a
//!                          deterministic script of `apply_*` ops with
//!                          byte-exact expected output (the round-trip
//!                          identity guard for the edit API).
//!
//! Usage: `loom-golden <examples-dir> <out-fixtures-dir>`
//!
//! Build with `--no-default-features` to skip mlua; this tool only
//! touches `loom_parser`, so the play/lsp goldens live in later passes.

use std::fs;
use std::path::{Path, PathBuf};

use loom_parser::ast::Item;
use loom_parser::{
    apply_edits, insert_beat, move_beat, parse, remove_beat, set_beat_property, Anchor, Diagnostic,
    LoomFile, Severity,
};
use serde_json::{json, Map, Value as JsonValue};

fn main() {
    let mut args = std::env::args().skip(1);
    let examples_dir = args.next().unwrap_or_else(|| usage());
    let out_dir = args.next().unwrap_or_else(|| usage());
    let examples = PathBuf::from(&examples_dir);
    let out = PathBuf::from(&out_dir);

    let mut count = 0;
    for example in example_dirs(&examples) {
        let name = example
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let mut files = loom_files(&example);
        files.sort();

        let mut sources = Map::new();
        let mut diagnostics = Map::new();
        let mut asts = Map::new();
        let mut applies = Map::new();

        for file in &files {
            let rel = rel_path(&example, file);
            let source = match fs::read_to_string(file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("skip {}: {e}", file.display());
                    continue;
                }
            };
            let (ast, diags) = parse(&source);

            sources.insert(rel.clone(), JsonValue::String(source.clone()));
            diagnostics.insert(rel.clone(), js_diagnostics(&diags));
            asts.insert(
                rel.clone(),
                serde_json::to_value(&ast).expect("LoomFile serializes"),
            );
            applies.insert(rel.clone(), apply_script(&source, &ast));
        }

        let dir = out.join(&name);
        fs::create_dir_all(&dir).expect("create fixture dir");
        write_json(&dir.join("sources.json"), &JsonValue::Object(sources));
        write_json(&dir.join("diagnostics.json"), &JsonValue::Object(diagnostics));
        write_json(&dir.join("ast.json"), &JsonValue::Object(asts));
        write_json(&dir.join("apply.json"), &JsonValue::Object(applies));
        count += 1;
        eprintln!("wrote fixtures for {name} ({} files)", files.len());
    }
    eprintln!("done: {count} example(s)");
}

fn usage() -> ! {
    eprintln!("usage: loom-golden <examples-dir> <out-fixtures-dir>");
    std::process::exit(2);
}

/// Each immediate subdirectory of `examples/` that contains at least one
/// `.loom` file is an example project.
fn example_dirs(examples: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(examples)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !loom_files(p).is_empty())
        .collect();
    dirs.sort();
    dirs
}

/// All `.loom` files under `root`, recursively.
fn loom_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("loom") {
                out.push(path);
            }
        }
    }
    out
}

/// Project-relative path with forward slashes.
fn rel_path(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Mirror of `wasm/src/lib.rs::JsDiagnostic` — `{code, severity,
/// message, from, to, line, column}` with lowercase severity.
fn js_diagnostics(diags: &[Diagnostic]) -> JsonValue {
    JsonValue::Array(
        diags
            .iter()
            .map(|d| {
                json!({
                    "code": d.code.id(),
                    "severity": match d.severity {
                        Severity::Error => "error",
                        Severity::Warning => "warning",
                    },
                    "message": d.message,
                    "from": d.span.start.byte,
                    "to": d.span.end.byte,
                    "line": d.span.start.line,
                    "column": d.span.start.column,
                })
            })
            .collect(),
    )
}

/// First beat name in the file, plus its first contract key (if any).
fn first_beat(ast: &LoomFile) -> Option<(String, Option<String>)> {
    for item in &ast.items {
        if let Item::Beat(beat) = item {
            let key = beat.contract.keys().next().cloned();
            return Some((beat.name.clone(), key));
        }
    }
    None
}

/// Run a deterministic script of `apply_*` ops, each against the
/// ORIGINAL source independently, recording byte-exact output (or the
/// error `Display`). The TS harness reruns the identical script and
/// asserts equality.
fn apply_script(source: &str, ast: &LoomFile) -> JsonValue {
    let mut ops: Vec<JsonValue> = Vec::new();
    let Some((beat, contract_key)) = first_beat(ast) else {
        return JsonValue::Array(ops);
    };

    // 1. set a fresh property on the first beat (insert path).
    ops.push(run_set_property(source, &beat, "golden_probe", "1"));
    // 2. overwrite an existing contract key, if present (update path).
    if let Some(key) = &contract_key {
        ops.push(run_set_property(source, &beat, key, "GOLDEN"));
    }
    // 3. insert a new beat at end of file.
    ops.push(run_insert(source, "golden_new", "end", ""));
    // 4. insert a new beat before the first beat.
    ops.push(run_insert(source, "golden_before", "before", &beat));
    // 5. move the first beat to the end of the file.
    ops.push(run_move(source, &beat, "end", ""));
    // 6. remove the first beat.
    ops.push(run_remove(source, &beat));

    JsonValue::Array(ops)
}

fn run_set_property(source: &str, beat: &str, key: &str, value: &str) -> JsonValue {
    let (file, _) = parse(source);
    let result = set_beat_property(source, &file, beat, key, value)
        .and_then(|edits| apply_edits(source, &edits));
    op_result("set_beat_property", json!([beat, key, value]), result)
}

fn run_insert(source: &str, name: &str, anchor_kind: &str, anchor_name: &str) -> JsonValue {
    let (file, _) = parse(source);
    let result = anchor_of(anchor_kind, anchor_name)
        .and_then(|anchor| insert_beat(source, &file, name, anchor).map_err(|e| e.to_string()))
        .and_then(|edits| apply_edits(source, &edits).map_err(|e| e.to_string()));
    op_result_str("insert_beat", json!([name, anchor_kind, anchor_name]), result)
}

fn run_move(source: &str, beat: &str, anchor_kind: &str, anchor_name: &str) -> JsonValue {
    let (file, _) = parse(source);
    let result = anchor_of(anchor_kind, anchor_name)
        .and_then(|anchor| move_beat(source, &file, beat, anchor).map_err(|e| e.to_string()))
        .and_then(|edits| apply_edits(source, &edits).map_err(|e| e.to_string()));
    op_result_str("move_beat", json!([beat, anchor_kind, anchor_name]), result)
}

fn run_remove(source: &str, beat: &str) -> JsonValue {
    let (file, _) = parse(source);
    let result =
        remove_beat(source, &file, beat).and_then(|edits| apply_edits(source, &edits));
    op_result("remove_beat", json!([beat]), result)
}

fn anchor_of(kind: &str, name: &str) -> Result<Anchor, String> {
    match kind {
        "before" => Ok(Anchor::Before(name.to_string())),
        "after" => Ok(Anchor::After(name.to_string())),
        "start" => Ok(Anchor::Start),
        "end" => Ok(Anchor::End),
        other => Err(format!("unknown anchor kind: {other}")),
    }
}

fn op_result(
    op: &str,
    args: JsonValue,
    result: Result<String, loom_parser::EditError>,
) -> JsonValue {
    op_result_str(op, args, result.map_err(|e| e.to_string()))
}

fn op_result_str(op: &str, args: JsonValue, result: Result<String, String>) -> JsonValue {
    match result {
        Ok(output) => json!({ "op": op, "args": args, "ok": true, "output": output }),
        Err(message) => json!({ "op": op, "args": args, "ok": false, "output": message }),
    }
}

fn write_json(path: &Path, value: &JsonValue) {
    let text = serde_json::to_string_pretty(value).expect("serialize fixture");
    fs::write(path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}
