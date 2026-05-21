//! PRSS stylesheet parser and IR.
//!
//! ```text
//! [tokens.colors]
//! accent = "#7c3aed"
//!
//! [class.btn]
//! background = "{tokens.colors.accent}"
//! radius = 8
//!
//! [class.btn.hovered]
//! background = "#a78bfa"
//! ```
//!
//! Parses through `toml`'s recovery-friendly deserialiser; every
//! per-class diagnostic surfaces alongside the partial result so the
//! editor can show every issue at once. Class property values stay as
//! raw strings; the runtime's existing `apply_style_override` consumes
//! them through the same parser inline `style:` attributes use.

use std::collections::HashSet;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::language::syntax::Scanner;

/// Canonical file extension. Hosts may accept variants; the loader
/// in `prism-cli` watches anything under this list.
pub const PRSS_EXTENSIONS: &[&str] = &["prss"];

/// Split a class-key spelling into its descendant-selector segments.
///
/// PRSS class keys are TOML strings, so authors write the multi-segment
/// form via a quoted-key table — `[class."btn icon"]` or
/// `[class.".btn .icon"]` (CSS-style with leading dots, ignored).
/// The returned slice has one segment per non-empty whitespace token,
/// each with a leading `.` stripped so PRSS lookup keys match the
/// PRUI `class="..."` spelling.
///
/// Examples:
/// * `"btn"` → `["btn"]` (flat — caller treats as a normal class).
/// * `"btn icon"` → `["btn", "icon"]` (descendant: `btn` ancestor,
///   `icon` current).
/// * `".btn .icon"` → `["btn", "icon"]` (CSS-style).
/// * `""` → `[]` (degenerate; caller drops).
pub fn selector_segments(name: &str) -> Vec<&str> {
    name.split_whitespace()
        .map(|s| s.trim_start_matches('.'))
        .filter(|s| !s.is_empty())
        .collect()
}

/// True when `name` is a descendant selector — i.e. its
/// [`selector_segments`] decomposition has more than one segment.
/// Convenience for callers that branch on flat-vs-descendant
/// without caring about the segment list.
pub fn is_descendant_selector(name: &str) -> bool {
    selector_segments(name).len() > 1
}

/// The recognised state suffixes a `[class.NAME.STATE]` table may
/// use. Mirrors `prism_core::language::prism_ui::STATE_SUFFIXES`;
/// the two must stay in sync since PRSS classes and inline
/// `style:key:state=` attrs both walk the same state-bucket
/// runtime. Phase 1 (§7.7) expanded the set from three to ten.
pub const STATE_SUFFIXES: &[&str] = &[
    "hovered",
    "pressed",
    "focused",
    "focus-within",
    "selected",
    "disabled",
    "empty",
    "checked",
    "entry",
    "exit",
];

/// Parsed PRSS file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StyleSheet {
    /// Optional schema-version pin (the `prss-version = N` top-level
    /// key). Today the only valid value is `1`; the parser tolerates
    /// missing / unknown versions so a forward-compat stylesheet
    /// doesn't break a current-runtime load.
    pub version: Option<u32>,
    /// Token overrides — sparse; missing keys fall through to the
    /// runtime's `DesignTokens` defaults via [`Self::merged_tokens`].
    pub tokens: TokenOverrides,
    /// User-defined classes, in declaration order.
    pub classes: IndexMap<String, ClassDef>,
}

/// Per-bucket overrides for the four token groups
/// (`colors`/`spacing`/`radius`/`typography`).
///
/// Values are kept as raw strings so the same vocabulary
/// `apply_style_override` consumes (`"#rrggbbaa"`, `"16"`,
/// `"1rem"`, …) flows through unchanged. Numeric tokens (spacing,
/// radius, typography) get coerced to a string at parse time when
/// authored as TOML integers / floats.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenOverrides {
    #[serde(default)]
    pub colors: IndexMap<String, String>,
    #[serde(default)]
    pub spacing: IndexMap<String, String>,
    #[serde(default)]
    pub radius: IndexMap<String, String>,
    #[serde(default)]
    pub typography: IndexMap<String, String>,
}

/// One class definition.
///
/// `extends` carries the parent class name (`Some("btn")` means
/// "inherit btn's properties first"). The `properties` map carries
/// the base-state key-value pairs. `states` carries per-state
/// overrides; each inner map is `<key> -> <value>` with the same
/// vocabulary the base properties accept.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassDef {
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub properties: IndexMap<String, String>,
    #[serde(default)]
    pub states: IndexMap<String, IndexMap<String, String>>,
}

/// A class with its `extends` chain flattened. Property order is
/// parent-first → child last so a later assignment wins on conflict
/// (the canonical specificity rule from `prss-reference.md` §4.6).
#[derive(Debug, Clone, Default)]
pub struct ResolvedClass {
    /// Flattened base properties in application order.
    pub properties: Vec<(String, String)>,
    /// Flattened per-state overrides. Each `(state, key, value)`
    /// applies as `<key>:<state>` through the runtime's existing
    /// state-suffix pathway.
    pub states: Vec<(String, String, String)>,
}

/// Per-class parse diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseError {
    pub message: String,
    pub code: &'static str,
    /// The class name the diagnostic is attached to, when applicable.
    /// `None` for file-level diagnostics (e.g. malformed TOML).
    pub class: Option<String>,
}

impl ParseError {
    fn file(msg: impl Into<String>, code: &'static str) -> Self {
        Self {
            message: msg.into(),
            code,
            class: None,
        }
    }

    fn class(class: impl Into<String>, msg: impl Into<String>, code: &'static str) -> Self {
        Self {
            message: msg.into(),
            code,
            class: Some(class.into()),
        }
    }
}

// ---------------------------------------------------------------------------
// On-disk TOML shape (private)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct PrssFile {
    #[serde(rename = "prss-version", default)]
    version: Option<u32>,
    #[serde(default)]
    tokens: RawTokens,
    #[serde(default)]
    class: IndexMap<String, RawClass>,
}

#[derive(Deserialize, Default)]
struct RawTokens {
    #[serde(default)]
    colors: IndexMap<String, toml::Value>,
    #[serde(default)]
    spacing: IndexMap<String, toml::Value>,
    #[serde(default)]
    radius: IndexMap<String, toml::Value>,
    #[serde(default)]
    typography: IndexMap<String, toml::Value>,
}

/// Raw class table — we serde-flatten the body into a single map so
/// scalar properties and state sub-tables can coexist on the same
/// `[class.NAME]` header. Post-parse, [`split_class_body`] separates
/// scalars (→ properties) from tables (→ states).
#[derive(Deserialize, Default)]
struct RawClass {
    #[serde(flatten)]
    body: IndexMap<String, toml::Value>,
}

// ---------------------------------------------------------------------------
// Public parsing surface
// ---------------------------------------------------------------------------

/// Parse a `.prss` source string into a [`StyleSheet`].
///
/// Returns `(stylesheet, errors)`. Recoverable diagnostics
/// (`missing-parent`, `cyclic-extends`, `unknown-state`, …) populate
/// the errors vec without aborting the whole load — partial classes
/// land and editors can show every issue at once. Hard failures
/// (TOML syntax error) abort and surface as a single
/// `toml-syntax` diagnostic.
/// **§5.10** — desugar the bare-expression brace surface
/// (`background = { darken(accent, 0.1) }`) into the TOML-valid
/// `{ lua = "…" }` form the rest of the pipeline (and
/// [`LUA_VALUE_SENTINEL`]) already understands. PRSS is a
/// TOML-*shaped* superset: a brace whose body is a bare Luau
/// expression is not valid TOML, so it is rewritten before the
/// `toml` crate sees it. A brace whose body is `key = …` (an inline
/// table — a state sub-table, or the legacy `{ lua = "…" }`) is left
/// untouched.
/// Recursive, `Scanner`-driven, quote/comment/brace-aware rewrite
/// (the project's standing "parsers go through Prism Syntax" rule —
/// no hand-rolled string indexing). Every brace group in *value
/// position* (preceded by `=`) is classified: empty → left as-is; an
/// inline table (`key = …`) → kept a table but its own values
/// recursed into (so a `{expr}` nested inside a state sub-table is
/// still desugared); anything else → a bare Luau expression wrapped
/// as `{ lua = "…" }`.
fn desugar_brace_exprs(source: &str) -> String {
    let mut sc = Scanner::new(source);
    let mut out = String::with_capacity(source.len());
    while let Some(c) = sc.peek() {
        match c {
            // TOML line comment — copy verbatim to EOL.
            '#' => {
                while let Some(ch) = sc.peek() {
                    if ch == '\n' {
                        break;
                    }
                    out.push(ch);
                    sc.advance();
                }
            }
            '"' | '\'' => {
                out.push(c);
                sc.advance();
                while let Some(ch) = sc.advance() {
                    out.push(ch);
                    if ch == '\\' {
                        if let Some(n) = sc.advance() {
                            out.push(n);
                        }
                        continue;
                    }
                    if ch == c {
                        break;
                    }
                }
            }
            '{' if last_non_ws(&out) == Some('=') => {
                let open = sc.offset();
                match scan_matching_brace(&mut sc) {
                    Some(close) => {
                        let body = &source[open + 1..close];
                        let trimmed = body.trim();
                        if trimmed.is_empty() {
                            out.push_str(&source[open..close + 1]);
                        } else if inner_is_inline_table(trimmed) {
                            out.push('{');
                            out.push_str(&desugar_brace_exprs(body));
                            out.push('}');
                        } else {
                            let escaped = trimmed.replace('\\', "\\\\").replace('"', "\\\"");
                            out.push_str(&format!("{{ lua = \"{escaped}\" }}"));
                        }
                    }
                    None => {
                        out.push('{');
                        sc.advance();
                    }
                }
            }
            _ => {
                out.push(c);
                sc.advance();
            }
        }
    }
    out
}

/// Last non-whitespace char already emitted — used to detect that a
/// `{` is in TOML value position (`key = {`).
fn last_non_ws(s: &str) -> Option<char> {
    s.chars().rev().find(|c| !c.is_whitespace())
}

/// Scanner positioned at the opening `{`; consumes through the
/// matching `}` (nested-brace + quote aware) and returns the byte
/// offset of that `}`. Scanner is left just past it. `None` if
/// unbalanced.
fn scan_matching_brace(sc: &mut Scanner) -> Option<usize> {
    let mut depth = 0i32;
    while let Some(c) = sc.peek() {
        match c {
            '"' | '\'' => {
                sc.advance();
                while let Some(ch) = sc.advance() {
                    if ch == '\\' {
                        sc.advance();
                        continue;
                    }
                    if ch == c {
                        break;
                    }
                }
            }
            '{' => {
                depth += 1;
                sc.advance();
            }
            '}' => {
                depth -= 1;
                let here = sc.offset();
                sc.advance();
                if depth == 0 {
                    return Some(here);
                }
            }
            _ => {
                sc.advance();
            }
        }
    }
    None
}

/// True when a brace body opens like a TOML inline table — a key
/// (`ident`) followed by `=` (and not `==`). `lua = "…"`,
/// `background = "#fff"`, `a = 1, b = 2` all match; bare Luau like
/// `tokens.colors.accent`, `darken(x, 0.1)`, `a == b` do not.
fn inner_is_inline_table(inner: &str) -> bool {
    let mut chars = inner.char_indices().peekable();
    let mut saw_ident = false;
    while let Some(&(_, c)) = chars.peek() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            saw_ident = true;
            chars.next();
        } else {
            break;
        }
    }
    if !saw_ident {
        return false;
    }
    while let Some(&(_, c)) = chars.peek() {
        if c == ' ' || c == '\t' {
            chars.next();
        } else {
            break;
        }
    }
    match chars.next() {
        Some((_, '=')) => !matches!(chars.peek(), Some(&(_, '='))),
        _ => false,
    }
}

pub fn parse(source: &str) -> (StyleSheet, Vec<ParseError>) {
    let source = desugar_brace_exprs(source);
    let raw: PrssFile = match toml::from_str(&source) {
        Ok(f) => f,
        Err(e) => {
            return (
                StyleSheet::default(),
                vec![ParseError::file(e.to_string(), "toml-syntax")],
            );
        }
    };

    let mut errors = Vec::new();
    let mut sheet = StyleSheet {
        version: raw.version,
        tokens: TokenOverrides {
            colors: coerce_token_table(&raw.tokens.colors),
            spacing: coerce_token_table(&raw.tokens.spacing),
            radius: coerce_token_table(&raw.tokens.radius),
            typography: coerce_token_table(&raw.tokens.typography),
        },
        classes: IndexMap::new(),
    };

    // Populate classes. Per-class diagnostics for unknown state
    // names and non-scalar property values surface here.
    for (name, raw_class) in raw.class {
        let split = split_class_body(&name, raw_class.body);
        errors.extend(split.errors);
        let extends = split
            .properties
            .get("extends")
            .cloned()
            .filter(|s| !s.is_empty());
        let mut props_no_extends = split.properties;
        props_no_extends.shift_remove("extends");
        sheet.classes.insert(
            name,
            ClassDef {
                extends,
                properties: props_no_extends,
                states: split.states,
            },
        );
    }

    // Post-parse validation: missing-parent + cyclic-extends.
    validate_extends_chains(&sheet, &mut errors);

    (sheet, errors)
}

impl StyleSheet {
    /// **Wave H (`prui-luau-fusion.md` §5.3)** — layer `other` over
    /// `self`, returning the merged sheet. Class definitions and
    /// per-bucket token overrides from `other` win on key collision
    /// (the §5.3 rule: a later sheet — sidecar then inline, or
    /// successive `<style>` blocks — overrides an earlier one).
    /// Declaration order is preserved with `self`'s entries first so
    /// descendant-selector visitation stays deterministic.
    pub fn merged_with(mut self, other: StyleSheet) -> StyleSheet {
        if other.version.is_some() {
            self.version = other.version;
        }
        let bucket = |into: &mut IndexMap<String, String>, from: IndexMap<String, String>| {
            for (k, v) in from {
                into.insert(k, v);
            }
        };
        bucket(&mut self.tokens.colors, other.tokens.colors);
        bucket(&mut self.tokens.spacing, other.tokens.spacing);
        bucket(&mut self.tokens.radius, other.tokens.radius);
        bucket(&mut self.tokens.typography, other.tokens.typography);
        for (name, def) in other.classes {
            self.classes.insert(name, def);
        }
        self
    }
}

// ---------------------------------------------------------------------------
// Resolved-class helpers
// ---------------------------------------------------------------------------

impl StyleSheet {
    /// Flatten one class plus its `extends` chain into a
    /// [`ResolvedClass`]. Returns `None` when the class name is
    /// unknown.
    ///
    /// Property order: deepest parent first → this class last. State
    /// order: same. Cycles are broken silently via a `HashSet`
    /// visited-guard (cycles already surface as `cyclic-extends`
    /// diagnostics at parse time).
    pub fn resolve(&self, name: &str) -> Option<ResolvedClass> {
        self.classes.get(name)?;
        let mut out = ResolvedClass::default();
        let mut visited = HashSet::new();
        self.resolve_into(name, &mut out, &mut visited);
        Some(out)
    }

    fn resolve_into(&self, name: &str, out: &mut ResolvedClass, visited: &mut HashSet<String>) {
        if !visited.insert(name.to_string()) {
            return;
        }
        let Some(class) = self.classes.get(name) else {
            return;
        };
        if let Some(parent) = &class.extends {
            self.resolve_into(parent, out, visited);
        }
        for (k, v) in &class.properties {
            out.properties.push((k.clone(), v.clone()));
        }
        for (state, props) in &class.states {
            for (k, v) in props {
                out.states.push((state.clone(), k.clone(), v.clone()));
            }
        }
    }

    /// Iterate every descendant selector in declaration order
    /// alongside its [`ResolvedClass`] (the `extends` chain
    /// flattened parent-first). A descendant selector is any class
    /// name whose key parses to two or more whitespace-separated
    /// segments (`"btn icon"`, `".btn .icon"`, …) — see
    /// [`selector_segments`].
    ///
    /// The runtime walks the iterator at element-lower time and
    /// applies the resolved class whenever the segment chain
    /// matches the active ancestor-class chain (CSS descendant
    /// matching: each segment must appear in order, but ancestors
    /// between segments don't have to match).
    pub fn descendant_selectors(&self) -> Vec<(&str, Vec<&str>, ResolvedClass)> {
        let mut out = Vec::new();
        for name in self.classes.keys() {
            let segments = selector_segments(name);
            if segments.len() <= 1 {
                continue;
            }
            if let Some(resolved) = self.resolve(name) {
                out.push((name.as_str(), segments, resolved));
            }
        }
        out
    }

    /// Merge another stylesheet's tokens + classes over this one,
    /// later wins. Tokens merge per-bucket (overrides only the keys
    /// the second sheet sets); classes do a name-keyed replacement.
    pub fn merge(&mut self, other: StyleSheet) {
        merge_map(&mut self.tokens.colors, other.tokens.colors);
        merge_map(&mut self.tokens.spacing, other.tokens.spacing);
        merge_map(&mut self.tokens.radius, other.tokens.radius);
        merge_map(&mut self.tokens.typography, other.tokens.typography);
        for (name, class) in other.classes {
            self.classes.insert(name, class);
        }
        if other.version.is_some() {
            self.version = other.version;
        }
    }
}

fn merge_map(into: &mut IndexMap<String, String>, from: IndexMap<String, String>) {
    for (k, v) in from {
        into.insert(k, v);
    }
}

// ---------------------------------------------------------------------------
// Helpers (private)
// ---------------------------------------------------------------------------

/// Coerce a TOML token table into the `IndexMap<String, String>`
/// shape `apply_style_override` consumes. Strings pass through;
/// integers and floats stringify so `radius = 8` and `radius = "8"`
/// land identically.
fn coerce_token_table(input: &IndexMap<String, toml::Value>) -> IndexMap<String, String> {
    let mut out = IndexMap::with_capacity(input.len());
    for (k, v) in input {
        if let Some(s) = toml_value_to_string(v) {
            out.insert(k.clone(), s);
        }
    }
    out
}

/// **Wave F (`prui-luau-fusion.md` §7.9)** — sentinel prefix marking
/// a class/token value as a `{ lua = "…" }` computed expression. The
/// PRSS IR stays a flat `IndexMap<String, String>`; the runtime
/// detects this prefix at apply time and evaluates the trailing
/// expression against the document's Luau frame (falling back to the
/// PRUI expression evaluator). The `\u{1}` bytes can't occur in
/// authored CSS-shaped values, so the encoding is collision-free.
pub const LUA_VALUE_SENTINEL: &str = "\u{1}lua\u{1}";

/// Recognise a `{ lua = "expr" }` single-key TOML table. Returns the
/// inner expression string when matched.
fn lua_table_expr(table: &toml::value::Table) -> Option<&str> {
    if table.len() != 1 {
        return None;
    }
    match table.get("lua")? {
        toml::Value::String(s) => Some(s.as_str()),
        _ => None,
    }
}

fn toml_value_to_string(v: &toml::Value) -> Option<String> {
    match v {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Integer(n) => Some(n.to_string()),
        toml::Value::Float(f) => Some(format!("{f}")),
        toml::Value::Boolean(b) => Some(b.to_string()),
        // **Wave F** — `{ lua = "…" }` → sentinel-encoded expr.
        toml::Value::Table(t) => {
            lua_table_expr(t).map(|expr| format!("{LUA_VALUE_SENTINEL}{expr}"))
        }
        _ => None,
    }
}

/// Output of [`split_class_body`] — base properties, per-state
/// overrides, and per-class diagnostics.
struct SplitClassBody {
    properties: IndexMap<String, String>,
    states: IndexMap<String, IndexMap<String, String>>,
    errors: Vec<ParseError>,
}

/// Split a `[class.NAME]` body into base properties and state
/// sub-tables.
///
/// - Scalar values (string / int / float / bool) → base properties.
/// - Table values → state overrides (key = state name).
/// - Array / DateTime values → dropped + `invalid-property-value` diag.
/// - Unknown state names → dropped + `unknown-state` diag.
fn split_class_body(name: &str, body: IndexMap<String, toml::Value>) -> SplitClassBody {
    let mut properties = IndexMap::new();
    let mut states: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
    let mut errors = Vec::new();

    for (key, value) in body {
        match value {
            // **Wave F** — `key = { lua = "…" }` is a *computed
            // property*, not a state sub-table. Checked before the
            // state-name guard so `background = { lua = "…" }`
            // doesn't trip "unknown state 'lua'".
            toml::Value::Table(ref table) if lua_table_expr(table).is_some() => {
                let expr = lua_table_expr(table).unwrap();
                properties.insert(key, format!("{LUA_VALUE_SENTINEL}{expr}"));
            }
            toml::Value::Table(table) => {
                if !STATE_SUFFIXES.contains(&key.as_str()) {
                    errors.push(ParseError::class(
                        name,
                        format!(
                            "Unknown state '{key}' on class '{name}'. Recognised: {}.",
                            STATE_SUFFIXES.join(", ")
                        ),
                        "unknown-state",
                    ));
                    continue;
                }
                let mut state_props = IndexMap::new();
                for (k, v) in table {
                    if let Some(s) = toml_value_to_string(&v) {
                        state_props.insert(k.to_string(), s);
                    } else {
                        errors.push(ParseError::class(
                            name,
                            format!("Invalid value for '{k}' under [{name}.{key}]"),
                            "invalid-property-value",
                        ));
                    }
                }
                states.insert(key, state_props);
            }
            other => {
                if let Some(s) = toml_value_to_string(&other) {
                    properties.insert(key, s);
                } else {
                    errors.push(ParseError::class(
                        name,
                        format!("Invalid value for '{key}'"),
                        "invalid-property-value",
                    ));
                }
            }
        }
    }

    SplitClassBody {
        properties,
        states,
        errors,
    }
}

fn validate_extends_chains(sheet: &StyleSheet, errors: &mut Vec<ParseError>) {
    for name in sheet.classes.keys() {
        let mut visited = HashSet::new();
        let mut cursor = Some(name.clone());
        while let Some(curr) = cursor.take() {
            if !visited.insert(curr.clone()) {
                errors.push(ParseError::class(
                    name,
                    format!("Cyclic `extends` chain involving class '{curr}'"),
                    "cyclic-extends",
                ));
                break;
            }
            let Some(class) = sheet.classes.get(&curr) else {
                // Parent unknown.
                errors.push(ParseError::class(
                    name,
                    format!("Class '{curr}' (in the extends chain of '{name}') is not defined"),
                    "missing-parent",
                ));
                break;
            };
            cursor = class.extends.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s510_bare_brace_expr_desugars_to_lua_sentinel() {
        let src = r##"
            [class.btn]
            background = { tokens.colors.accent }
            radius     = { tokens.radius.md }
            padding    = { tokens.spacing.sm * 2 }
            hovered    = { background = { darken(tokens.colors.accent, 0.1) }, color = "#fff" }
        "##;
        let (sheet, errs) = parse(src);
        assert!(errs.is_empty(), "errors: {errs:?}");
        let btn = sheet.classes.get("btn").expect("btn class");
        assert_eq!(
            btn.properties.get("background").unwrap(),
            &format!("{LUA_VALUE_SENTINEL}tokens.colors.accent")
        );
        assert_eq!(
            btn.properties.get("padding").unwrap(),
            &format!("{LUA_VALUE_SENTINEL}tokens.spacing.sm * 2")
        );
        // Nested brace-expr inside a state sub-table is desugared too.
        let hov = btn.states.get("hovered").expect("hovered state");
        assert_eq!(
            hov.get("background").unwrap(),
            &format!("{LUA_VALUE_SENTINEL}darken(tokens.colors.accent, 0.1)")
        );
        assert_eq!(hov.get("color").map(String::as_str), Some("#fff"));
        // Legacy `{ lua = "…" }` and real inline tables still parse.
        let (sheet, errs) = parse(
            "[class.x]\nbackground = { lua = \"tokens.colors.accent\" }\nhovered = { background = \"#fff\" }\n",
        );
        assert!(errs.is_empty(), "errors: {errs:?}");
        let x = sheet.classes.get("x").unwrap();
        assert_eq!(
            x.properties.get("background").unwrap(),
            &format!("{LUA_VALUE_SENTINEL}tokens.colors.accent")
        );
        assert_eq!(
            x.states.get("hovered").and_then(|s| s.get("background")),
            Some(&"#fff".to_string())
        );
    }

    #[test]
    fn empty_file_parses_to_empty_stylesheet() {
        let (sheet, errs) = parse("");
        assert!(errs.is_empty());
        assert!(sheet.classes.is_empty());
        assert!(sheet.tokens.colors.is_empty());
    }

    #[test]
    fn token_groups_round_trip() {
        let src = r##"
            [tokens.colors]
            accent = "#7c3aed"
            surface = "#ffffff"

            [tokens.spacing]
            md = 16

            [tokens.radius]
            md = 8
        "##;
        let (sheet, errs) = parse(src);
        assert!(errs.is_empty(), "errors: {errs:?}");
        assert_eq!(
            sheet.tokens.colors.get("accent").map(String::as_str),
            Some("#7c3aed")
        );
        assert_eq!(
            sheet.tokens.colors.get("surface").map(String::as_str),
            Some("#ffffff")
        );
        assert_eq!(
            sheet.tokens.spacing.get("md").map(String::as_str),
            Some("16")
        );
        assert_eq!(sheet.tokens.radius.get("md").map(String::as_str), Some("8"));
    }

    #[test]
    fn class_with_base_properties_only() {
        let src = r##"
            [class.btn]
            background = "#0060c0"
            color = "#ffffff"
            radius = 8
            padding = 12
        "##;
        let (sheet, errs) = parse(src);
        assert!(errs.is_empty());
        let class = sheet.classes.get("btn").unwrap();
        assert!(class.extends.is_none());
        assert_eq!(
            class.properties.get("background").map(String::as_str),
            Some("#0060c0")
        );
        assert_eq!(
            class.properties.get("radius").map(String::as_str),
            Some("8")
        );
        assert_eq!(
            class.properties.get("padding").map(String::as_str),
            Some("12")
        );
        assert!(class.states.is_empty());
    }

    #[test]
    fn extends_lifts_into_classdef() {
        let src = r##"
            [class.btn]
            radius = 8

            [class.btn-primary]
            extends = "btn"
            background = "#0060c0"
        "##;
        let (sheet, errs) = parse(src);
        assert!(errs.is_empty());
        let child = sheet.classes.get("btn-primary").unwrap();
        assert_eq!(child.extends.as_deref(), Some("btn"));
        assert!(!child.properties.contains_key("extends"));
    }

    #[test]
    fn state_sub_table_splits_into_states_map() {
        let src = r##"
            [class.btn]
            background = "#fff"

            [class.btn.hovered]
            background = "#aaa"

            [class.btn.selected]
            background = "#000"
        "##;
        let (sheet, errs) = parse(src);
        assert!(errs.is_empty());
        let class = sheet.classes.get("btn").unwrap();
        assert_eq!(
            class.properties.get("background").map(String::as_str),
            Some("#fff")
        );
        assert_eq!(
            class
                .states
                .get("hovered")
                .and_then(|m| m.get("background"))
                .map(String::as_str),
            Some("#aaa")
        );
        assert_eq!(
            class
                .states
                .get("selected")
                .and_then(|m| m.get("background"))
                .map(String::as_str),
            Some("#000")
        );
    }

    #[test]
    fn unknown_state_name_yields_diagnostic() {
        let src = r##"
            [class.btn]
            background = "#fff"

            [class.btn.weird]
            background = "#aaa"
        "##;
        let (_, errs) = parse(src);
        assert!(errs.iter().any(|e| e.code == "unknown-state"));
    }

    #[test]
    fn missing_parent_yields_diagnostic() {
        let src = r##"
            [class.btn-primary]
            extends = "no-such-class"
            background = "#0060c0"
        "##;
        let (_, errs) = parse(src);
        assert!(errs.iter().any(|e| e.code == "missing-parent"));
    }

    #[test]
    fn cyclic_extends_yields_diagnostic() {
        let src = r##"
            [class.a]
            extends = "b"

            [class.b]
            extends = "a"
        "##;
        let (_, errs) = parse(src);
        assert!(errs.iter().any(|e| e.code == "cyclic-extends"));
    }

    #[test]
    fn toml_syntax_error_aborts_with_one_diagnostic() {
        let src = "[class.btn\nbackground = \"#fff\"";
        let (sheet, errs) = parse(src);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].code, "toml-syntax");
        assert!(sheet.classes.is_empty());
    }

    #[test]
    fn resolve_flattens_extends_chain_parent_first() {
        let src = r##"
            [class.row]
            direction = "row"
            gap = 8

            [class.btn]
            extends = "row"
            background = "#fff"
            radius = 8

            [class.btn-primary]
            extends = "btn"
            background = "#0060c0"
            color = "#ffffff"
        "##;
        let (sheet, errs) = parse(src);
        assert!(errs.is_empty());
        let resolved = sheet.resolve("btn-primary").expect("class exists");
        // Properties: row (direction, gap) → btn (background, radius) → btn-primary (background, color)
        let keys: Vec<&str> = resolved
            .properties
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(
            keys,
            vec![
                "direction",
                "gap",
                "background",
                "radius",
                "background",
                "color"
            ]
        );
        // Later assignment wins: the last `background` is `#0060c0`.
        let last_bg = resolved
            .properties
            .iter()
            .rev()
            .find(|(k, _)| k == "background")
            .map(|(_, v)| v.as_str());
        assert_eq!(last_bg, Some("#0060c0"));
    }

    #[test]
    fn resolve_unknown_class_returns_none() {
        let sheet = StyleSheet::default();
        assert!(sheet.resolve("not-a-class").is_none());
    }

    #[test]
    fn merge_combines_tokens_and_classes_later_wins() {
        let (mut a, _) = parse(
            r##"
            [tokens.colors]
            accent = "#000000"

            [class.btn]
            background = "#fff"
        "##,
        );
        let (b, _) = parse(
            r##"
            [tokens.colors]
            accent = "#ff0000"
            surface = "#fafafa"

            [class.btn]
            background = "#aaa"

            [class.large]
            font-size = 18
        "##,
        );
        a.merge(b);
        assert_eq!(
            a.tokens.colors.get("accent").map(String::as_str),
            Some("#ff0000")
        );
        assert_eq!(
            a.tokens.colors.get("surface").map(String::as_str),
            Some("#fafafa")
        );
        assert_eq!(
            a.classes
                .get("btn")
                .unwrap()
                .properties
                .get("background")
                .map(String::as_str),
            Some("#aaa")
        );
        assert!(a.classes.contains_key("large"));
    }

    #[test]
    fn selector_segments_splits_on_whitespace_strips_dots() {
        assert_eq!(selector_segments("btn"), vec!["btn"]);
        assert_eq!(selector_segments("btn icon"), vec!["btn", "icon"]);
        assert_eq!(selector_segments(".btn .icon"), vec!["btn", "icon"]);
        assert_eq!(
            selector_segments("  card  body  title  "),
            vec!["card", "body", "title"]
        );
        assert!(selector_segments("").is_empty());
    }

    #[test]
    fn is_descendant_selector_branches_on_segment_count() {
        assert!(!is_descendant_selector("btn"));
        assert!(is_descendant_selector("btn icon"));
        assert!(is_descendant_selector(".btn .icon"));
    }

    #[test]
    fn descendant_selectors_returns_multi_segment_only() {
        let src = r##"
            [class.btn]
            background = "#fff"

            [class."btn icon"]
            color = "#0060c0"

            [class.".card .title"]
            color = "#000"
        "##;
        let (sheet, errs) = parse(src);
        assert!(errs.is_empty(), "errors: {errs:?}");
        let selectors = sheet.descendant_selectors();
        let names: Vec<&str> = selectors.iter().map(|(n, _, _)| *n).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"btn icon"));
        assert!(names.contains(&".card .title"));
        // Segments stripped of leading dots.
        let by_name: std::collections::HashMap<_, _> = selectors
            .iter()
            .map(|(n, segs, _)| (*n, segs.clone()))
            .collect();
        assert_eq!(by_name.get("btn icon").unwrap(), &vec!["btn", "icon"]);
        assert_eq!(by_name.get(".card .title").unwrap(), &vec!["card", "title"]);
    }

    #[test]
    fn integer_and_float_property_values_stringify() {
        let src = r##"
            [class.btn]
            radius = 8
            padding = 12.5
            ghost = true
        "##;
        let (sheet, errs) = parse(src);
        assert!(errs.is_empty());
        let class = sheet.classes.get("btn").unwrap();
        assert_eq!(
            class.properties.get("radius").map(String::as_str),
            Some("8")
        );
        assert_eq!(
            class.properties.get("padding").map(String::as_str),
            Some("12.5")
        );
        assert_eq!(
            class.properties.get("ghost").map(String::as_str),
            Some("true")
        );
    }
}
