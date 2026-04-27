//! `LuauSyntaxProvider` — full_moon-backed diagnostics, completions,
//! and hover for Luau.

use crate::language::syntax::{
    CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity, HoverInfo, SchemaContext,
    SyntaxProvider, TextRange,
};

#[derive(Debug, Clone, Default)]
pub struct LuauSyntaxProvider;

impl LuauSyntaxProvider {
    pub fn new() -> Self {
        Self
    }
}

const LUAU_KEYWORDS: &[&str] = &[
    "and", "break", "continue", "do", "else", "elseif", "end", "false", "for", "function", "if",
    "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "type", "until",
    "while", "export",
];

const LUAU_BUILTINS: &[(&str, &str)] = &[
    ("print", "print(...: any) -> ()"),
    ("error", "error(message: string, level: number?) -> never"),
    ("warn", "warn(...: any) -> ()"),
    ("assert", "assert(condition: any, message: string?) -> any"),
    ("type", "type(value: any) -> string"),
    ("typeof", "typeof(value: any) -> string"),
    ("tostring", "tostring(value: any) -> string"),
    ("tonumber", "tonumber(value: any) -> number?"),
    ("select", "select(index: number | string, ...: any) -> any"),
    ("pairs", "pairs(table: {[K]: V}) -> ((table, K?) -> (K, V), table, nil)"),
    ("ipairs", "ipairs(table: {V}) -> ((table, number) -> (number, V), table, number)"),
    ("next", "next(table: {[K]: V}, index: K?) -> (K?, V)"),
    ("rawget", "rawget(table: table, key: any) -> any"),
    ("rawset", "rawset(table: table, key: any, value: any) -> table"),
    ("rawlen", "rawlen(table: table) -> number"),
    ("rawequal", "rawequal(a: any, b: any) -> boolean"),
    ("setmetatable", "setmetatable(table: table, mt: table?) -> table"),
    ("getmetatable", "getmetatable(table: table) -> table?"),
    ("pcall", "pcall(f: (...any) -> ...any, ...: any) -> (boolean, ...any)"),
    ("xpcall", "xpcall(f: (...any) -> ...any, handler: (err: any) -> (), ...: any) -> (boolean, ...any)"),
    ("require", "require(module: any) -> any"),
    ("unpack", "unpack(table: {V}, i: number?, j: number?) -> ...V"),
    ("table.insert", "table.insert(t: {V}, pos: number?, value: V) -> ()"),
    ("table.remove", "table.remove(t: {V}, pos: number?) -> V?"),
    ("table.sort", "table.sort(t: {V}, comp: ((V, V) -> boolean)?) -> ()"),
    ("table.concat", "table.concat(t: {string}, sep: string?, i: number?, j: number?) -> string"),
    ("table.find", "table.find(t: {V}, value: V, init: number?) -> number?"),
    ("table.create", "table.create(count: number, value: V?) -> {V}"),
    ("table.move", "table.move(a1: {V}, f: number, e: number, t: number, a2: {V}?) -> {V}"),
    ("table.freeze", "table.freeze(t: {[K]: V}) -> {[K]: V}"),
    ("table.isfrozen", "table.isfrozen(t: table) -> boolean"),
    ("table.clone", "table.clone(t: {[K]: V}) -> {[K]: V}"),
    ("string.format", "string.format(fmt: string, ...: any) -> string"),
    ("string.len", "string.len(s: string) -> number"),
    ("string.sub", "string.sub(s: string, i: number, j: number?) -> string"),
    ("string.find", "string.find(s: string, pattern: string, init: number?, plain: boolean?) -> (number?, number?, ...string)"),
    ("string.match", "string.match(s: string, pattern: string, init: number?) -> ...string?"),
    ("string.gmatch", "string.gmatch(s: string, pattern: string) -> () -> ...string"),
    ("string.gsub", "string.gsub(s: string, pattern: string, repl: string | table | ((...string) -> string), n: number?) -> (string, number)"),
    ("string.rep", "string.rep(s: string, n: number, sep: string?) -> string"),
    ("string.upper", "string.upper(s: string) -> string"),
    ("string.lower", "string.lower(s: string) -> string"),
    ("string.byte", "string.byte(s: string, i: number?, j: number?) -> ...number"),
    ("string.char", "string.char(...: number) -> string"),
    ("string.split", "string.split(s: string, sep: string?) -> {string}"),
    ("math.abs", "math.abs(x: number) -> number"),
    ("math.ceil", "math.ceil(x: number) -> number"),
    ("math.floor", "math.floor(x: number) -> number"),
    ("math.max", "math.max(...: number) -> number"),
    ("math.min", "math.min(...: number) -> number"),
    ("math.sqrt", "math.sqrt(x: number) -> number"),
    ("math.random", "math.random(m: number?, n: number?) -> number"),
    ("math.clamp", "math.clamp(x: number, min: number, max: number) -> number"),
    ("math.round", "math.round(x: number) -> number"),
    ("math.sign", "math.sign(x: number) -> number"),
    ("math.noise", "math.noise(x: number, y: number?, z: number?) -> number"),
];

impl SyntaxProvider for LuauSyntaxProvider {
    fn name(&self) -> &str {
        "prism:luau"
    }

    fn diagnose(&self, source: &str, _context: Option<&SchemaContext>) -> Vec<Diagnostic> {
        let lua_version = full_moon::LuaVersion::luau();
        let result = full_moon::parse_fallible(source, lua_version);
        match result.into_result() {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .into_iter()
                .map(|e| {
                    let range = error_range(&e);
                    Diagnostic {
                        message: e.to_string(),
                        severity: DiagnosticSeverity::Error,
                        range,
                        code: None,
                    }
                })
                .collect(),
        }
    }

    fn complete(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Vec<CompletionItem> {
        let prefix = extract_word_prefix(source, offset);
        if prefix.is_empty() {
            return Vec::new();
        }

        let mut items = Vec::new();

        for &kw in LUAU_KEYWORDS {
            if kw.starts_with(&prefix) {
                items.push(CompletionItem {
                    label: kw.to_string(),
                    kind: CompletionKind::Keyword,
                    detail: Some("keyword".into()),
                    documentation: None,
                    sort_order: Some(200),
                    replace_range: None,
                    insert_text: None,
                });
            }
        }

        for &(name, sig) in LUAU_BUILTINS {
            if name.starts_with(&prefix) {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: CompletionKind::Function,
                    detail: Some(sig.to_string()),
                    documentation: None,
                    sort_order: Some(100),
                    replace_range: None,
                    insert_text: None,
                });
            }
        }

        if let Some(ctx) = context {
            for field in &ctx.fields {
                if field.id.starts_with(&prefix) {
                    items.push(CompletionItem {
                        label: field.id.clone(),
                        kind: CompletionKind::Field,
                        detail: Some(format!("{:?}", field.field_type)),
                        documentation: field.description.clone(),
                        sort_order: Some(50),
                        replace_range: None,
                        insert_text: None,
                    });
                }
            }

            for sig in &ctx.signals {
                let handler_name = format!("on_{}", sig.name.replace('-', "_"));
                if handler_name.starts_with(&prefix) {
                    let params = if sig.payload.is_empty() {
                        String::new()
                    } else {
                        sig.payload
                            .iter()
                            .map(|p| format!("{}: {}", p.name, p.luau_type))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    let detail = format!("fun({params}) -> ()");
                    items.push(CompletionItem {
                        label: handler_name,
                        kind: CompletionKind::Function,
                        detail: Some(detail),
                        documentation: Some(sig.description.clone()),
                        sort_order: Some(30),
                        replace_range: None,
                        insert_text: None,
                    });
                }
            }
        }

        items.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.label.cmp(&b.label)));
        items
    }

    fn hover(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Option<HoverInfo> {
        let (word, start, end) = extract_word_at(source, offset)?;

        for &(name, sig) in LUAU_BUILTINS {
            if name == word {
                return Some(HoverInfo {
                    range: TextRange { start, end },
                    contents: sig.to_string(),
                });
            }
        }

        if LUAU_KEYWORDS.contains(&word.as_str()) {
            return Some(HoverInfo {
                range: TextRange { start, end },
                contents: format!("{word} (keyword)"),
            });
        }

        if let Some(ctx) = context {
            for sig in &ctx.signals {
                let handler_name = format!("on_{}", sig.name.replace('-', "_"));
                if word == handler_name {
                    let params = if sig.payload.is_empty() {
                        "()".to_string()
                    } else {
                        let p = sig
                            .payload
                            .iter()
                            .map(|f| format!("{}: {}", f.name, f.luau_type))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("({p})")
                    };
                    return Some(HoverInfo {
                        range: TextRange { start, end },
                        contents: format!(
                            "signal handler `{}`\n{}\nfun{params} -> ()",
                            sig.name, sig.description
                        ),
                    });
                }
            }
        }

        None
    }
}

fn extract_word_prefix(source: &str, offset: usize) -> String {
    let before = &source[..offset.min(source.len())];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    before[start..].to_string()
}

fn extract_word_at(source: &str, offset: usize) -> Option<(String, usize, usize)> {
    if offset > source.len() {
        return None;
    }
    let before = &source[..offset];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    let after = &source[offset..];
    let end_offset = after
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .unwrap_or(after.len());
    let end = offset + end_offset;
    if start == end {
        return None;
    }
    Some((source[start..end].to_string(), start, end))
}

fn error_range(error: &full_moon::Error) -> TextRange {
    match error {
        full_moon::Error::AstError(ast_err) => {
            let token = ast_err.token();
            let start = token.start_position();
            let end = token.end_position();
            TextRange {
                start: start.bytes(),
                end: end.bytes(),
            }
        }
        full_moon::Error::TokenizerError(tok_err) => TextRange {
            start: tok_err.position().bytes(),
            end: tok_err.position().bytes() + 1,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_name_matches_contribution_id() {
        let provider = LuauSyntaxProvider::new();
        assert_eq!(provider.name(), "prism:luau");
    }

    #[test]
    fn diagnose_clean_source() {
        let provider = LuauSyntaxProvider::new();
        assert!(provider.diagnose("return 1 + 2", None).is_empty());
    }

    #[test]
    fn diagnose_reports_syntax_errors() {
        let provider = LuauSyntaxProvider::new();
        let diags = provider.diagnose("if then end", None);
        assert!(!diags.is_empty());
        assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
    }

    #[test]
    fn complete_keywords() {
        let provider = LuauSyntaxProvider::new();
        let items = provider.complete("loc", 3, None);
        assert!(items.iter().any(|i| i.label == "local"));
    }

    #[test]
    fn complete_builtins() {
        let provider = LuauSyntaxProvider::new();
        let items = provider.complete("pri", 3, None);
        assert!(items.iter().any(|i| i.label == "print"));
    }

    #[test]
    fn complete_table_methods() {
        let provider = LuauSyntaxProvider::new();
        let items = provider.complete("table.ins", 9, None);
        assert!(items.iter().any(|i| i.label == "table.insert"));
    }

    #[test]
    fn complete_empty_prefix_returns_nothing() {
        let provider = LuauSyntaxProvider::new();
        let items = provider.complete("", 0, None);
        assert!(items.is_empty());
    }

    #[test]
    fn hover_builtin() {
        let provider = LuauSyntaxProvider::new();
        let hover = provider.hover("print(42)", 2, None);
        assert!(hover.is_some());
        let info = hover.unwrap();
        assert!(info.contents.contains("print"));
    }

    #[test]
    fn hover_keyword() {
        let provider = LuauSyntaxProvider::new();
        let hover = provider.hover("local x = 1", 2, None);
        assert!(hover.is_some());
        assert!(hover.unwrap().contents.contains("keyword"));
    }

    #[test]
    fn hover_unknown_returns_none() {
        let provider = LuauSyntaxProvider::new();
        let hover = provider.hover("myCustomVar = 1", 5, None);
        assert!(hover.is_none());
    }

    #[test]
    fn complete_with_schema_context() {
        use crate::foundation::object_model::types::EntityFieldDef;
        let provider = LuauSyntaxProvider::new();
        let field: EntityFieldDef =
            serde_json::from_value(serde_json::json!({"id": "status", "type": "text"})).unwrap();
        let ctx = SchemaContext {
            object_type: "task".into(),
            fields: vec![field],
            signals: vec![],
        };
        let items = provider.complete("sta", 3, Some(&ctx));
        assert!(items.iter().any(|i| i.label == "status"));
    }

    #[test]
    fn complete_signal_handlers() {
        use crate::language::syntax::{SignalContext, SignalPayloadField};
        let provider = LuauSyntaxProvider::new();
        let ctx = SchemaContext {
            object_type: "button".into(),
            fields: vec![],
            signals: vec![
                SignalContext {
                    name: "clicked".into(),
                    description: "Fires on click".into(),
                    payload: vec![
                        SignalPayloadField {
                            name: "x".into(),
                            luau_type: "number".into(),
                        },
                        SignalPayloadField {
                            name: "y".into(),
                            luau_type: "number".into(),
                        },
                    ],
                },
                SignalContext {
                    name: "hover-ended".into(),
                    description: "Fires on hover leave".into(),
                    payload: vec![],
                },
            ],
        };
        let items = provider.complete("on_", 3, Some(&ctx));
        assert!(
            items.iter().any(|i| i.label == "on_clicked"),
            "should complete on_clicked"
        );
        assert!(
            items.iter().any(|i| i.label == "on_hover_ended"),
            "should complete on_hover_ended"
        );

        let clicked = items.iter().find(|i| i.label == "on_clicked").unwrap();
        assert!(clicked.detail.as_ref().unwrap().contains("x: number"));
        assert_eq!(clicked.sort_order, Some(30));
    }

    #[test]
    fn hover_signal_handler() {
        use crate::language::syntax::{SignalContext, SignalPayloadField};
        let provider = LuauSyntaxProvider::new();
        let ctx = SchemaContext {
            object_type: "button".into(),
            fields: vec![],
            signals: vec![SignalContext {
                name: "clicked".into(),
                description: "Fires on click".into(),
                payload: vec![SignalPayloadField {
                    name: "x".into(),
                    luau_type: "number".into(),
                }],
            }],
        };
        let hover = provider.hover("on_clicked()", 5, Some(&ctx));
        assert!(hover.is_some());
        let info = hover.unwrap();
        assert!(info.contents.contains("signal handler"));
        assert!(info.contents.contains("clicked"));
        assert!(info.contents.contains("x: number"));
    }
}
