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
