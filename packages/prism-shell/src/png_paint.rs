//! Wave 7.2 of `docs/dev/composable-builder-plan.md` — software
//! rasteriser for the headless `--screenshot` PNG path. Walks a
//! `prism_ui_runtime::command::RenderCommand` stream and paints
//! into a flat RGBA buffer.
//!
//! The rasteriser is intentionally minimal: solid colour rectangles
//! (front-to-back alpha blending), rectangular borders, scissor
//! clipping, image and text placeholders. It does **not** call
//! through a GPU path, which is what unblocks PNG capture in
//! headless CI / tests where no GL context is available. The
//! femtovg backend is the production-quality path; this is the
//! deterministic-text-diff-equivalent **and** captures real layout
//! through the same Taffy pass.
//!
//! When a real offscreen femtovg surface lands the encoder seam in
//! `headless::Shell::dump_png` swaps target without touching this
//! module's contract.

use prism_ui_runtime::command::{Color, CornerRadius, Rect, RenderCommand};

/// Rasterise a `RenderCommand` stream into an RGBA8 buffer of
/// `width × height` pixels (one byte per channel, row-major,
/// top-left origin). The returned buffer is `4 * width * height`
/// bytes long.
pub fn rasterize(commands: &[RenderCommand], width: u32, height: u32) -> Vec<u8> {
    let mut buffer = vec![0u8; (width as usize) * (height as usize) * 4];
    // Clip stack — each ScissorStart pushes; ScissorEnd pops. The
    // active clip is `clips.last()` (or the viewport when empty).
    let mut clips: Vec<Rect> = Vec::new();
    for cmd in commands {
        match cmd {
            RenderCommand::Rectangle {
                bounds,
                color,
                radius,
            } => {
                let clip = current_clip(&clips, width, height);
                fill_rounded_rect(&mut buffer, width, height, bounds, radius, *color, &clip);
            }
            RenderCommand::Border {
                bounds,
                color,
                width: stroke,
                radius: _,
            } => {
                let clip = current_clip(&clips, width, height);
                stroke_border(&mut buffer, width, height, bounds, *stroke, *color, &clip);
            }
            RenderCommand::Text {
                bounds,
                color,
                caret,
                content: _,
                font_size: _,
                caret_byte: _,
                selection: _,
                selection_color: _,
                spans: _,
                underline: _,
                underline_color: _,
                glyph_outlines: _,
                glyph_outline_color: _,
            } => {
                // Glyph rasterisation isn't in scope for the
                // software path. Paint a single-pixel-tall accent
                // strip at the text baseline so before/after diffs
                // still show where text *was* — enough to catch a
                // missing label or a wandered baseline. The full
                // glyph paint lands when femtovg offscreen does.
                let clip = current_clip(&clips, width, height);
                let baseline = Rect {
                    x: bounds.x,
                    y: bounds.y + bounds.height - 1.0,
                    width: bounds.width,
                    height: 1.0,
                };
                fill_rect(&mut buffer, width, height, &baseline, *color, &clip);
                if let Some(caret_color) = caret {
                    let caret_rect = Rect {
                        x: bounds.x + bounds.width,
                        y: bounds.y,
                        width: 1.5,
                        height: bounds.height,
                    };
                    fill_rect(&mut buffer, width, height, &caret_rect, *caret_color, &clip);
                }
            }
            RenderCommand::Image { bounds, tint, .. } => {
                // Image decoding isn't in scope. Paint a tinted
                // placeholder rect — either the caller's tint, or a
                // neutral grey when the icon paints as-is.
                let fill = tint.unwrap_or(Color {
                    r: 0x66,
                    g: 0x66,
                    b: 0x66,
                    a: 0xff,
                });
                let clip = current_clip(&clips, width, height);
                fill_rect(&mut buffer, width, height, bounds, fill, &clip);
            }
            RenderCommand::ScissorStart { bounds } => {
                let next = match clips.last() {
                    Some(parent) => intersect(parent, bounds),
                    None => *bounds,
                };
                clips.push(next);
            }
            RenderCommand::ScissorEnd => {
                clips.pop();
            }
            RenderCommand::Hint { .. } => {
                // Hint commands target the HTML backend; the PNG
                // sink has nothing to do with them.
            }
        }
    }
    buffer
}

fn current_clip(clips: &[Rect], width: u32, height: u32) -> Rect {
    match clips.last() {
        Some(c) => *c,
        None => Rect {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
        },
    }
}

fn intersect(a: &Rect, b: &Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    Rect {
        x,
        y,
        width: (right - x).max(0.0),
        height: (bottom - y).max(0.0),
    }
}

fn fill_rect(buffer: &mut [u8], w: u32, h: u32, bounds: &Rect, color: Color, clip: &Rect) {
    let area = intersect(bounds, clip);
    paint_solid(buffer, w, h, &area, color);
}

fn fill_rounded_rect(
    buffer: &mut [u8],
    w: u32,
    h: u32,
    bounds: &Rect,
    radius: &CornerRadius,
    color: Color,
    clip: &Rect,
) {
    // The software path treats corner radius as a *mask* applied
    // on top of the sharp fill. Each corner is a quarter-circle:
    // pixels outside the radius arc clip out. The mask runs only
    // when at least one radius is non-zero — otherwise we drop
    // straight to the sharp `fill_rect` to keep the hot path fast.
    if radius.tl == 0.0 && radius.tr == 0.0 && radius.br == 0.0 && radius.bl == 0.0 {
        fill_rect(buffer, w, h, bounds, color, clip);
        return;
    }
    let area = intersect(bounds, clip);
    if area.width <= 0.0 || area.height <= 0.0 {
        return;
    }
    let x0 = area.x.floor().max(0.0) as i32;
    let y0 = area.y.floor().max(0.0) as i32;
    let x1 = (area.x + area.width).ceil().min(w as f32) as i32;
    let y1 = (area.y + area.height).ceil().min(h as f32) as i32;
    let bx = bounds.x;
    let by = bounds.y;
    let bw = bounds.width;
    let bh = bounds.height;
    let r_tl = radius.tl.min(bw * 0.5).min(bh * 0.5).max(0.0);
    let r_tr = radius.tr.min(bw * 0.5).min(bh * 0.5).max(0.0);
    let r_br = radius.br.min(bw * 0.5).min(bh * 0.5).max(0.0);
    let r_bl = radius.bl.min(bw * 0.5).min(bh * 0.5).max(0.0);
    for py in y0..y1 {
        for px in x0..x1 {
            let cx = px as f32 + 0.5;
            let cy = py as f32 + 0.5;
            let inside = corner_test(cx, cy, bx, by, bw, bh, r_tl, r_tr, r_br, r_bl);
            if !inside {
                continue;
            }
            blend_pixel(buffer, w, px as u32, py as u32, color);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn corner_test(
    px: f32,
    py: f32,
    bx: f32,
    by: f32,
    bw: f32,
    bh: f32,
    r_tl: f32,
    r_tr: f32,
    r_br: f32,
    r_bl: f32,
) -> bool {
    // Top-left
    if r_tl > 0.0 && px < bx + r_tl && py < by + r_tl {
        let dx = bx + r_tl - px;
        let dy = by + r_tl - py;
        return dx * dx + dy * dy <= r_tl * r_tl;
    }
    // Top-right
    if r_tr > 0.0 && px > bx + bw - r_tr && py < by + r_tr {
        let dx = px - (bx + bw - r_tr);
        let dy = by + r_tr - py;
        return dx * dx + dy * dy <= r_tr * r_tr;
    }
    // Bottom-right
    if r_br > 0.0 && px > bx + bw - r_br && py > by + bh - r_br {
        let dx = px - (bx + bw - r_br);
        let dy = py - (by + bh - r_br);
        return dx * dx + dy * dy <= r_br * r_br;
    }
    // Bottom-left
    if r_bl > 0.0 && px < bx + r_bl && py > by + bh - r_bl {
        let dx = bx + r_bl - px;
        let dy = py - (by + bh - r_bl);
        return dx * dx + dy * dy <= r_bl * r_bl;
    }
    true
}

fn stroke_border(
    buffer: &mut [u8],
    w: u32,
    h: u32,
    bounds: &Rect,
    stroke: f32,
    color: Color,
    clip: &Rect,
) {
    let s = stroke.max(1.0);
    let top = Rect {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: s,
    };
    let bottom = Rect {
        x: bounds.x,
        y: bounds.y + bounds.height - s,
        width: bounds.width,
        height: s,
    };
    let left = Rect {
        x: bounds.x,
        y: bounds.y,
        width: s,
        height: bounds.height,
    };
    let right = Rect {
        x: bounds.x + bounds.width - s,
        y: bounds.y,
        width: s,
        height: bounds.height,
    };
    fill_rect(buffer, w, h, &top, color, clip);
    fill_rect(buffer, w, h, &bottom, color, clip);
    fill_rect(buffer, w, h, &left, color, clip);
    fill_rect(buffer, w, h, &right, color, clip);
}

fn paint_solid(buffer: &mut [u8], w: u32, h: u32, area: &Rect, color: Color) {
    if area.width <= 0.0 || area.height <= 0.0 {
        return;
    }
    let x0 = area.x.floor().max(0.0) as i32;
    let y0 = area.y.floor().max(0.0) as i32;
    let x1 = (area.x + area.width).ceil().min(w as f32) as i32;
    let y1 = (area.y + area.height).ceil().min(h as f32) as i32;
    for py in y0..y1 {
        for px in x0..x1 {
            blend_pixel(buffer, w, px as u32, py as u32, color);
        }
    }
}

fn blend_pixel(buffer: &mut [u8], w: u32, x: u32, y: u32, src: Color) {
    if src.a == 0 {
        return;
    }
    let idx = (y as usize * w as usize + x as usize) * 4;
    if idx + 3 >= buffer.len() {
        return;
    }
    let sa = src.a as u32;
    let dr = buffer[idx] as u32;
    let dg = buffer[idx + 1] as u32;
    let db = buffer[idx + 2] as u32;
    let da = buffer[idx + 3] as u32;
    let inv = 255 - sa;
    let r = (src.r as u32 * sa + dr * inv) / 255;
    let g = (src.g as u32 * sa + dg * inv) / 255;
    let b = (src.b as u32 * sa + db * inv) / 255;
    let a = (sa * 255 + da * inv) / 255;
    buffer[idx] = r as u8;
    buffer[idx + 1] = g as u8;
    buffer[idx + 2] = b as u8;
    buffer[idx + 3] = a.min(255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_ui_runtime::command::{Color, CornerRadius, Rect};

    fn opaque(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 0xff }
    }

    #[test]
    fn empty_command_stream_yields_transparent_buffer() {
        let buf = rasterize(&[], 8, 8);
        assert_eq!(buf.len(), 8 * 8 * 4);
        assert!(buf.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn rectangle_paints_solid_block() {
        let cmds = vec![RenderCommand::Rectangle {
            bounds: Rect {
                x: 1.0,
                y: 1.0,
                width: 2.0,
                height: 2.0,
            },
            color: opaque(0xff, 0, 0),
            radius: CornerRadius::default(),
        }];
        let buf = rasterize(&cmds, 4, 4);
        // Pixel (1,1) should be opaque red.
        let idx = (4 + 1) * 4;
        assert_eq!(&buf[idx..idx + 4], &[0xff, 0, 0, 0xff]);
        // Pixel (0,0) untouched.
        assert_eq!(&buf[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn alpha_blend_combines_with_existing() {
        let cmds = vec![
            RenderCommand::Rectangle {
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                color: opaque(0xff, 0xff, 0xff),
                radius: CornerRadius::default(),
            },
            RenderCommand::Rectangle {
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0x80,
                },
                radius: CornerRadius::default(),
            },
        ];
        let buf = rasterize(&cmds, 1, 1);
        // 50%-ish blend of white and black ≈ 127.
        assert!(buf[0] >= 120 && buf[0] <= 135);
    }

    #[test]
    fn scissor_clips_subsequent_rectangles() {
        let cmds = vec![
            RenderCommand::ScissorStart {
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
            },
            RenderCommand::Rectangle {
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 4.0,
                    height: 4.0,
                },
                color: opaque(0xff, 0, 0),
                radius: CornerRadius::default(),
            },
            RenderCommand::ScissorEnd,
        ];
        let buf = rasterize(&cmds, 4, 4);
        // Only the top-left pixel is filled.
        assert_eq!(&buf[0..4], &[0xff, 0, 0, 0xff]);
        let idx = (4 + 1) * 4;
        assert_eq!(&buf[idx..idx + 4], &[0, 0, 0, 0]);
    }

    #[test]
    fn rounded_corners_clip_corner_pixel() {
        let cmds = vec![RenderCommand::Rectangle {
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
            },
            color: opaque(0xff, 0, 0),
            radius: CornerRadius {
                tl: 2.0,
                tr: 0.0,
                br: 0.0,
                bl: 0.0,
            },
        }];
        let buf = rasterize(&cmds, 4, 4);
        // (0,0) sits outside the 2-px TL quarter-circle → transparent.
        assert_eq!(&buf[0..4], &[0, 0, 0, 0]);
        // (3,3) is on the un-rounded BR corner → opaque red.
        let idx = (3 * 4 + 3) * 4;
        assert_eq!(&buf[idx..idx + 4], &[0xff, 0, 0, 0xff]);
    }
}
