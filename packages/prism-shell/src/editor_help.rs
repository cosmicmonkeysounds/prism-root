//! Built-in help entries for the in-shell code editor.
//!
//! Populates a [`HelpRegistry`] with one entry per known Luau /
//! Rust / JavaScript keyword (plus a handful of common identifiers).
//! The shell's hover detector resolves the byte under the pointer
//! to a token through [`prism_ui_runtime::syntax::token_at`], builds
//! a help id of the shape `editor.<lang>.keyword.<lexeme>`, and
//! looks it up here. Misses surface no tooltip (the editor stays
//! silent rather than guessing).
//!
//! Lives in the shell rather than `prism-ui-runtime` because the
//! registry is a host concern — the runtime tokenizer only emits
//! `TokenKind`, and the shell is what owns the help slot
//! (`state.overlay.help_tooltip`).
use prism_core::{HelpEntry, HelpRegistry};

/// Builder of an editor-flavoured `HelpRegistry` seeded with the
/// well-known keywords for every language the syntax module
/// recognises. Hosts can layer their own entries on top by calling
/// [`HelpRegistry::register`] after this returns.
pub fn editor_help_registry() -> HelpRegistry {
    let mut reg = HelpRegistry::new();
    register_luau(&mut reg);
    register_rust(&mut reg);
    register_javascript(&mut reg);
    reg
}

fn register_luau(reg: &mut HelpRegistry) {
    let entries: &[(&str, &str, &str)] = &[
        ("local", "local", "Declare a variable scoped to the current block."),
        ("function", "function", "Define a function. Pair with `end` to close the body."),
        ("if", "if", "Begin a conditional. Pair with `then` / optional `elseif` / `else` / `end`."),
        ("then", "then", "Marker that closes an `if`/`elseif` condition before the branch body."),
        ("else", "else", "Fallback branch in a conditional. No condition; runs when no `if`/`elseif` matched."),
        ("elseif", "elseif", "Additional condition in an `if` chain."),
        ("end", "end", "Closes a `function` / `if` / `for` / `while` / `do` block."),
        ("for", "for", "Loop over a range or generic iterator."),
        ("in", "in", "Separates the iteration variables from the iterator in a `for` loop."),
        ("while", "while", "Loop while the condition is truthy."),
        ("do", "do", "Open a block. Used by `for`/`while`, or standalone for scoping."),
        ("repeat", "repeat", "Loop until the condition becomes truthy. Pair with `until`."),
        ("until", "until", "Closes a `repeat` block with its exit condition."),
        ("return", "return", "Return from the current function with optional values."),
        ("break", "break", "Exit the innermost loop."),
        ("continue", "continue", "Skip to the next iteration of the innermost loop."),
        ("and", "and", "Logical AND — short-circuiting."),
        ("or", "or", "Logical OR — short-circuiting."),
        ("not", "not", "Logical NOT — boolean inversion."),
        // Literal-style identifiers (`nil` / `true` / `false`) tokenize
        // as `Type` rather than `Keyword`, but users still hover them.
        ("nil", "nil", "The absent / undefined value. Falsy in boolean context."),
        ("true", "true", "Boolean true. The other truthy values are everything but `nil` / `false`."),
        ("false", "false", "Boolean false. With `nil`, one of Luau's two falsy values."),
        ("self", "self", "Implicit first argument inside a method declared with `:`."),
    ];
    for (lex, title, summary) in entries {
        reg.register(HelpEntry::new(
            format!("editor.luau.keyword.{lex}"),
            *title,
            *summary,
        ));
    }
}

fn register_rust(reg: &mut HelpRegistry) {
    let entries: &[(&str, &str, &str)] = &[
        ("fn", "fn", "Declare a function."),
        ("let", "let", "Bind a name to a value. Add `mut` for mutability."),
        ("mut", "mut", "Marks a binding (or reference) as mutable."),
        ("const", "const", "Compile-time constant."),
        ("static", "static", "A value with a fixed location in memory and `'static` lifetime."),
        ("if", "if", "Conditional expression. Both arms must have compatible types."),
        ("else", "else", "Fallback branch in an `if` expression."),
        ("match", "match", "Pattern-match expression. Exhaustive over the input's type."),
        ("for", "for", "Iterator loop. Desugars to `IntoIterator` + `Iterator::next`."),
        ("while", "while", "Loop while the condition is true."),
        ("loop", "loop", "Infinite loop. Use `break` to exit (optionally with a value)."),
        ("return", "return", "Return from the current function."),
        ("break", "break", "Exit the innermost loop (optionally with a value for `loop`)."),
        ("continue", "continue", "Skip to the next iteration."),
        ("struct", "struct", "Aggregate type with named fields."),
        ("enum", "enum", "Tagged union — one of several variants."),
        ("trait", "trait", "Interface a type can implement."),
        ("impl", "impl", "Implement methods on a type or a trait."),
        ("pub", "pub", "Visibility modifier — exposes the item to outer modules."),
        ("use", "use", "Bring a path into the local namespace."),
        ("mod", "mod", "Declare a submodule."),
        ("self", "self", "The receiver of a method, or the current module path."),
        ("Self", "Self", "The implementing type inside an `impl` block."),
        ("crate", "crate", "Root of the current crate's namespace."),
        ("super", "super", "Parent module."),
        ("move", "move", "Capture variables by value in a closure."),
        ("async", "async", "Mark a function or block as returning a future."),
        ("await", "await", "Suspend until the awaited future resolves."),
        ("dyn", "dyn", "Trait-object marker — dynamic dispatch."),
        ("ref", "ref", "Bind by reference in a pattern."),
        ("true", "true", "Boolean true."),
        ("false", "false", "Boolean false."),
        ("Some", "Some", "`Option<T>` variant carrying a value."),
        ("None", "None", "`Option<T>` variant for absence."),
        ("Ok", "Ok", "`Result<T, E>` success variant."),
        ("Err", "Err", "`Result<T, E>` failure variant."),
    ];
    for (lex, title, summary) in entries {
        reg.register(HelpEntry::new(
            format!("editor.rust.keyword.{lex}"),
            *title,
            *summary,
        ));
    }
}

fn register_javascript(reg: &mut HelpRegistry) {
    let entries: &[(&str, &str, &str)] = &[
        ("let", "let", "Block-scoped variable. Reassignable; not hoisted in the temporal-dead-zone sense."),
        ("const", "const", "Block-scoped immutable binding (the *binding* — object contents are still mutable)."),
        ("var", "var", "Function-scoped variable. Prefer `let` / `const` in new code."),
        ("function", "function", "Function declaration / expression."),
        ("return", "return", "Return from the current function."),
        ("if", "if", "Conditional statement."),
        ("else", "else", "Fallback branch in an `if` statement."),
        ("for", "for", "C-style for loop or `for...of` / `for...in`."),
        ("while", "while", "Condition-tested-first loop."),
        ("do", "do", "Body-then-condition loop (`do { … } while (…)`)."),
        ("class", "class", "Class declaration (ES2015+ sugar over prototypes)."),
        ("extends", "extends", "Inheritance — base-class declaration on a `class`."),
        ("new", "new", "Invoke a constructor."),
        ("this", "this", "The receiver of the enclosing call. Arrow functions inherit from the lexical scope."),
        ("super", "super", "Reference to the parent class inside a `class` body."),
        ("import", "import", "ES-module import declaration."),
        ("export", "export", "ES-module export declaration."),
        ("async", "async", "Mark a function as returning a Promise."),
        ("await", "await", "Suspend on a Promise inside an `async` function."),
        ("try", "try", "Begin a guarded block. Pair with `catch` / `finally`."),
        ("catch", "catch", "Handle exceptions from the preceding `try` block."),
        ("finally", "finally", "Always run after `try` / `catch`, even on early return."),
        ("throw", "throw", "Raise an exception."),
        ("typeof", "typeof", "Runtime type tag — `string` / `number` / `object` / etc."),
        ("instanceof", "instanceof", "Check whether an object's prototype chain contains a constructor."),
        ("in", "in", "Membership test inside a `for...in` loop or `key in object`."),
        ("of", "of", "Element iteration inside a `for...of` loop."),
        ("true", "true", "Boolean true."),
        ("false", "false", "Boolean false."),
        ("null", "null", "Intentional absence of an object value."),
        ("undefined", "undefined", "Uninitialised or non-existent value."),
    ];
    for (lex, title, summary) in entries {
        reg.register(HelpEntry::new(
            format!("editor.javascript.keyword.{lex}"),
            *title,
            *summary,
        ));
    }
}

/// Resolve a `(language, lexeme)` pair to a help id under the
/// editor namespace. Mirrors the construction inside the registry
/// seed so the lookup-side never drifts from the registration-side.
pub fn help_id_for(language: &str, lexeme: &str) -> Option<String> {
    let lang = match language.trim().to_ascii_lowercase().as_str() {
        "luau" | "lua" => "luau",
        "rust" | "rs" => "rust",
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" => "javascript",
        _ => return None,
    };
    Some(format!("editor.{lang}.keyword.{lexeme}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_well_known_luau_keywords() {
        let reg = editor_help_registry();
        assert!(reg.get("editor.luau.keyword.local").is_some());
        assert!(reg.get("editor.luau.keyword.function").is_some());
    }

    #[test]
    fn help_id_for_normalises_language() {
        assert_eq!(
            help_id_for("luau", "local"),
            Some("editor.luau.keyword.local".into())
        );
        assert_eq!(
            help_id_for("TypeScript", "let"),
            Some("editor.javascript.keyword.let".into())
        );
        assert!(help_id_for("klingon", "qapla").is_none());
    }
}
