//! Cosmic-text shaping + glyph rasterization, wrapped behind a
//! [`TextSystem`] that femtovg paint code holds onto across frames.
//!
//! Per `docs/dev/clay-migration-plan.md` §5.3, native + web backends
//! shape with `cosmic-text` and rasterize each glyph through swash
//! into a femtovg `ImageId` cached by `(CacheKey, RGBA)`. femtovg's
//! own rustybuzz-backed text path is intentionally not used —
//! cosmic-text is the single text engine across both backends.

use std::collections::HashMap;

use cosmic_text::{
    Attrs, Buffer, CacheKey, FontSystem, Metrics, Shaping, SwashCache, SwashContent,
};
use femtovg::imgref::ImgRef;
use femtovg::rgb::RGBA8;
use femtovg::{Canvas, ImageFlags, ImageId, ImageSource, Renderer};

use crate::command::Color;

/// One glyph baked to a femtovg image plus the placement offsets
/// cosmic-text reported (cosmic-text's `image.placement.left` is the
/// pixel offset from the glyph's pen position to the image's top-left
/// corner; `top` is positive-up from the baseline).
#[derive(Debug, Clone, Copy)]
struct GlyphImage {
    image: ImageId,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

/// Hash key that combines a cosmic-text glyph identity with the
/// requested fill colour. We bake the colour into the texture (alpha
/// mask × text RGB) because femtovg has no first-class "tint this
/// alpha-only image" paint, so different colours can't share an
/// image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    cache: CacheKey,
    rgba: u32,
}

/// Owns a `cosmic_text::FontSystem`, a `SwashCache`, and a per-canvas
/// glyph image cache. One instance per `Surface`/window — the cache
/// is keyed by `(glyph cache key, fill colour)` so re-using the same
/// glyph at the same colour is a single `draw_image` per frame.
pub struct TextSystem {
    font_system: FontSystem,
    swash_cache: SwashCache,
    glyphs: HashMap<GlyphKey, Option<GlyphImage>>,
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TextSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextSystem")
            .field("glyphs_cached", &self.glyphs.len())
            .finish()
    }
}

impl TextSystem {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            glyphs: HashMap::new(),
        }
    }

    /// Build a shaped buffer at `font_size` containing `text`. The
    /// buffer is sized to `width` and a generous height so cosmic-text
    /// has room to lay every line out before we walk the runs.
    pub fn shape(&mut self, text: &str, font_size: f32, width: f32) -> Buffer {
        let metrics = Metrics::new(font_size, font_size * 1.2);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        {
            let mut buf = buffer.borrow_with(&mut self.font_system);
            buf.set_size(Some(width.max(1.0)), Some(f32::INFINITY));
            buf.set_text(text, Attrs::new(), Shaping::Advanced);
            buf.shape_until_scroll(true);
        }
        buffer
    }

    /// Look up (or create) a femtovg image for the glyph identified by
    /// `cache_key`, baked at `colour`. Returns `None` for empty/whitespace
    /// glyphs (cosmic-text returns no swash image for them).
    pub fn glyph_image<R: Renderer>(
        &mut self,
        canvas: &mut Canvas<R>,
        cache_key: CacheKey,
        colour: Color,
    ) -> Option<GlyphRef> {
        let key = GlyphKey {
            cache: cache_key,
            rgba: rgba_pack(colour),
        };
        // Two-step (entry-or-insert) is fine here even if it costs an
        // extra hash on miss — the cache is the hot path; misses load
        // a font glyph either way.
        if !self.glyphs.contains_key(&key) {
            let baked = bake_glyph(
                canvas,
                &mut self.font_system,
                &mut self.swash_cache,
                cache_key,
                colour,
            );
            self.glyphs.insert(key, baked);
        }
        self.glyphs
            .get(&key)
            .and_then(|maybe| maybe.as_ref())
            .map(GlyphRef::from)
    }

    pub fn font_system_mut(&mut self) -> &mut FontSystem {
        &mut self.font_system
    }

    /// Resolve a pixel offset within a shaped text box to the byte
    /// index inside `text` the click is closest to. `local_x` /
    /// `local_y` are **relative to the text's shaped origin** —
    /// i.e. the same coordinate space the per-glyph
    /// `physical()` placements land in. Callers translate the
    /// global pointer position into this space by subtracting the
    /// text's bounds origin plus any in-box padding.
    ///
    /// Resolution rules:
    /// * The row whose `[line_top, line_top + line_height)` covers
    ///   `local_y` wins; clicks above the first row snap to the
    ///   first row, clicks below the last row to the last row.
    /// * On that row, the glyph whose horizontal span covers
    ///   `local_x` wins. Within a covered glyph, the byte returned
    ///   is `start` when the click lands in the glyph's left half,
    ///   `end` otherwise.
    /// * Clicks to the left of every glyph on the row land on the
    ///   row's first byte; clicks to the right land on the row's
    ///   last byte.
    /// * Empty buffers return `0`.
    pub fn byte_at(
        &mut self,
        text: &str,
        font_size: f32,
        width: f32,
        local_x: f32,
        local_y: f32,
    ) -> usize {
        if text.is_empty() {
            return 0;
        }
        let natural_w = text.chars().count() as f32 * font_size * 0.55;
        let shape_width = if width + 0.5 >= natural_w {
            width
        } else {
            f32::INFINITY
        };
        let buffer = self.shape(text, font_size, shape_width);
        // Cosmic-text reports `glyph.start` / `glyph.end` *relative
        // to the buffer line* (the slice between `\n`s), not the
        // global buffer. Walk the source once to learn each line's
        // starting byte and stamp the offset onto every glyph as we
        // collect placements.
        let mut line_offsets: Vec<usize> = Vec::with_capacity(8);
        line_offsets.push(0);
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_offsets.push(i + 1);
            }
        }
        #[derive(Clone, Copy)]
        struct LineRow {
            line_top: f32,
            line_height: f32,
        }
        let mut rows: Vec<LineRow> = Vec::new();
        // `(global_start, global_end, x_min, x_max)`
        let mut row_glyphs: Vec<Vec<(usize, usize, f32, f32)>> = Vec::new();
        for (idx, run) in buffer.layout_runs().enumerate() {
            rows.push(LineRow {
                line_top: run.line_top,
                line_height: run.line_height,
            });
            let base = line_offsets.get(idx).copied().unwrap_or_else(|| {
                // Fallback for wrapped lines (more runs than `\n`s):
                // pin to the last known offset.
                *line_offsets.last().unwrap_or(&0)
            });
            let mut row = Vec::new();
            for glyph in run.glyphs.iter() {
                row.push((
                    base + glyph.start,
                    base + glyph.end,
                    glyph.x,
                    glyph.x + glyph.w,
                ));
            }
            row_glyphs.push(row);
        }
        drop(buffer);
        if rows.is_empty() {
            return 0;
        }
        // Snap row.
        let mut chosen = 0usize;
        for (idx, r) in rows.iter().enumerate() {
            if local_y >= r.line_top && local_y < r.line_top + r.line_height {
                chosen = idx;
                break;
            }
            if local_y >= r.line_top + r.line_height {
                chosen = idx;
            }
        }
        let glyphs = &row_glyphs[chosen];
        if glyphs.is_empty() {
            // Empty source line (just a `\n`). Use the precomputed
            // line offset directly.
            return line_offsets.get(chosen).copied().unwrap_or(text.len());
        }
        if local_x <= glyphs[0].2 {
            return glyphs[0].0;
        }
        if local_x >= glyphs.last().unwrap().3 {
            return glyphs.last().unwrap().1;
        }
        for (start, end, x0, x1) in glyphs {
            if local_x >= *x0 && local_x <= *x1 {
                let mid = (*x0 + *x1) * 0.5;
                return if local_x < mid { *start } else { *end };
            }
        }
        glyphs.last().unwrap().1
    }

    /// Pixel position of the caret bar for byte offset `caret_byte`
    /// within `text` shaped at `font_size` / `width`. Returns the
    /// triple `(x_local, y_local, row_height)` in the same shaped
    /// origin space [`byte_at`] consumes. Used by the host to auto-
    /// scroll an editor viewport so the caret stays visible after
    /// an edit.
    pub fn caret_pixel_pos(
        &mut self,
        text: &str,
        font_size: f32,
        width: f32,
        caret_byte: usize,
    ) -> (f32, f32, f32) {
        let natural_w = text.chars().count() as f32 * font_size * 0.55;
        let shape_width = if width + 0.5 >= natural_w {
            width
        } else {
            f32::INFINITY
        };
        // Per-line byte offsets — cosmic-text reports glyph.start /
        // .end relative to its `BufferLine`, so the comparison
        // against `caret_byte` (a global offset) needs the line's
        // origin added.
        let mut line_offsets: Vec<usize> = Vec::with_capacity(8);
        line_offsets.push(0);
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_offsets.push(i + 1);
            }
        }
        let buffer = self.shape(text, font_size, shape_width);
        // Prefer the *last* row whose glyph ends at `caret_byte` (so
        // a caret parked on a `\n` boundary sits at the end of the
        // previous line, not column 0 of the next).
        let mut last_top: f32 = 0.0;
        let mut last_height: f32 = font_size * 1.2;
        let mut last_x: f32 = 0.0;
        let mut found = false;
        let mut start_match: Option<(f32, f32, f32)> = None;
        for (idx, run) in buffer.layout_runs().enumerate() {
            last_top = run.line_top;
            last_height = run.line_height;
            let base = line_offsets.get(idx).copied().unwrap_or(0);
            let mut row_x_end = 0.0f32;
            // If the caret lies on a row's start byte but the row has
            // no glyphs (empty line), still register the row.
            if base == caret_byte && run.glyphs.iter().next().is_none() {
                start_match = Some((0.0, run.line_top, run.line_height));
            }
            for glyph in run.glyphs.iter() {
                let global_start = base + glyph.start;
                let global_end = base + glyph.end;
                let x_min = glyph.x;
                let x_max = glyph.x + glyph.w;
                if global_start == caret_byte && start_match.is_none() {
                    start_match = Some((x_min, run.line_top, run.line_height));
                }
                if global_end == caret_byte {
                    last_x = x_max;
                    last_top = run.line_top;
                    last_height = run.line_height;
                    found = true;
                }
                if x_max > row_x_end {
                    row_x_end = x_max;
                }
            }
            if !found && caret_byte > 0 {
                last_x = row_x_end;
            }
        }
        drop(buffer);
        if let Some(t) = start_match {
            return t;
        }
        if found {
            return (last_x, last_top, last_height);
        }
        (0.0, 0.0, last_height)
    }
}

/// Borrow-friendly view of a [`GlyphImage`] handed back to paint code.
/// Same fields, but copied so the caller can drop the `&mut TextSystem`
/// borrow before issuing the femtovg draw call.
#[derive(Debug, Clone, Copy)]
pub struct GlyphRef {
    pub image: ImageId,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

impl From<&GlyphImage> for GlyphRef {
    fn from(g: &GlyphImage) -> Self {
        Self {
            image: g.image,
            left: g.left,
            top: g.top,
            width: g.width,
            height: g.height,
        }
    }
}

fn bake_glyph<R: Renderer>(
    canvas: &mut Canvas<R>,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    cache_key: CacheKey,
    colour: Color,
) -> Option<GlyphImage> {
    // Ask swash via cosmic-text for a rasterized glyph image.
    let image = swash_cache.get_image(font_system, cache_key).clone()?;
    let placement = image.placement;
    let width = placement.width;
    let height = placement.height;
    if width == 0 || height == 0 {
        return None;
    }
    let mut rgba: Vec<RGBA8> = Vec::with_capacity((width * height) as usize);
    match image.content {
        SwashContent::Mask => {
            for &alpha in &image.data {
                let scaled = ((alpha as u16 * colour.a as u16) / 255) as u8;
                rgba.push(RGBA8 {
                    r: colour.r,
                    g: colour.g,
                    b: colour.b,
                    a: scaled,
                });
            }
        }
        SwashContent::Color => {
            // Color glyphs (emoji) — swash gives BGRA premultiplied.
            for chunk in image.data.chunks_exact(4) {
                rgba.push(RGBA8 {
                    r: chunk[2],
                    g: chunk[1],
                    b: chunk[0],
                    a: chunk[3],
                });
            }
        }
        SwashContent::SubpixelMask => {
            // Treat as mask — same path, channel-collapsed. Phase 2
            // upgrade can add a real sub-pixel blit.
            for chunk in image.data.chunks_exact(3) {
                let alpha = ((chunk[0] as u16 + chunk[1] as u16 + chunk[2] as u16) / 3) as u8;
                let scaled = ((alpha as u16 * colour.a as u16) / 255) as u8;
                rgba.push(RGBA8 {
                    r: colour.r,
                    g: colour.g,
                    b: colour.b,
                    a: scaled,
                });
            }
        }
    }
    let img: ImgRef<RGBA8> = ImgRef::new(&rgba, width as usize, height as usize);
    let id = canvas
        .create_image(ImageSource::from(img), ImageFlags::empty())
        .ok()?;
    Some(GlyphImage {
        image: id,
        left: placement.left,
        top: placement.top,
        width,
        height,
    })
}

fn rgba_pack(c: Color) -> u32 {
    (c.r as u32) | ((c.g as u32) << 8) | ((c.b as u32) << 16) | ((c.a as u32) << 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_at_empty_buffer_returns_zero() {
        let mut sys = TextSystem::new();
        assert_eq!(sys.byte_at("", 13.0, 200.0, 0.0, 0.0), 0);
    }

    #[test]
    fn byte_at_negative_y_snaps_to_first_row() {
        let mut sys = TextSystem::new();
        let b = sys.byte_at("hello", 13.0, f32::INFINITY, 0.0, -10.0);
        assert_eq!(b, 0);
    }

    #[test]
    fn byte_at_left_of_text_returns_first_byte() {
        let mut sys = TextSystem::new();
        let b = sys.byte_at("hello", 13.0, f32::INFINITY, -100.0, 6.0);
        assert_eq!(b, 0);
    }

    #[test]
    fn byte_at_far_right_returns_last_byte() {
        let mut sys = TextSystem::new();
        let b = sys.byte_at("hello", 13.0, f32::INFINITY, 1000.0, 6.0);
        // End of "hello" — 5 bytes.
        assert_eq!(b, 5);
    }

    #[test]
    fn byte_at_multiline_resolves_to_second_line() {
        let mut sys = TextSystem::new();
        let buf = sys.shape("aaa\nbbb", 13.0, f32::INFINITY);
        let row_metrics: Vec<(f32, f32)> = buf
            .layout_runs()
            .map(|r| (r.line_top, r.line_height))
            .collect();
        drop(buf);
        if row_metrics.len() < 2 {
            // Some font systems may not emit per-line runs; skip
            // rather than fail a build-machine variant.
            return;
        }
        // Click in the centre of the second row.
        let (top, height) = row_metrics[1];
        let y_centre = top + height * 0.5;
        let b = sys.byte_at("aaa\nbbb", 13.0, f32::INFINITY, 0.0, y_centre);
        // "aaa\nbbb" — line 1 starts at byte 4.
        assert_eq!(b, 4);
    }

    #[test]
    fn caret_pixel_pos_first_byte_is_zero_x() {
        let mut sys = TextSystem::new();
        let (x, _y, _h) = sys.caret_pixel_pos("hello", 13.0, f32::INFINITY, 0);
        assert!(x < 1.0, "expected x ≈ 0, got {x}");
    }

    #[test]
    fn caret_pixel_pos_monotonic_across_bytes() {
        let mut sys = TextSystem::new();
        let (x0, _, _) = sys.caret_pixel_pos("hello world", 13.0, f32::INFINITY, 0);
        let (x6, _, _) = sys.caret_pixel_pos("hello world", 13.0, f32::INFINITY, 6);
        let (x11, _, _) = sys.caret_pixel_pos("hello world", 13.0, f32::INFINITY, 11);
        assert!(x0 < x6);
        assert!(x6 < x11);
    }
}
