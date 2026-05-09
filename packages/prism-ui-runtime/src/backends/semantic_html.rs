//! Semantic HTML lowering — walks the typed `Node` tree directly,
//! emits meaningful tags (`<section>`, `<h1>`, `<p>`, `<img>`, …)
//! using the [`Semantic`] hint each node carries. Unlike
//! [`super::html`] (which converts the post-layout
//! `Vec<RenderCommand>` into absolutely-positioned `<div>`s for
//! pixel-faithful preview), this walker produces SEO/accessibility-
//! friendly markup suitable for SSR.
//!
//! ## Single source of truth
//!
//! Each block's `Component::lower_ui` impl declares the semantic
//! flavour of every node it produces — exactly once, alongside the
//! layout vocabulary. The walker dispatches on:
//!
//! 1. `Semantic::tag` if set — the block's explicit tag override,
//! 2. otherwise a per-variant default (`<div>` / `<span>` / `<img>`).
//!
//! Class, ARIA, role, and free-form attrs are emitted from the same
//! hint with no per-block branches in the walker.
//!
//! Layout properties (gap, padding, sizing, background, radius) lower
//! to a single inline `style="…"` attribute. Backends that want
//! pixel-precise positioning use the `html` backend instead; this one
//! is shaped for documents, not canvases.

use std::fmt::Write;

use crate::command::Color;
use crate::layout::{ContainerProps, Node, Padding, Semantic, Sizing, TextProps};

/// Lower a `Node` tree to a semantic HTML fragment. Pure function;
/// no layout pass, no commands, no IO.
pub fn lower(root: &Node) -> String {
    let mut out = String::new();
    write_node(root, &mut out);
    out
}

/// HTML void elements — emitted self-closing, children dropped. The
/// list is the spec's full set; the walker matches only against the
/// elements blocks in this workspace actually declare (`hr`, `input`,
/// `br`, `img` — though `img` flows through the `Node::Image` arm, not
/// here). Centralised so a new void-tag block doesn't need to teach
/// the walker about itself.
const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

fn is_void_tag(tag: &str) -> bool {
    VOID_TAGS.contains(&tag)
}

/// Tag-with-children writer. Ranges over every node variant and
/// collapses class/aria/role/attrs through the same helper, so adding
/// a new attribute namespace is one place to update.
fn write_node(node: &Node, out: &mut String) {
    match node {
        Node::Container {
            props, children, ..
        } => {
            // Same policy as text: when the block opts into a semantic
            // tag (`<section>`, `<form>`, `<nav>`, `<ul>`…), the SSR
            // output stays clean — flex/padding/sizing live in
            // stylesheets keyed by tag/class. The pixel-faithful
            // `backends::html` lowering is the place for inline-style
            // layout reproduction.
            let (tag, inline) = if let Some(tag) = props.semantic.tag.as_deref() {
                (tag, String::new())
            } else {
                ("div", container_inline_style(props))
            };
            if is_void_tag(tag) {
                // Void elements never have an end tag and must not
                // emit children. Blocks that map to `<hr>`/`<input>`
                // declare their shape via attrs on `Semantic`; any
                // children present in the layout tree are layout-only
                // and get dropped from semantic output.
                let _ = write!(out, "<{tag}");
                write_class_aria_role_attrs(out, &props.semantic);
                if !inline.is_empty() {
                    let _ = write!(out, " style=\"{inline}\"");
                }
                out.push_str("/>");
                return;
            }
            write_open_tag(out, tag, &props.semantic, inline);
            for child in children {
                write_node(child, out);
            }
            let _ = write!(out, "</{tag}>");
        }
        Node::Text { content, props, .. } => {
            // When the block declared an explicit semantic tag, defer
            // styling to the browser's heading defaults / external
            // stylesheets — emitting `font-size` inline on every `<h1>`
            // is noise and breaks the "semantic HTML" contract that
            // SEO crawlers and assistive tech rely on. Color follows
            // the same rule. Only when the tag falls back to the
            // bucket-by-size default do we keep inline styles, since
            // there's no semantic anchor to defer to.
            let (tag, inline) = if let Some(tag) = props.semantic.tag.as_deref() {
                (tag, String::new())
            } else {
                (default_text_tag(props.font_size), text_inline_style(props))
            };
            write_open_tag(out, tag, &props.semantic, inline);
            push_escaped_text(out, content);
            let _ = write!(out, "</{tag}>");
        }
        Node::Image {
            source,
            width,
            height,
            radius,
            semantic,
            ..
        } => {
            // `<img>` is void — single self-closing tag, src always
            // emitted, optional inline sizing/radius.
            out.push_str("<img src=\"");
            push_escaped_attr(out, source);
            out.push('"');
            write_class_aria_role_attrs(out, semantic);
            let style = image_inline_style(*width, *height, *radius);
            if !style.is_empty() {
                let _ = write!(out, " style=\"{style}\"");
            }
            out.push_str("/>");
        }
        Node::TextInput {
            value,
            placeholder,
            semantic,
            ..
        } => {
            // `<input>` is void — single self-closing tag, value /
            // placeholder always emitted, free-form attrs from the
            // semantic hint flow through after. The block can override
            // `type` (e.g. `email`, `search`) via `Semantic::with_attr`;
            // we default to `text` so the HTML is always valid.
            let has_type_override = semantic.attrs.iter().any(|(k, _)| k == "type");
            out.push_str("<input");
            if !has_type_override {
                out.push_str(" type=\"text\"");
            }
            if !value.is_empty() {
                out.push_str(" value=\"");
                push_escaped_attr(out, value);
                out.push('"');
            }
            if !placeholder.is_empty() {
                out.push_str(" placeholder=\"");
                push_escaped_attr(out, placeholder);
                out.push('"');
            }
            write_class_aria_role_attrs(out, semantic);
            out.push_str("/>");
        }
        Node::Spacer { width, height, .. } => {
            // No content; render as a sized div so the SSR output keeps
            // the spacing the editor authored. No semantic hint on
            // Spacer — layout-only.
            let _ = write!(
                out,
                "<div style=\"width:{}px;height:{}px\"></div>",
                width, height
            );
        }
    }
}

/// Open tag for a non-void element — emits `<tag class=… aria-label=…
/// role=… attr-k=attr-v… style=…>`. The structural attribute order is
/// stable so snapshot tests don't churn on hint order changes.
fn write_open_tag(out: &mut String, tag: &str, hint: &Semantic, inline_style: String) {
    let _ = write!(out, "<{tag}");
    write_class_aria_role_attrs(out, hint);
    if !inline_style.is_empty() {
        let _ = write!(out, " style=\"{inline_style}\"");
    }
    out.push('>');
}

fn write_class_aria_role_attrs(out: &mut String, hint: &Semantic) {
    if let Some(class) = &hint.class {
        out.push_str(" class=\"");
        push_escaped_attr(out, class);
        out.push('"');
    }
    if let Some(label) = &hint.aria_label {
        out.push_str(" aria-label=\"");
        push_escaped_attr(out, label);
        out.push('"');
    }
    if let Some(role) = &hint.role {
        out.push_str(" role=\"");
        push_escaped_attr(out, role);
        out.push('"');
    }
    for (k, v) in &hint.attrs {
        out.push(' ');
        push_escaped_attr(out, k);
        out.push_str("=\"");
        push_escaped_attr(out, v);
        out.push('"');
    }
}

/// Default tag selection for text nodes when the block doesn't
/// override. Buckets by font size into heading levels — matches
/// `render_slint`'s level→size mapping in reverse, so legacy blocks
/// that just call `text_node` with a default size land on a sensible
/// tag without each block opting in.
fn default_text_tag(font_size: f32) -> &'static str {
    match font_size {
        s if s >= 32.0 => "h1",
        s if s >= 24.0 => "h2",
        s if s >= 20.0 => "h3",
        s if s >= 17.0 => "h4",
        s if s >= 15.0 => "h5",
        // Paragraph-scale → `<p>` is correct for prose; a span is
        // fine for inline glyphs but `<p>` round-trips to flow layout
        // in browsers and is what SEO crawlers expect.
        _ => "p",
    }
}

fn container_inline_style(props: &ContainerProps) -> String {
    let mut s = String::new();
    s.push_str("display:flex;");
    let dir = match props.direction {
        crate::layout::Direction::Row => "row",
        crate::layout::Direction::Column => "column",
    };
    let _ = write!(s, "flex-direction:{dir};");
    if props.gap != 0.0 {
        let _ = write!(s, "gap:{}px;", props.gap);
    }
    push_padding(&mut s, props.padding);
    push_sizing(&mut s, "width", props.width);
    push_sizing(&mut s, "height", props.height);
    if let Some(bg) = props.background {
        let _ = write!(s, "background:{};", css_color(bg));
    }
    let r = props.radius;
    if r.tl != 0.0 || r.tr != 0.0 || r.br != 0.0 || r.bl != 0.0 {
        let _ = write!(
            s,
            "border-radius:{}px {}px {}px {}px;",
            r.tl, r.tr, r.br, r.bl
        );
    }
    s
}

fn text_inline_style(props: &TextProps) -> String {
    let mut s = String::new();
    let _ = write!(s, "font-size:{}px;", props.font_size);
    let _ = write!(s, "color:{};", css_color(props.color));
    s
}

fn image_inline_style(
    width: Sizing,
    height: Sizing,
    radius: crate::command::CornerRadius,
) -> String {
    let mut s = String::new();
    push_sizing(&mut s, "width", width);
    push_sizing(&mut s, "height", height);
    if radius.tl != 0.0 || radius.tr != 0.0 || radius.br != 0.0 || radius.bl != 0.0 {
        let _ = write!(
            s,
            "border-radius:{}px {}px {}px {}px;",
            radius.tl, radius.tr, radius.br, radius.bl
        );
    }
    if !s.is_empty() {
        s.push_str("object-fit:cover;");
    }
    s
}

fn push_padding(s: &mut String, p: Padding) {
    if p.left == 0.0 && p.right == 0.0 && p.top == 0.0 && p.bottom == 0.0 {
        return;
    }
    let _ = write!(
        s,
        "padding:{}px {}px {}px {}px;",
        p.top, p.right, p.bottom, p.left
    );
}

fn push_sizing(s: &mut String, prop: &str, sz: Sizing) {
    match sz {
        Sizing::Fit => {}
        Sizing::Grow => {
            let _ = write!(s, "{prop}:100%;");
        }
        Sizing::Fixed(v) => {
            let _ = write!(s, "{prop}:{v}px;");
        }
    }
}

fn css_color(c: Color) -> String {
    format!("rgba({},{},{},{})", c.r, c.g, c.b, f32::from(c.a) / 255.0)
}

fn push_escaped_text(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

fn push_escaped_attr(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CornerRadius;
    use crate::layout::{ContainerProps, Direction, Node, Sizing, TextProps};

    fn text(content: &str, semantic: Semantic) -> Node {
        Node::Text {
            id: String::new(),
            content: content.into(),
            props: TextProps {
                semantic,
                ..Default::default()
            },
        }
    }

    #[test]
    fn default_tags_buckets_text_by_font_size() {
        let big = Node::Text {
            id: String::new(),
            content: "Title".into(),
            props: TextProps {
                font_size: 32.0,
                ..Default::default()
            },
        };
        assert!(lower(&big).starts_with("<h1"));
    }

    #[test]
    fn semantic_tag_overrides_default() {
        let html = lower(&text("Hello", Semantic::tag("h2").with_class("title")));
        assert!(html.starts_with("<h2"));
        assert!(html.contains("class=\"title\""));
        assert!(html.contains(">Hello</h2>"));
    }

    #[test]
    fn container_emits_flex_inline_styles() {
        let node = Node::Container {
            id: String::new(),
            children: vec![],
            props: ContainerProps {
                direction: Direction::Row,
                gap: 8.0,
                ..Default::default()
            },
        };
        let html = lower(&node);
        assert!(html.contains("display:flex"));
        assert!(html.contains("flex-direction:row"));
        assert!(html.contains("gap:8px"));
    }

    #[test]
    fn image_emits_void_tag_with_alt_through_attrs() {
        let node = Node::Image {
            id: String::new(),
            source: "/asset/abc".into(),
            width: Sizing::Grow,
            height: Sizing::Grow,
            radius: CornerRadius::default(),
            tint: None,
            semantic: Semantic::default().with_attr("alt", "A cat"),
        };
        let html = lower(&node);
        assert!(html.starts_with("<img src=\"/asset/abc\""));
        assert!(html.contains("alt=\"A cat\""));
        assert!(html.ends_with("/>"));
    }

    #[test]
    fn text_content_is_html_escaped() {
        let html = lower(&text("<script>", Semantic::default()));
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn aria_and_role_round_trip() {
        let html = lower(&Node::Container {
            id: String::new(),
            children: vec![],
            props: ContainerProps {
                semantic: Semantic::tag("nav")
                    .with_aria_label("Primary")
                    .with_role("navigation"),
                ..Default::default()
            },
        });
        assert!(html.starts_with("<nav"));
        assert!(html.contains("aria-label=\"Primary\""));
        assert!(html.contains("role=\"navigation\""));
    }

    #[test]
    fn void_tag_container_emits_self_closing_and_drops_children() {
        let html = lower(&Node::Container {
            id: String::new(),
            children: vec![text("ignored", Semantic::default())],
            props: ContainerProps {
                semantic: Semantic::tag("hr"),
                ..Default::default()
            },
        });
        assert!(html.starts_with("<hr"));
        assert!(html.ends_with("/>"));
        assert!(!html.contains("ignored"));
        assert!(!html.contains("</hr>"));
    }

    #[test]
    fn input_void_tag_carries_attrs() {
        let html = lower(&Node::Container {
            id: String::new(),
            children: vec![],
            props: ContainerProps {
                semantic: Semantic::tag("input")
                    .with_attr("type", "text")
                    .with_attr("placeholder", "name"),
                ..Default::default()
            },
        });
        assert!(html.starts_with("<input"));
        assert!(html.contains("type=\"text\""));
        assert!(html.contains("placeholder=\"name\""));
        assert!(html.ends_with("/>"));
    }

    #[test]
    fn nested_tree_round_trips() {
        let inner = text("Body copy", Semantic::tag("p"));
        let outer = Node::Container {
            id: String::new(),
            children: vec![inner],
            props: ContainerProps {
                semantic: Semantic::tag("article"),
                ..Default::default()
            },
        };
        let html = lower(&outer);
        assert!(html.starts_with("<article"));
        assert!(html.contains("<p"));
        assert!(html.contains(">Body copy</p>"));
        assert!(html.ends_with("</article>"));
    }
}
