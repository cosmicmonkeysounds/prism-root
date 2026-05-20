use super::*;

fn parse_ok(src: &str) -> Document {
    let (doc, errs) = parse(src);
    assert!(errs.is_empty(), "unexpected parse errors: {errs:?}");
    doc
}

#[test]
fn parses_self_closing_element() {
    let doc = parse_ok(r#"<spacer/>"#);
    assert_eq!(doc.nodes.len(), 1);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!("expected element");
    };
    assert_eq!(el.tag, "spacer");
    assert!(el.self_closing);
    assert!(el.children.is_empty());
    assert!(el.attributes.is_empty());
}

#[test]
fn parses_element_with_string_attribute() {
    let doc = parse_ok(r#"<button label="Save"/>"#);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!()
    };
    assert_eq!(el.tag, "button");
    assert_eq!(el.attributes.len(), 1);
    assert_eq!(el.attributes[0].name.raw, "label");
    assert_eq!(el.attributes[0].name.namespace, AttributeNamespace::Bare);
    match &el.attributes[0].value {
        AttributeValue::String { value, .. } => assert_eq!(value, "Save"),
        other => panic!("expected string value, got {other:?}"),
    }
}

#[test]
fn parses_attribute_namespaces() {
    let doc = parse_ok(r#"<container on:click="emit save" style:tone="primary" class="card"/>"#);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!()
    };
    assert_eq!(el.attributes.len(), 3);
    assert_eq!(el.attributes[0].name.namespace, AttributeNamespace::On);
    assert_eq!(el.attributes[0].name.local, "click");
    assert_eq!(el.attributes[1].name.namespace, AttributeNamespace::Style);
    assert_eq!(el.attributes[1].name.local, "tone");
    assert_eq!(
        el.attributes[2].name.namespace,
        AttributeNamespace::Identifier
    );
}

#[test]
fn parses_open_close_with_text_child() {
    let doc = parse_ok(r#"<heading level="3">Hello</heading>"#);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!()
    };
    assert_eq!(el.tag, "heading");
    assert!(!el.self_closing);
    assert_eq!(el.children.len(), 1);
    match &el.children[0] {
        Node::Text { value, .. } => assert_eq!(value, "Hello"),
        other => panic!("expected text child, got {other:?}"),
    }
}

#[test]
fn parses_interpolation_in_attr_and_body() {
    let doc = parse_ok(r#"<heading title="{user.name}">{user.name}</heading>"#);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!()
    };
    match &el.attributes[0].value {
        AttributeValue::Expression(expr) => assert_eq!(expr.body, "user.name"),
        other => panic!("expected expression, got {other:?}"),
    }
    match &el.children[0] {
        Node::Interpolation(expr) => assert_eq!(expr.body, "user.name"),
        other => panic!("expected interpolation, got {other:?}"),
    }
}

#[test]
fn parses_template_attribute_value() {
    let doc = parse_ok(r#"<a href="/users/{id}/edit"/>"#);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!()
    };
    match &el.attributes[0].value {
        AttributeValue::Template { parts, .. } => {
            assert_eq!(parts.len(), 3);
            matches!(parts[0], TemplatePart::Literal { .. });
            matches!(parts[1], TemplatePart::Expression(_));
            matches!(parts[2], TemplatePart::Literal { .. });
        }
        other => panic!("expected template, got {other:?}"),
    }
}

#[test]
fn parses_nested_elements() {
    let doc = parse_ok(r#"<container><heading>Hi</heading></container>"#);
    let Node::Element(outer) = &doc.nodes[0] else {
        panic!()
    };
    assert_eq!(outer.tag, "container");
    assert_eq!(outer.children.len(), 1);
    let Node::Element(inner) = &outer.children[0] else {
        panic!()
    };
    assert_eq!(inner.tag, "heading");
}

#[test]
fn parses_html_comment() {
    let doc = parse_ok(r#"<!-- a note --><spacer/>"#);
    assert_eq!(doc.nodes.len(), 2);
    match &doc.nodes[0] {
        Node::Comment { value, .. } => assert_eq!(value, " a note "),
        other => panic!("expected comment, got {other:?}"),
    }
}

#[test]
fn comment_with_non_ascii_content_round_trips() {
    // Em-dash, curly quotes, accented chars — anything multibyte
    // in a comment body would previously panic the scanner.
    let doc = parse_ok("<!-- résumé — “smart” quotes -->\n<spacer/>");
    match &doc.nodes[0] {
        Node::Comment { value, .. } => {
            assert_eq!(value, " résumé — “smart” quotes ");
        }
        other => panic!("expected comment, got {other:?}"),
    }
}

#[test]
fn text_node_with_non_ascii_content_round_trips() {
    let doc = parse_ok("<text>résumé — naïve façade</text>");
    let Node::Element(el) = &doc.nodes[0] else {
        panic!()
    };
    match &el.children[0] {
        Node::Text { value, .. } => assert_eq!(value, "résumé — naïve façade"),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn parses_boolean_attribute() {
    let doc = parse_ok(r#"<input disabled/>"#);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!()
    };
    assert_eq!(el.attributes[0].name.raw, "disabled");
    assert!(matches!(el.attributes[0].value, AttributeValue::Empty));
}

#[test]
fn parses_control_flow_attribute() {
    let doc = parse_ok(r#"<pill if="{badge}">{badge}</pill>"#);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!()
    };
    assert_eq!(
        el.attributes[0].name.namespace,
        AttributeNamespace::ControlFlow
    );
}

#[test]
fn reports_unclosed_element() {
    let (_doc, errs) = parse(r#"<container>"#);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "unclosed-element"));
}

#[test]
fn reports_mismatched_close() {
    let (_doc, errs) = parse(r#"<container></heading>"#);
    assert!(errs.iter().any(|e| e.code == "mismatched-close"));
}

#[test]
fn reports_unterminated_interpolation() {
    let (_doc, errs) = parse(r#"<a title="{foo"/>"#);
    assert!(errs
        .iter()
        .any(|e| e.code == "unterminated-interpolation" || e.code == "unterminated-string"));
}

#[test]
fn reports_stray_closing_tag() {
    let (_doc, errs) = parse(r#"</orphan>"#);
    assert!(errs.iter().any(|e| e.code == "stray-close-tag"));
}

#[test]
fn parse_to_root_emits_element_node() {
    let root = parse_to_root(r#"<button label="Save"/>"#);
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.children[0].kind, "element");
    assert_eq!(root.children[0].value.as_deref(), Some("button"));
}

#[test]
fn parses_script_block_as_raw_text() {
    // `local`, `{`, `<`, `function` — every Luau-shaped token the
    // PRUI parser would otherwise misinterpret. Wave A of
    // `prui-luau-fusion.md` §7.1 requires the body to survive
    // verbatim as a single text child.
    let src = r##"<script>
local function priority_color(p)
  if p == "high" then return "#ff0000" end
  return "#888888"
end
local state = prism.state { expanded = false }
</script>"##;
    let doc = parse_ok(src);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!("expected element");
    };
    assert_eq!(el.tag, "script");
    // §5.10 — `<script>` carries no `lang=` attribute.
    assert!(el.attributes.is_empty());
    assert_eq!(el.children.len(), 1);
    let Node::Text { value, .. } = &el.children[0] else {
        panic!("expected raw text body");
    };
    assert!(value.contains("local function priority_color"));
    assert!(value.contains("prism.state { expanded = false }"));
}

#[test]
fn parses_style_block_as_raw_text() {
    let src = r##"<style>
[class.card]
background = "#ffffff"
radius = 8
</style>"##;
    let doc = parse_ok(src);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!();
    };
    assert_eq!(el.tag, "style");
    let Node::Text { value, .. } = &el.children[0] else {
        panic!();
    };
    assert!(value.contains("[class.card]"));
}

fn attr<'a>(el: &'a Element, name: &str) -> &'a AttributeValue {
    &el.attributes
        .iter()
        .find(|a| a.name.raw == name)
        .unwrap_or_else(|| panic!("missing attr {name}"))
        .value
}
fn as_str(v: &AttributeValue) -> &str {
    match v {
        AttributeValue::String { value, .. } => value,
        other => panic!("expected String, got {other:?}"),
    }
}
fn el0(doc: &Document) -> &Element {
    match &doc.nodes[0] {
        Node::Element(el) => el,
        n => panic!("expected element, got {n:?}"),
    }
}

#[test]
fn s510_comma_separated_bare_values() {
    let doc = parse_ok(r#"<container direction=row, gap=8, padding=12 16>x</container>"#);
    let el = el0(&doc);
    assert_eq!(as_str(attr(el, "direction")), "row");
    assert_eq!(as_str(attr(el, "gap")), "8");
    assert_eq!(as_str(attr(el, "padding")), "12 16");
}

#[test]
fn s510_quoted_and_whitespace_still_parse() {
    // Back-compat: the existing single-token / quoted,
    // whitespace-separated corpus keeps working unchanged.
    let doc = parse_ok(r#"<container class="card" title="Hi"/>"#);
    let el = el0(&doc);
    assert_eq!(as_str(attr(el, "class")), "card");
    assert_eq!(as_str(attr(el, "title")), "Hi");
}

#[test]
fn s510_action_body_dollar() {
    // `$` wraps to the canonical `luau { … }` action so the
    // dispatcher routes it to ParsedAction::Luau.
    let doc = parse_ok(r#"<button on:click=$state.x = !state.x>Go</button>"#);
    assert_eq!(
        as_str(attr(el0(&doc), "on:click")),
        "luau { state.x = !state.x }"
    );
    // Comma / `>` inside a string or call must not terminate.
    let doc = parse_ok(r#"<button on:click=$emit("a, b")>Go</button>"#);
    assert_eq!(
        as_str(attr(el0(&doc), "on:click")),
        r#"luau { emit("a, b") }"#
    );
}

#[test]
fn s510_list_value() {
    let doc = parse_ok(r#"<container class=[card, {priority_class(p)}]>x</container>"#);
    let AttributeValue::Template { parts, .. } = attr(el0(&doc), "class") else {
        panic!("expected Template");
    };
    assert!(matches!(&parts[0], TemplatePart::Literal { value, .. } if value == "card"));
    assert!(parts
        .iter()
        .any(|p| matches!(p, TemplatePart::Expression(e) if e.body.contains("priority_class"))));

    let doc = parse_ok(r#"<container class=[a, b]>x</container>"#);
    assert_eq!(as_str(attr(el0(&doc), "class")), "a b");
}

#[test]
fn s510_lang_attr_is_an_error() {
    let (_, errs) = parse(r#"<script lang="luau"></script>"#);
    assert!(
        errs.iter().any(|e| e.code == "unexpected-lang-attr"),
        "expected unexpected-lang-attr, got {errs:?}"
    );
}

#[test]
fn s510_import_postfix_as() {
    let doc = parse_ok(r#"<import script="./fmt.luau"/> as fmt"#);
    let el = el0(&doc);
    assert_eq!(el.tag, "import");
    assert_eq!(as_str(attr(el, "script")), "./fmt.luau");
    assert_eq!(as_str(attr(el, "as")), "fmt");

    // No postfix → no synthetic `as`, scanner not over-consumed.
    let doc = parse_ok(r#"<import script="./x.luau"/><spacer/>"#);
    assert!(doc
        .nodes
        .iter()
        .any(|n| matches!(n, Node::Element(e) if e.tag == "spacer")));
}

#[test]
fn script_block_then_sibling_element() {
    // After the raw-text body closes, the parser must resume
    // normal mode and read the sibling element. Regression guard
    // against a sticky "still in raw text" state.
    let src = r#"<script>local x = 1</script>
<container/>"#;
    let doc = parse_ok(src);
    assert!(doc
        .nodes
        .iter()
        .any(|n| matches!(n, Node::Element(el) if el.tag == "script")));
    assert!(doc
        .nodes
        .iter()
        .any(|n| matches!(n, Node::Element(el) if el.tag == "container")));
}

#[test]
fn parses_language_block_as_raw_text() {
    let src = "<language name=\"sql\">select * from t where x < 3 and y = '{a}'</language>";
    let doc = parse_ok(src);
    let Node::Element(el) = &doc.nodes[0] else {
        panic!()
    };
    assert_eq!(el.tag, "language");
    assert_eq!(el.attributes[0].name.local, "name");
    let Node::Text { value, .. } = &el.children[0] else {
        panic!()
    };
    assert!(value.contains("select * from t where x < 3"));
    assert!(value.contains("'{a}'"));
}

#[test]
fn parses_dialect_sigil_sugar() {
    let doc = parse_ok("<container>~md{**bold** and _it_}</container>");
    let Node::Element(c) = &doc.nodes[0] else {
        panic!()
    };
    let Node::Element(lang) = &c.children[0] else {
        panic!("expected <language>, got {:?}", c.children[0]);
    };
    assert_eq!(lang.tag, "language");
    match &lang.attributes[0].value {
        AttributeValue::String { value, .. } => assert_eq!(value, "md"),
        o => panic!("{o:?}"),
    }
    let Node::Text { value, .. } = &lang.children[0] else {
        panic!()
    };
    assert_eq!(value, "**bold** and _it_");
}

#[test]
fn sigil_balances_braces_and_escapes() {
    let doc = parse_ok(r"<container>~tex{a {b} c \{lit\}}</container>");
    let Node::Element(c) = &doc.nodes[0] else {
        panic!()
    };
    let Node::Element(lang) = &c.children[0] else {
        panic!()
    };
    let Node::Text { value, .. } = &lang.children[0] else {
        panic!()
    };
    assert_eq!(value, "a {b} c {lit}");
}

#[test]
fn bare_tilde_in_prose_is_not_a_sigil() {
    let doc = parse_ok("<text>about ~5 items</text>");
    let Node::Element(t) = &doc.nodes[0] else {
        panic!()
    };
    let Node::Text { value, .. } = &t.children[0] else {
        panic!()
    };
    assert_eq!(value, "about ~5 items");
}

#[test]
fn parses_full_strawman_card() {
    let src = r#"
<component name="Card">
  <container layout="flow" gap="{tokens.spacing.md}" on:click="emit clicked">
    <heading level="3">{title}</heading>
    <pill tone="accent" if="{badge}">{badge}</pill>
  </container>
</component>
"#;
    let (doc, errs) = parse(src);
    assert!(errs.is_empty(), "errors: {errs:?}");
    // Top-level: one whitespace text node + the <component>.
    let component = doc
        .nodes
        .iter()
        .find_map(|n| match n {
            Node::Element(el) if el.tag == "component" => Some(el),
            _ => None,
        })
        .expect("component element");
    assert_eq!(component.attributes[0].name.raw, "name");
}
