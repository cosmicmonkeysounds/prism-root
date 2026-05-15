//! HTML backend — pure render-command → HTML/CSS string lowering.
//! Side-effect-free; consumed by `prism-relay` for SSR. This is the
//! function that obsoletes `Component::render_html` and the parallel
//! `HtmlRegistry` walker.

use crate::command::{Color, RenderCommand};

/// Lower a render-command stream to a single HTML/CSS document
/// fragment. Phase 1 stub — emits a `<div>` skeleton with absolutely
/// positioned children. Phase 2 will switch to semantic-element
/// emission driven by `Hint` commands carrying class/ARIA metadata.
pub fn lower(commands: &[RenderCommand]) -> String {
    let mut out = String::new();
    out.push_str("<div class=\"prism-ui-root\" style=\"position:relative\">");
    for cmd in commands {
        match cmd {
            RenderCommand::Rectangle {
                bounds,
                color,
                radius,
            } => {
                out.push_str(&format!(
                    "<div style=\"position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;background:{};border-radius:{}px {}px {}px {}px\"></div>",
                    bounds.x, bounds.y, bounds.width, bounds.height,
                    css_color(*color),
                    radius.tl, radius.tr, radius.br, radius.bl,
                ));
            }
            RenderCommand::Text {
                bounds,
                content,
                color,
                font_size,
                ..
            } => {
                out.push_str(&format!(
                    "<span style=\"position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;color:{};font-size:{}px\">{}</span>",
                    bounds.x, bounds.y, bounds.width, bounds.height,
                    css_color(*color), font_size,
                    html_escape(content),
                ));
            }
            RenderCommand::Image {
                bounds,
                source,
                radius,
                tint,
            } => {
                if let Some(tint) = tint {
                    // Tinted icon — paint a masked block so the
                    // monochrome SVG / PNG silhouette adopts the
                    // palette colour. This is the canonical
                    // CSS icon-tint hack (`mask-image` + a solid
                    // `background-color`) and round-trips perfectly
                    // for monochrome assets.
                    out.push_str(&format!(
                        "<span style=\"position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;background-color:{};-webkit-mask-image:url('{src}');mask-image:url('{src}');-webkit-mask-size:cover;mask-size:cover;-webkit-mask-repeat:no-repeat;mask-repeat:no-repeat;border-radius:{}px {}px {}px {}px\"></span>",
                        bounds.x, bounds.y, bounds.width, bounds.height,
                        css_color(*tint),
                        radius.tl, radius.tr, radius.br, radius.bl,
                        src = html_escape_attr(source),
                    ));
                } else {
                    out.push_str(&format!(
                        "<img src=\"{}\" style=\"position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;object-fit:cover;border-radius:{}px {}px {}px {}px\"/>",
                        html_escape_attr(source),
                        bounds.x, bounds.y, bounds.width, bounds.height,
                        radius.tl, radius.tr, radius.br, radius.bl,
                    ));
                }
            }
            RenderCommand::Border { .. }
            | RenderCommand::ScissorStart { .. }
            | RenderCommand::ScissorEnd
            | RenderCommand::Hint { .. } => {
                // TODO Phase 1: border, scissor, hint pass-through.
            }
        }
    }
    out.push_str("</div>");
    out
}

fn css_color(c: Color) -> String {
    format!("rgba({},{},{},{})", c.r, c.g, c.b, f32::from(c.a) / 255.0)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Attribute-context escape — escapes the `"` quote that closes the
/// attribute value, plus the structural ampersand / less-than. Same
/// rules as `html_escape` but skips `>` since it's not significant
/// inside an attribute value.
fn html_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CornerRadius, Rect};

    #[test]
    fn empty_stream_renders_root_div() {
        let html = lower(&[]);
        assert!(html.starts_with("<div"));
        assert!(html.ends_with("</div>"));
    }

    #[test]
    fn rectangle_lowers_to_positioned_div() {
        let cmd = RenderCommand::Rectangle {
            bounds: Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 50.0,
            },
            color: Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            radius: CornerRadius {
                tl: 4.0,
                tr: 4.0,
                br: 4.0,
                bl: 4.0,
            },
        };
        let html = lower(&[cmd]);
        assert!(html.contains("left:10px"));
        assert!(html.contains("rgba(255,0,0,1)"));
    }

    #[test]
    fn untinted_image_lowers_to_img_tag() {
        let cmd = RenderCommand::Image {
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 16.0,
                height: 16.0,
            },
            source: "icons/x.svg".into(),
            radius: CornerRadius::default(),
            tint: None,
        };
        let html = lower(&[cmd]);
        assert!(html.contains("<img src=\"icons/x.svg\""));
        assert!(!html.contains("mask-image"));
    }

    #[test]
    fn tinted_image_lowers_to_masked_span_with_background_color() {
        let cmd = RenderCommand::Image {
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 16.0,
                height: 16.0,
            },
            source: "icons/x.svg".into(),
            radius: CornerRadius::default(),
            tint: Some(Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            }),
        };
        let html = lower(&[cmd]);
        assert!(html.contains("mask-image:url('icons/x.svg')"));
        assert!(html.contains("background-color:rgba(255,0,0,1)"));
        assert!(!html.contains("<img"));
    }

    #[test]
    fn text_is_html_escaped() {
        let cmd = RenderCommand::Text {
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            content: "<script>".into(),
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            font_size: 14.0,
            caret: None,
            caret_byte: None,
            selection: None,
            selection_color: None,
            spans: Vec::new(),
            underline: None,
            underline_color: None,
        };
        let html = lower(&[cmd]);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
