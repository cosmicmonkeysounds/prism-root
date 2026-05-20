//! Filesystem-backed [`ImportResolver`] — the host half of
//! `prui-luau-fusion.md` §5.4 (`<import>`) + §5.9 (tier-2 named
//! Luau modules) + §5.2 (convention sibling pairing).
//!
//! The runtime has no filesystem of its own; it delegates every
//! `<import KIND="path"/>` and every sibling probe through the
//! [`ImportResolver`] trait. This module is the concrete shell/relay
//! implementation: it resolves a verbatim attribute path against the
//! importing document's directory (relative paths) or the
//! workspace's named script roots (`prism://<root>/…`), enforces the
//! per-kind extension contract, and refuses any path that escapes
//! its allowed root.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use prism_ui_runtime::interpret::ImportResolver;

/// Resolves `<import>` / sibling paths from disk.
///
/// - **Relative** (`./fmt.luau`, `../lib/dates.luau`) — resolved
///   against [`Self::base_dir`], the importing document's directory.
/// - **`prism://<root>/<rest>`** — resolved against the workspace
///   script root registered under `<root>` (declared in
///   `.prism.json` `scripts.*`); `prism://lib/fmt.luau` →
///   `<roots["lib"]>/fmt.luau`.
///
/// Both forms are canonicalised and re-checked to stay inside their
/// allowed root, so a crafted `../../../etc/passwd` never escapes.
#[derive(Clone, Debug)]
pub struct FsImportResolver {
    base_dir: PathBuf,
    roots: BTreeMap<String, PathBuf>,
}

impl FsImportResolver {
    /// Resolver rooted at one document directory, no `prism://`
    /// roots. The common widget case (`widget.prui` importing a
    /// sibling `./fmt.luau`).
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            roots: BTreeMap::new(),
        }
    }

    /// Register a `prism://<name>/…` root (a `.prism.json`
    /// `scripts.<name>` glob root). Chained at construction.
    pub fn with_root(mut self, name: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        self.roots.insert(name.into(), dir.into());
        self
    }

    /// Wrap as the trait object the runtime's
    /// `LowerScope::with_import_resolver` takes.
    pub fn arc(self) -> Arc<dyn ImportResolver> {
        Arc::new(self)
    }

    /// The `.prui` ↔ `.luau` / `.prss` convention probe (§5.2): given
    /// the importing document's path, read the same-stem sibling with
    /// `ext` if it exists. Returns `None` when there is no sibling —
    /// the loader treats that as "single-file widget", not an error.
    pub fn sibling_source(doc_path: impl AsRef<Path>, ext: &str) -> Option<String> {
        let sib = doc_path.as_ref().with_extension(ext);
        std::fs::read_to_string(sib).ok()
    }

    /// Map an import `kind` to the file extension it is allowed to
    /// resolve. Defence-in-depth: a `script` import can never pull a
    /// `.prss`, a `stylesheet` can never pull a `.luau`, etc.
    /// The `widget=` projection was Phase-0-removed; component imports
    /// land in Phase 7 under `component=`.
    fn allowed_ext(kind: &str) -> &'static [&'static str] {
        match kind {
            "script" | "dialect" => &["luau", "lua"],
            "stylesheet" => &["prss"],
            _ => &[],
        }
    }

    /// Resolve a verbatim attribute path to an absolute, in-root,
    /// existing file. `None` on any failure (unknown root, escape,
    /// missing file) — callers treat unresolved imports as skipped,
    /// never fatal, matching the runtime's graceful-degradation rule.
    fn resolve_path(&self, path: &str) -> Option<(PathBuf, PathBuf)> {
        if let Some(rest) = path.strip_prefix("prism://") {
            let (root_name, tail) = rest.split_once('/')?;
            let root = self.roots.get(root_name)?;
            Some((root.clone(), root.join(tail)))
        } else {
            // Relative (or absolute, which still gets root-checked
            // against base_dir below and will fail the containment
            // test unless it actually lives there).
            Some((self.base_dir.clone(), self.base_dir.join(path)))
        }
    }
}

impl ImportResolver for FsImportResolver {
    fn resolve_import(&self, kind: &str, path: &str) -> Option<String> {
        let exts = Self::allowed_ext(kind);
        if exts.is_empty() {
            return None;
        }
        let (root, candidate) = self.resolve_path(path)?;
        // Extension contract: the resolved file must carry an
        // extension this `kind` is allowed to load.
        let ext_ok = candidate
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| exts.contains(&e))
            .unwrap_or(false);
        if !ext_ok {
            return None;
        }
        // Containment: canonicalise both sides and require the file
        // to live inside its allowed root. `canonicalize` also
        // resolves `..` / symlinks, so this is the real escape gate.
        let root = root.canonicalize().ok()?;
        let file = candidate.canonicalize().ok()?;
        if !file.starts_with(&root) {
            return None;
        }
        std::fs::read_to_string(&file).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(label: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("prism-fsimport-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn resolves_relative_script() {
        let d = tmp("rel");
        std::fs::write(d.join("fmt.luau"), "return { v = 1 }").unwrap();
        let r = FsImportResolver::new(&d);
        assert_eq!(
            r.resolve_import("script", "./fmt.luau").as_deref(),
            Some("return { v = 1 }")
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn resolves_prism_root() {
        let base = tmp("base");
        let lib = tmp("lib");
        std::fs::write(lib.join("dates.luau"), "return {}").unwrap();
        let r = FsImportResolver::new(&base).with_root("lib", &lib);
        assert_eq!(
            r.resolve_import("script", "prism://lib/dates.luau")
                .as_deref(),
            Some("return {}")
        );
        std::fs::remove_dir_all(&base).unwrap();
        std::fs::remove_dir_all(&lib).unwrap();
    }

    #[test]
    fn rejects_extension_mismatch() {
        let d = tmp("extmix");
        std::fs::write(d.join("theme.prss"), "[class.x]").unwrap();
        let r = FsImportResolver::new(&d);
        // A `script` import must not be able to pull a `.prss`.
        assert_eq!(r.resolve_import("script", "./theme.prss"), None);
        // …but a `stylesheet` import can.
        assert!(r.resolve_import("stylesheet", "./theme.prss").is_some());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn refuses_path_escape() {
        let base = tmp("escbase");
        let secret = tmp("escsecret");
        std::fs::write(secret.join("steal.luau"), "return 'pwned'").unwrap();
        let r = FsImportResolver::new(base.join("widgets"));
        std::fs::create_dir_all(base.join("widgets")).unwrap();
        let rel = format!(
            "../../{}/steal.luau",
            secret.file_name().unwrap().to_str().unwrap()
        );
        assert_eq!(r.resolve_import("script", &rel), None);
        std::fs::remove_dir_all(&base).unwrap();
        std::fs::remove_dir_all(&secret).unwrap();
    }

    #[test]
    fn sibling_probe_reads_same_stem() {
        let d = tmp("sib");
        std::fs::write(d.join("card.prui"), "<container/>").unwrap();
        std::fs::write(d.join("card.luau"), "return { ok = true }").unwrap();
        assert_eq!(
            FsImportResolver::sibling_source(d.join("card.prui"), "luau").as_deref(),
            Some("return { ok = true }")
        );
        assert!(FsImportResolver::sibling_source(d.join("card.prui"), "prss").is_none());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
