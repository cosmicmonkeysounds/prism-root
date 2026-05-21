//! `prism rewrite-canonical <paths>...` — Phase 2 migration tool.
//!
//! See `docs/dev/prui-expressiveness-roadmap.md` §6.24 + §8 Phase 2.
//! Walks one or more `.prui` files (or directories) and rewrites
//! their XML-shape declarations into the canonical surface.
//!
//! Calling pattern:
//!
//! ```text
//! prism rewrite-canonical packages/prism-shell/web/strawman.prui
//! prism rewrite-canonical apps/                        # walk a tree
//! prism --dry-run rewrite-canonical apps/              # show diff, do not write
//! ```
//!
//! Idempotent — a file already in canonical surface is returned
//! unchanged. Files with XML-side parse errors are skipped with a
//! warning unless `--force` is supplied; ambiguous shapes that the
//! cheatsheet doesn't cover get a `-- TODO:` line in the output so
//! a human reviewer cleans them up.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;

use prism_core::language::prism_ui::rewrite_xml_to_canonical;

use crate::workspace::Workspace;

#[derive(Debug, Args)]
pub struct RewriteCanonicalArgs {
    /// Files / directories to rewrite. Directories are walked
    /// recursively; only `.prui` files are touched.
    pub paths: Vec<PathBuf>,

    /// Print the rewritten output to stdout instead of overwriting
    /// the source file. Useful for a single-file review.
    #[arg(long)]
    pub stdout: bool,

    /// Continue even when a file has parse errors (the rewriter is
    /// best-effort; this flag commits the best-effort output).
    #[arg(long)]
    pub force: bool,

    /// Only print a summary of what would change. Same as the
    /// global `--dry-run` but available locally for ergonomics.
    #[arg(long)]
    pub check: bool,
}

pub fn run(args: &RewriteCanonicalArgs, _ws: &Workspace, dry_run: bool) -> Result<u8> {
    if args.paths.is_empty() {
        bail!(
            "prism rewrite-canonical: at least one path is required \
             (file or directory)"
        );
    }
    let files = collect_targets(&args.paths)?;
    if files.is_empty() {
        eprintln!("no `.prui` files found under the given paths");
        return Ok(0);
    }

    let mut total = 0usize;
    let mut changed = 0usize;
    let mut skipped = 0usize;
    let dry = dry_run || args.check || args.stdout;

    for path in &files {
        total += 1;
        let result = rewrite_file(path, args, dry);
        match result {
            Ok(Outcome::Unchanged) => {}
            Ok(Outcome::Changed) => {
                changed += 1;
                if !dry {
                    println!("rewrote {}", path.display());
                } else if !args.stdout {
                    println!("would rewrite {}", path.display());
                }
            }
            Ok(Outcome::ParseError(msg)) => {
                skipped += 1;
                eprintln!("skip {} ({msg})", path.display());
            }
            Err(e) => {
                skipped += 1;
                eprintln!("error {}: {e:#}", path.display());
            }
        }
    }

    println!("{total} file(s) scanned, {changed} would-be-rewrite, {skipped} skipped",);

    if skipped > 0 && !args.force {
        return Ok(2);
    }
    Ok(0)
}

enum Outcome {
    Unchanged,
    Changed,
    ParseError(String),
}

fn rewrite_file(path: &Path, args: &RewriteCanonicalArgs, dry: bool) -> Result<Outcome> {
    let original =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let (rewritten, errs) = rewrite_xml_to_canonical(&original);

    if !errs.is_empty() && !args.force {
        return Ok(Outcome::ParseError(format!(
            "{} XML parse error(s) — pass --force to overwrite anyway",
            errs.len()
        )));
    }

    if rewritten == original {
        return Ok(Outcome::Unchanged);
    }

    if args.stdout {
        print!("{rewritten}");
        return Ok(Outcome::Changed);
    }

    if dry {
        return Ok(Outcome::Changed);
    }

    fs::write(path, &rewritten).with_context(|| format!("writing {}", path.display()))?;
    Ok(Outcome::Changed)
}

fn collect_targets(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for p in paths {
        if !p.exists() {
            bail!("path does not exist: {}", p.display());
        }
        if p.is_file() {
            if is_prui(p) {
                out.push(p.clone());
            }
            continue;
        }
        // Directory — walk it.
        walk_dir(p, &mut out)?;
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn walk_dir(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        fs::read_dir(root).with_context(|| format!("reading directory {}", root.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // Skip target / .git / node_modules to avoid wasting
            // cycles on never-edited trees.
            if is_skippable_dir(&path) {
                continue;
            }
            walk_dir(&path, out)?;
        } else if is_prui(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn is_prui(p: &Path) -> bool {
    p.extension().and_then(OsStr::to_str) == Some("prui")
}

fn is_skippable_dir(p: &Path) -> bool {
    matches!(
        p.file_name().and_then(OsStr::to_str),
        Some("target" | ".git" | "node_modules" | "dist" | ".next")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn collects_single_file() {
        let mut tmp = NamedTempFile::with_suffix(".prui").unwrap();
        writeln!(tmp, "<namespace name=\"Foo\"/>").unwrap();
        let paths = vec![tmp.path().to_path_buf()];
        let collected = collect_targets(&paths).unwrap();
        assert_eq!(collected.len(), 1);
    }

    #[test]
    fn rewrites_inline_and_returns_changed() {
        let mut tmp = NamedTempFile::with_suffix(".prui").unwrap();
        writeln!(tmp, "<namespace name=\"Foo\"/>").unwrap();
        let path = tmp.path().to_path_buf();
        let args = RewriteCanonicalArgs {
            paths: vec![path.clone()],
            stdout: false,
            force: false,
            check: false,
        };
        let outcome = rewrite_file(&path, &args, false).unwrap();
        assert!(matches!(outcome, Outcome::Changed));
        let read = fs::read_to_string(&path).unwrap();
        assert!(read.starts_with("namespace Foo"));
    }

    #[test]
    fn dry_run_does_not_overwrite() {
        let mut tmp = NamedTempFile::with_suffix(".prui").unwrap();
        writeln!(tmp, "<namespace name=\"Foo\"/>").unwrap();
        let path = tmp.path().to_path_buf();
        let original = fs::read_to_string(&path).unwrap();
        let args = RewriteCanonicalArgs {
            paths: vec![path.clone()],
            stdout: false,
            force: false,
            check: true,
        };
        let outcome = rewrite_file(&path, &args, true).unwrap();
        assert!(matches!(outcome, Outcome::Changed));
        // File on disk is untouched.
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn idempotent_on_canonical_file() {
        let mut tmp = NamedTempFile::with_suffix(".prui").unwrap();
        writeln!(tmp, "namespace Foo").unwrap();
        let path = tmp.path().to_path_buf();
        let args = RewriteCanonicalArgs {
            paths: vec![path.clone()],
            stdout: false,
            force: false,
            check: false,
        };
        let outcome = rewrite_file(&path, &args, false).unwrap();
        assert!(matches!(outcome, Outcome::Unchanged));
    }
}
