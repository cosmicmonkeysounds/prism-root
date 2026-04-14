//! Case-conversion utilities for codegen.
//!
//! Port of `language/syntax/case-utils.ts`. Single source of truth
//! for `to_camel_case` / `to_pascal_case` / `to_screaming_snake` /
//! `to_snake_case` plus their safe-identifier variants.

/// Split a string into words, handling snake_case, kebab-case,
/// dot.case, camelCase, PascalCase, and SCREAMING_SNAKE.
fn split_words(s: &str) -> Vec<String> {
    // First collapse non-alphanumerics to spaces.
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();

    // Walk with camelCase-boundary + ACRONYMBoundary splitting.
    let chars: Vec<char> = cleaned.chars().collect();
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();

    for i in 0..chars.len() {
        let ch = chars[i];
        if ch == ' ' {
            if !current.is_empty() {
                pieces.push(std::mem::take(&mut current));
            }
            continue;
        }

        if !current.is_empty() {
            let prev = chars[i - 1];
            // camelCase boundary: aB
            let camel_boundary = prev.is_ascii_lowercase() && ch.is_ascii_uppercase();
            // Acronym boundary: HTMLParser → HTML|Parser.
            // Break when we go UPPER, UPPER, lower (split before the
            // second upper).
            let acronym_boundary = prev.is_ascii_uppercase()
                && ch.is_ascii_uppercase()
                && i + 1 < chars.len()
                && chars[i + 1].is_ascii_lowercase();
            if camel_boundary || acronym_boundary {
                pieces.push(std::mem::take(&mut current));
            }
        }
        current.push(ch);
    }
    if !current.is_empty() {
        pieces.push(current);
    }

    pieces.into_iter().filter(|w| !w.is_empty()).collect()
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let rest: String = chars.as_str().to_lowercase();
            first.to_ascii_uppercase().to_string() + &rest
        }
    }
}

/// Strip non-identifier characters; ensure the result starts with
/// a letter or underscore.
pub fn safe_identifier(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        return "_unknown".to_string();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

pub fn to_camel_case(s: &str) -> String {
    let words = split_words(s);
    if words.is_empty() {
        return "_unknown".to_string();
    }
    words
        .into_iter()
        .enumerate()
        .map(|(i, w)| {
            if i == 0 {
                w.to_lowercase()
            } else {
                capitalize(&w)
            }
        })
        .collect()
}

pub fn to_pascal_case(s: &str) -> String {
    let words = split_words(s);
    if words.is_empty() {
        return "_Unknown".to_string();
    }
    words.iter().map(|w| capitalize(w)).collect()
}

pub fn to_screaming_snake(s: &str) -> String {
    let words = split_words(s);
    if words.is_empty() {
        return "_UNKNOWN".to_string();
    }
    words
        .into_iter()
        .map(|w| w.to_uppercase())
        .collect::<Vec<_>>()
        .join("_")
}

pub fn to_snake_case(s: &str) -> String {
    let words = split_words(s);
    if words.is_empty() {
        return "_unknown".to_string();
    }
    words
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

pub fn to_camel_ident(s: &str) -> String {
    to_camel_case(&safe_identifier(s))
}

pub fn to_pascal_ident(s: &str) -> String {
    to_pascal_case(&safe_identifier(s))
}

pub fn to_screaming_snake_ident(s: &str) -> String {
    to_screaming_snake(&safe_identifier(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_case_conversions() {
        assert_eq!(to_camel_case("hello_world"), "helloWorld");
        assert_eq!(to_camel_case("HelloWorld"), "helloWorld");
        assert_eq!(to_camel_case("hello-world"), "helloWorld");
        assert_eq!(to_camel_case("hello.world"), "helloWorld");
        assert_eq!(to_camel_case("HTMLParser"), "htmlParser");
    }

    #[test]
    fn pascal_case_conversions() {
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(to_pascal_case("helloWorld"), "HelloWorld");
        assert_eq!(to_pascal_case("HTMLParser"), "HtmlParser");
    }

    #[test]
    fn screaming_snake_conversions() {
        assert_eq!(to_screaming_snake("helloWorld"), "HELLO_WORLD");
        assert_eq!(to_screaming_snake("hello-world"), "HELLO_WORLD");
    }

    #[test]
    fn snake_case_conversions() {
        assert_eq!(to_snake_case("helloWorld"), "hello_world");
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("HTMLParser"), "html_parser");
    }

    #[test]
    fn safe_identifier_leading_digit() {
        assert_eq!(safe_identifier("123abc"), "_123abc");
        assert_eq!(safe_identifier("foo-bar"), "foo_bar");
        assert_eq!(safe_identifier(""), "_unknown");
    }
}
