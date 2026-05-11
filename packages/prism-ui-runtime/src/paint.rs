//! Render-command → femtovg paint-list translation.
//!
//! Generic over `femtovg::Renderer` so the same code drives the
//! native (`OpenGl` over glutin) and web (`OpenGl` over WebGL2)
//! backends. This is the load-bearing chunk both Phase-1 backend
//! lifecycles wrap.

use femtovg::{Canvas, Color as FemtoColor, Paint, Path, Renderer};

use crate::command::{Color, RenderCommand};
use crate::images::ImageCache;
use crate::layout::Viewport;
use crate::text::TextSystem;

/// Walk `commands` and issue the corresponding femtovg draw calls
/// against `canvas`. The caller is responsible for `set_size` /
/// `clear_rect` / `flush_to_output` around the call — `draw` only
/// touches paths, paints, scissors, and images.
pub fn draw<R: Renderer>(
    canvas: &mut Canvas<R>,
    _viewport: Viewport,
    commands: &[RenderCommand],
    text: &mut TextSystem,
    images: &mut ImageCache,
) {
    for cmd in commands {
        match cmd {
            RenderCommand::Rectangle {
                bounds,
                color,
                radius,
            } => {
                let mut path = Path::new();
                if radius.tl == 0.0 && radius.tr == 0.0 && radius.br == 0.0 && radius.bl == 0.0 {
                    path.rect(bounds.x, bounds.y, bounds.width, bounds.height);
                } else {
                    path.rounded_rect_varying(
                        bounds.x,
                        bounds.y,
                        bounds.width,
                        bounds.height,
                        radius.tl,
                        radius.tr,
                        radius.br,
                        radius.bl,
                    );
                }
                canvas.fill_path(&path, &Paint::color(femto(*color)));
            }
            RenderCommand::Border {
                bounds,
                color,
                width,
                radius,
            } => {
                let mut path = Path::new();
                if radius.tl == 0.0 && radius.tr == 0.0 && radius.br == 0.0 && radius.bl == 0.0 {
                    path.rect(bounds.x, bounds.y, bounds.width, bounds.height);
                } else {
                    path.rounded_rect_varying(
                        bounds.x,
                        bounds.y,
                        bounds.width,
                        bounds.height,
                        radius.tl,
                        radius.tr,
                        radius.br,
                        radius.bl,
                    );
                }
                let mut paint = Paint::color(femto(*color));
                paint.set_line_width(*width);
                canvas.stroke_path(&path, &paint);
            }
            RenderCommand::Text {
                bounds,
                content,
                color,
                font_size,
                caret,
            } => {
                draw_text(
                    canvas,
                    text,
                    content,
                    bounds.x,
                    bounds.y,
                    *font_size,
                    *color,
                    bounds.width,
                    *caret,
                    bounds.height,
                );
            }
            RenderCommand::Image {
                bounds,
                source,
                radius,
                tint,
            } => {
                let Some(image_id) = images.ensure(canvas, source) else {
                    continue;
                };
                let mut path = Path::new();
                if radius.tl == 0.0 && radius.tr == 0.0 && radius.br == 0.0 && radius.bl == 0.0 {
                    path.rect(bounds.x, bounds.y, bounds.width, bounds.height);
                } else {
                    path.rounded_rect_varying(
                        bounds.x,
                        bounds.y,
                        bounds.width,
                        bounds.height,
                        radius.tl,
                        radius.tr,
                        radius.br,
                        radius.bl,
                    );
                }
                let mut paint = Paint::image(
                    image_id,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    0.0,
                    1.0,
                );
                // Tint contract: when `Some`, the renderer treats the
                // source as a mask and multiplies in the tint colour.
                // femtovg's image paint already multiplies the colour
                // attached to the paint against the sampled texel,
                // which matches the `tint = mask × colour` contract
                // for icon glyphs. `None` paints the image verbatim
                // (white = identity multiplier).
                if let Some(c) = tint {
                    paint.set_color(femto(*c));
                }
                canvas.fill_path(&path, &paint);
            }
            RenderCommand::ScissorStart { bounds } => {
                canvas.save();
                canvas.scissor(bounds.x, bounds.y, bounds.width, bounds.height);
            }
            RenderCommand::ScissorEnd => {
                canvas.restore();
            }
            RenderCommand::Hint { .. } => {
                // Pass-through metadata; no native render effect.
            }
        }
    }
}

fn femto(c: Color) -> FemtoColor {
    FemtoColor::rgba(c.r, c.g, c.b, c.a)
}

/// Shape `content` with cosmic-text and blit each glyph as a femtovg
/// image. `top` is the top of the layout box; cosmic-text reports
/// glyph y_offsets relative to the baseline within each run, so we
/// add `run.line_y` (also relative to the buffer top) to translate
/// into bounds-local coordinates.
///
/// `width` is the Taffy-computed box width. We pass it through to
/// cosmic-text's `set_size(width, …)` only when it can plausibly fit
/// the content's natural width — otherwise the buffer would wrap the
/// label mid-word inside a too-narrow flex parent ("Window" →
/// "Windo / w"). When the box is narrower than the natural width we
/// fall back to `f32::INFINITY`, telling cosmic-text "lay this out on
/// one line"; the visual overflow is much friendlier than the
/// glyph-broken wrap and pairs with the `flex_shrink: 0` defence on
/// text leaves so the parent has reserved the space anyway.
///
/// `caret` paints a 1.5-px-wide vertical bar in `caret`'s colour
/// immediately after the rendered glyphs. The position is taken
/// from the cosmic-text buffer's max-glyph-x (i.e. the actual
/// shaped end), so the caret sits exactly at the next-character
/// insertion point — unlike a chars-times-font-size estimate, which
/// drifts past the real glyph end and gives the user the impression
/// that Backspace deletes the wrong character.
#[allow(clippy::too_many_arguments)]
fn draw_text<R: Renderer>(
    canvas: &mut Canvas<R>,
    text: &mut TextSystem,
    content: &str,
    left: f32,
    top: f32,
    font_size: f32,
    colour: Color,
    width: f32,
    caret: Option<Color>,
    box_height: f32,
) {
    // Heuristic natural width — same `chars * font_size * 0.55`
    // mapping `measure_text` uses, kept in sync intentionally so the
    // layout and paint passes agree on what counts as "enough room".
    let natural_w = content.chars().count() as f32 * font_size * 0.55;
    let shape_width = if width + 0.5 >= natural_w {
        width
    } else {
        f32::INFINITY
    };
    let buffer = text.shape(content, font_size, shape_width);
    // Collect placements first so we drop the buffer borrow before we
    // re-enter `text` for glyph baking. We also track the rightmost
    // glyph end on the first line — that's the pixel offset the caret
    // bar should sit at (immediately after the last shaped glyph).
    struct Placement {
        x: f32,
        y: f32,
        cache: cosmic_text::CacheKey,
    }
    let mut placements: Vec<Placement> = Vec::new();
    let mut shaped_end_x: f32 = 0.0;
    let mut first_line_top: f32 = 0.0;
    for (line_idx, run) in buffer.layout_runs().enumerate() {
        if line_idx == 0 {
            first_line_top = run.line_top;
        }
        for glyph in run.glyphs.iter() {
            let physical = glyph.physical((0.0, 0.0), 1.0);
            placements.push(Placement {
                x: physical.x as f32,
                y: run.line_y + physical.y as f32,
                cache: physical.cache_key,
            });
            if line_idx == 0 {
                // glyph.x is the horizontal advance origin within the
                // run; adding glyph.w gives the post-glyph cursor
                // position the caret should land at.
                let end = glyph.x + glyph.w;
                if end > shaped_end_x {
                    shaped_end_x = end;
                }
            }
        }
    }
    drop(buffer);
    for p in placements {
        let Some(g) = text.glyph_image(canvas, p.cache, colour) else {
            continue;
        };
        let dst_x = left + p.x + g.left as f32;
        let dst_y = top + p.y - g.top as f32;
        let mut path = Path::new();
        path.rect(dst_x, dst_y, g.width as f32, g.height as f32);
        let paint = Paint::image(
            g.image,
            dst_x,
            dst_y,
            g.width as f32,
            g.height as f32,
            0.0,
            1.0,
        );
        canvas.fill_path(&path, &paint);
    }
    if let Some(caret_color) = caret {
        // Paint a 1.5-px-wide caret bar at the shaped-text end. The
        // x lands one pixel after the last glyph's advance so the bar
        // sits in the "next insertion point" slot, exactly matching
        // Backspace's "delete the previous character" semantic. The
        // bar height tracks the box with 2-px padding so it doesn't
        // crowd the input's border.
        let caret_x = left + shaped_end_x + 1.0;
        let pad_y = 2.0;
        let caret_top = top + first_line_top + pad_y;
        let caret_height = (box_height - pad_y * 2.0).max(font_size * 0.9);
        let mut path = Path::new();
        path.rect(caret_x, caret_top, 1.5, caret_height);
        let paint = Paint::color(femto(caret_color));
        canvas.fill_path(&path, &paint);
    }
}
