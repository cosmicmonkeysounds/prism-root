//! Render-command → femtovg paint-list translation.
//!
//! Generic over `femtovg::Renderer` so the same code drives the
//! native (`OpenGl` over glutin) and web (`OpenGl` over WebGL2)
//! backends. This is the load-bearing chunk both Phase-1 backend
//! lifecycles wrap.

use femtovg::{Canvas, Color as FemtoColor, Paint, Path, Renderer};

use crate::command::{Color, RenderCommand, TextSelection, TextSpan};
use crate::images::ImageCache;
use crate::layout::Viewport;
use crate::text::TextSystem;

/// Walk `commands` and issue the corresponding femtovg draw calls
/// against `canvas`. The caller is responsible for `set_size` /
/// `clear_rect` / `flush_to_output` around the call — `draw` only
/// touches paths, paints, scissors, and images.
///
/// Animated image sources hard-pin to their first frame; the
/// animation-aware [`draw_at`] variant takes a monotonic `now_ms`
/// and rotates GIF / animated-WebP / APNG frames against it.
pub fn draw<R: Renderer>(
    canvas: &mut Canvas<R>,
    viewport: Viewport,
    commands: &[RenderCommand],
    text: &mut TextSystem,
    images: &mut ImageCache,
) {
    draw_at(canvas, viewport, commands, text, images, 0);
}

/// Wave 14.7 — animation-aware variant of [`draw`]. `now_ms` is a
/// monotonic millisecond clock from the host (typically
/// `Instant::now().duration_since(epoch).as_millis()`). Each
/// `RenderCommand::Image` whose source resolves to an animated
/// entry in `images` paints the frame appropriate to `now_ms`;
/// static sources ignore the clock and paint their sole `ImageId`
/// unchanged. Hosts merge `ImageCache::has_animations()` into their
/// per-frame "request redraw" bit so loops keep ticking without an
/// explicit timer.
pub fn draw_at<R: Renderer>(
    canvas: &mut Canvas<R>,
    _viewport: Viewport,
    commands: &[RenderCommand],
    text: &mut TextSystem,
    images: &mut ImageCache,
    now_ms: u64,
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
                caret_byte,
                selection,
                selection_color,
                spans,
                underline,
                underline_color,
                glyph_outlines,
                glyph_outline_color,
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
                    *caret_byte,
                    *selection,
                    *selection_color,
                    spans,
                    *underline,
                    *underline_color,
                    glyph_outlines,
                    *glyph_outline_color,
                    bounds.height,
                );
            }
            RenderCommand::Image {
                bounds,
                source,
                radius,
                tint,
            } => {
                let Some(image_id) = images.ensure_frame(canvas, source, now_ms) else {
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
struct GlyphSpan {
    line_idx: usize,
    start: usize,
    end: usize,
    x_min: f32,
    x_max: f32,
}

/// Look up the per-byte colour override for a glyph at byte
/// `offset`. Returns the *first* span whose `[start_byte, end_byte)`
/// range covers the offset. Spans are expected non-overlapping
/// (see [`TextSpan`]'s contract); a linear scan is fine — a code
/// editor's span list is line-local and short.
fn colour_for_byte(offset: usize, spans: &[TextSpan]) -> Option<Color> {
    spans
        .iter()
        .find(|s| offset >= s.start_byte && offset < s.end_byte)
        .map(|s| s.color)
}

struct LineMetrics {
    line_top: f32,
    line_height: f32,
    x_end: f32,
}

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
    caret_byte: Option<usize>,
    selection: Option<TextSelection>,
    selection_color: Option<Color>,
    spans: &[TextSpan],
    underline: Option<TextSelection>,
    underline_color: Option<Color>,
    glyph_outlines: &[usize],
    glyph_outline_color: Option<Color>,
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
    // Precompute per-source-line byte offsets — cosmic-text reports
    // `glyph.start` / `glyph.end` relative to the *buffer line*, not
    // the global text. Caret + selection + span lookups all compare
    // against global byte offsets, so we stamp the line's origin onto
    // every glyph below.
    let mut line_offsets: Vec<usize> = Vec::with_capacity(8);
    line_offsets.push(0);
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            line_offsets.push(i + 1);
        }
    }
    let buffer = text.shape(content, font_size, shape_width);
    struct Placement {
        x: f32,
        y: f32,
        cache: cosmic_text::CacheKey,
        /// Resolved colour for this glyph — looked up against
        /// `spans` by the glyph's global byte offset. `colour` is
        /// the fallback when no span covers the offset.
        colour: Color,
    }
    let mut placements: Vec<Placement> = Vec::new();
    let mut glyph_spans: Vec<GlyphSpan> = Vec::new();
    let mut line_metrics: Vec<LineMetrics> = Vec::new();
    let mut shaped_end_x: f32 = 0.0;
    let mut first_line_top: f32 = 0.0;
    for (line_idx, run) in buffer.layout_runs().enumerate() {
        if line_idx == 0 {
            first_line_top = run.line_top;
        }
        let line_base = line_offsets
            .get(line_idx)
            .copied()
            .unwrap_or_else(|| *line_offsets.last().unwrap_or(&0));
        let mut row_x_max = 0.0f32;
        for glyph in run.glyphs.iter() {
            let physical = glyph.physical((0.0, 0.0), 1.0);
            let global_start = line_base + glyph.start;
            let global_end = line_base + glyph.end;
            let glyph_colour = colour_for_byte(global_start, spans).unwrap_or(colour);
            placements.push(Placement {
                x: physical.x as f32,
                y: run.line_y + physical.y as f32,
                cache: physical.cache_key,
                colour: glyph_colour,
            });
            let gx_min = glyph.x;
            let gx_max = glyph.x + glyph.w;
            glyph_spans.push(GlyphSpan {
                line_idx,
                start: global_start,
                end: global_end,
                x_min: gx_min,
                x_max: gx_max,
            });
            if gx_max > row_x_max {
                row_x_max = gx_max;
            }
            if line_idx == 0 && gx_max > shaped_end_x {
                shaped_end_x = gx_max;
            }
        }
        line_metrics.push(LineMetrics {
            line_top: run.line_top,
            line_height: run.line_height,
            x_end: row_x_max,
        });
    }
    if line_metrics.is_empty() {
        line_metrics.push(LineMetrics {
            line_top: 0.0,
            line_height: font_size * 1.2,
            x_end: 0.0,
        });
    }
    drop(buffer);

    // ── Selection highlight (painted under the glyphs) ─────────
    if let (Some(sel), Some(sel_colour)) = (selection, selection_color) {
        if sel.start_byte < sel.end_byte {
            for (row_idx, row) in line_metrics.iter().enumerate() {
                let mut row_min = f32::INFINITY;
                let mut row_max = f32::NEG_INFINITY;
                for span in &glyph_spans {
                    if span.line_idx != row_idx {
                        continue;
                    }
                    if span.end > sel.start_byte && span.start < sel.end_byte {
                        if span.x_min < row_min {
                            row_min = span.x_min;
                        }
                        if span.x_max > row_max {
                            row_max = span.x_max;
                        }
                    }
                }
                if row_min.is_finite() && row_max > row_min {
                    let mut path = Path::new();
                    path.rect(
                        left + row_min,
                        top + row.line_top,
                        row_max - row_min,
                        row.line_height,
                    );
                    canvas.fill_path(&path, &Paint::color(femto(sel_colour)));
                }
                // Multi-line selection: when the trailing `\n` of this
                // row falls inside the selection range, draw a small
                // tail past the last glyph so users see the newline
                // is included.
                if row_idx + 1 < line_metrics.len() {
                    let row_last_end = glyph_spans
                        .iter()
                        .filter(|s| s.line_idx == row_idx)
                        .map(|s| s.end)
                        .max()
                        .unwrap_or(0);
                    if row_last_end > sel.start_byte && row_last_end < sel.end_byte {
                        let tail_w = font_size * 0.4;
                        let mut path = Path::new();
                        path.rect(
                            left + row.x_end,
                            top + row.line_top,
                            tail_w,
                            row.line_height,
                        );
                        canvas.fill_path(&path, &Paint::color(femto(sel_colour)));
                    }
                }
            }
        }
    }

    // ── Glyphs ─────────────────────────────────────────────────
    for p in placements {
        // Each glyph's colour comes from its `Placement.colour`,
        // resolved against the syntax-highlighting spans at shape
        // time. Glyphs outside every span pick up the command's
        // base `colour`. The glyph cache is keyed by
        // `(cache_key, rgba)`, so different colours rebake the
        // texture lazily — no per-frame re-rasterisation when the
        // span palette is stable.
        let Some(g) = text.glyph_image(canvas, p.cache, p.colour) else {
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

    // ── Underline (IME preedit) ────────────────────────────────
    if let (Some(ul), Some(ul_colour)) = (underline, underline_color) {
        if ul.start_byte < ul.end_byte {
            for (row_idx, row) in line_metrics.iter().enumerate() {
                let mut row_min = f32::INFINITY;
                let mut row_max = f32::NEG_INFINITY;
                for span in &glyph_spans {
                    if span.line_idx != row_idx {
                        continue;
                    }
                    if span.end > ul.start_byte && span.start < ul.end_byte {
                        if span.x_min < row_min {
                            row_min = span.x_min;
                        }
                        if span.x_max > row_max {
                            row_max = span.x_max;
                        }
                    }
                }
                if row_min.is_finite() && row_max > row_min {
                    // Draw a 1.5-px bar near the bottom of the row.
                    // Sits just *under* the baseline so it reads as an
                    // underline rather than a strikethrough or a
                    // border.
                    let stroke_h = 1.5_f32;
                    let stroke_y = top + row.line_top + row.line_height - stroke_h - 1.0;
                    let mut path = Path::new();
                    path.rect(left + row_min, stroke_y, row_max - row_min, stroke_h);
                    canvas.fill_path(&path, &Paint::color(femto(ul_colour)));
                }
            }
        }
    }

    // ── Glyph outlines (matching brackets, …) ──────────────────
    if !glyph_outlines.is_empty() {
        if let Some(outline_colour) = glyph_outline_color {
            for byte in glyph_outlines {
                // Find a glyph whose start is `byte` — the canonical
                // "outline this character" entry point. Fallback: if
                // no glyph starts at the byte, skip silently (the
                // bracket may have been scrolled out of view).
                if let Some(span) = glyph_spans.iter().find(|s| s.start == *byte) {
                    let row = &line_metrics[span.line_idx];
                    let mut path = Path::new();
                    path.rect(
                        left + span.x_min - 0.5,
                        top + row.line_top,
                        (span.x_max - span.x_min) + 1.0,
                        row.line_height,
                    );
                    let mut paint = Paint::color(femto(outline_colour));
                    paint.set_line_width(1.0);
                    canvas.stroke_path(&path, &paint);
                }
            }
        }
    }

    // ── Caret ──────────────────────────────────────────────────
    if let Some(caret_color) = caret {
        let (caret_x_local, caret_row_top, caret_row_height) = match caret_byte {
            Some(byte) => resolve_caret_pos(byte, &glyph_spans, &line_metrics, font_size),
            None => (
                shaped_end_x + 1.0,
                first_line_top,
                line_metrics
                    .first()
                    .map(|m| m.line_height)
                    .unwrap_or(box_height),
            ),
        };
        let (caret_top, caret_height) = if caret_byte.is_some() {
            (top + caret_row_top, caret_row_height.max(font_size * 0.9))
        } else {
            let pad = 2.0;
            (
                top + caret_row_top + pad,
                (box_height - pad * 2.0).max(font_size * 0.9),
            )
        };
        let caret_x = left + caret_x_local;
        let mut path = Path::new();
        path.rect(caret_x, caret_top, 1.5, caret_height);
        let paint = Paint::color(femto(caret_color));
        canvas.fill_path(&path, &paint);
    }
}

/// Resolve a byte offset to its `(x_local, row_top, row_height)`
/// triple. Handles caret-before-glyph, caret-after-glyph (cluster
/// boundary), end-of-line, and beyond-text cases.
fn resolve_caret_pos(
    byte: usize,
    glyph_spans: &[GlyphSpan],
    line_metrics: &[LineMetrics],
    font_size: f32,
) -> (f32, f32, f32) {
    // 1. Caret at the start of some glyph → that glyph's x_min.
    if let Some(span) = glyph_spans.iter().find(|s| s.start == byte) {
        let row = &line_metrics[span.line_idx];
        return (span.x_min, row.line_top, row.line_height);
    }
    // 2. Caret at the end of a glyph. Prefer the latest line so a
    //    caret at the end of a line (just before the `\n`) lands
    //    there, not at column 0 of the next line.
    let mut best: Option<(usize, f32)> = None;
    for span in glyph_spans {
        if span.end == byte {
            match best {
                None => best = Some((span.line_idx, span.x_max)),
                Some((line, _)) if span.line_idx >= line => {
                    best = Some((span.line_idx, span.x_max));
                }
                _ => {}
            }
        }
    }
    if let Some((line_idx, x)) = best {
        let row = &line_metrics[line_idx];
        return (x, row.line_top, row.line_height);
    }
    // 3. Caret beyond every glyph (empty buffer, trailing newline).
    if let Some(row) = line_metrics.last() {
        return (row.x_end, row.line_top, row.line_height);
    }
    (0.0, 0.0, font_size * 1.2)
}
