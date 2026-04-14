//! String-case helpers used by codegen and route generation. Port
//! of `foundation/object-model/str.ts`.

/// Convert kebab-case, snake_case, or whitespace-separated text to
/// `PascalCase`. The implementation matches the legacy regex
/// behaviour: promote the first character of the string and the
/// first alphabetic character after any `-`, `_`, or whitespace
/// separator, then strip the separators.
pub fn pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = true;
    for ch in s.chars() {
        if ch == '-' || ch == '_' || ch.is_whitespace() {
            upper_next = true;
            continue;
        }
        if upper_next && ch.is_ascii_alphabetic() {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
            upper_next = false;
        }
    }
    out
}

/// Convert kebab-case/snake_case/whitespace input to `camelCase`.
pub fn camel(s: &str) -> String {
    let p = pascal(s);
    let mut chars = p.chars();
    match chars.next() {
        Some(first) => {
            let mut out: String = first.to_lowercase().collect();
            out.push_str(chars.as_str());
            out
        }
        None => String::new(),
    }
}

/// Very light singularization for `operationId` generation. Matches
/// the three rules baked into the legacy code: `ies -> y`,
/// `ses -> s`, trailing `s` stripped unless preceded by another `s`.
pub fn singular(s: &str) -> String {
    if let Some(stem) = s.strip_suffix("ies") {
        return format!("{stem}y");
    }
    if let Some(stem) = s.strip_suffix("ses") {
        return format!("{stem}s");
    }
    if let Some(stem) = s.strip_suffix('s') {
        if !s.ends_with("ss") {
            return stem.to_string();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_promotes_first_letter_after_separators() {
        assert_eq!(pascal("hello-world"), "HelloWorld");
        assert_eq!(pascal("foo_bar_baz"), "FooBarBaz");
        assert_eq!(pascal("already Pascal"), "AlreadyPascal");
        assert_eq!(pascal(""), "");
    }

    #[test]
    fn camel_lowercases_first_char() {
        assert_eq!(camel("hello-world"), "helloWorld");
        assert_eq!(camel("foo"), "foo");
    }

    #[test]
    fn singular_matches_legacy_rules() {
        assert_eq!(singular("cities"), "city");
        assert_eq!(singular("processes"), "process");
        assert_eq!(singular("tasks"), "task");
        // Legacy behaviour: only the double-`ss` suffix is protected,
        // so "class" stays put but "status" loses its trailing `s`.
        assert_eq!(singular("class"), "class");
        assert_eq!(singular("status"), "statu");
        assert_eq!(singular("cats"), "cat");
    }
}
