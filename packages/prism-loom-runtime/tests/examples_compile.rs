//! Asserts that every `.loom` file under `examples/loom/` parses,
//! validates, and boots cleanly. Run with `cargo test
//! --package prism-loom-runtime --test examples_compile`.
//!
//! The examples folder is the language reference for new authors. A
//! broken example is a broken lesson — this test is the gate that
//! keeps that from happening.

use std::fs;
use std::path::{Path, PathBuf};

use prism_loom_runtime::Show;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at `packages/prism-loom-runtime/`. The
    // examples live up two levels.
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .expect("crate has two ancestors")
}

fn collect_loom_files(root: &Path) -> Vec<PathBuf> {
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
            } else if path.extension().and_then(|e| e.to_str()) == Some("loom") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_example_loom_file_compiles_cleanly() {
    let examples_root = workspace_root().join("examples").join("loom");
    if !examples_root.exists() {
        // No examples staged yet — pass silently rather than fail in
        // an unrelated checkout.
        return;
    }
    let files = collect_loom_files(&examples_root);
    assert!(
        !files.is_empty(),
        "expected at least one .loom file under examples/loom/"
    );

    let mut failures: Vec<String> = Vec::new();
    for path in &files {
        let src = fs::read_to_string(path).expect("read source");
        let result = match Show::load_with_diagnostics(&src) {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("{}: load error: {e}", path.display()));
                continue;
            }
        };
        let errors: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| {
                matches!(
                    d.severity,
                    prism_core::language::loom::parser::Severity::Error
                )
            })
            .collect();
        if !errors.is_empty() {
            failures.push(format!(
                "{}: {} error diagnostic(s): {}",
                path.display(),
                errors.len(),
                errors
                    .iter()
                    .map(|d| format!("[{}] {}", d.id, d.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "example files failed to compile:\n{}",
        failures.join("\n")
    );
}
