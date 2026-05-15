//! `ImageCache` — femtovg `ImageId` cache backing `RenderCommand::Image`.
//!
//! The runtime's render-command stream carries images as opaque
//! `source` strings (`"icons/box.svg"`, `"/asset/abc123"`, …). The
//! host plugs an [`AssetLoader`] closure that maps those strings to
//! raw bytes — filesystem in dev, `include_bytes!` in distribution,
//! a custom resolver for tests. This module owns the decode pass
//! (PNG / JPEG via the `image` crate, SVG via resvg) plus a
//! per-canvas [`femtovg::ImageId`] cache so each source uploads once.
//!
//! Decode failures and unknown sources are cached as `None` so a
//! missing asset doesn't re-decode every frame.
//!
//! See `docs/dev/ui-migration-followups.md` item B1.

use std::collections::HashMap;
use std::sync::Arc;

use femtovg::{Canvas, ImageFlags, ImageId, ImageSource, Renderer};

/// Host-supplied closure that maps a logical source string (the
/// value carried by [`crate::command::RenderCommand::Image::source`])
/// to its raw bytes. Returning `None` signals "unknown source" — the
/// cache treats that as a permanent miss for this `ImageCache`'s
/// lifetime.
pub type AssetLoader = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/// Build a no-op loader. Convenient for tests / SSR-style hosts that
/// don't surface raster images.
pub fn noop_loader() -> AssetLoader {
    Arc::new(|_| None)
}

/// Build a filesystem loader. Search roots are tried in order; the
/// first that resolves wins. Absolute paths in the source string
/// skip the search entirely.
#[cfg(not(target_arch = "wasm32"))]
pub fn filesystem_loader(roots: Vec<std::path::PathBuf>) -> AssetLoader {
    Arc::new(move |source: &str| -> Option<Vec<u8>> {
        let path = std::path::Path::new(source);
        if path.is_absolute() {
            return std::fs::read(path).ok();
        }
        for root in &roots {
            let candidate = root.join(source);
            if let Ok(bytes) = std::fs::read(&candidate) {
                return Some(bytes);
            }
        }
        None
    })
}

/// Per-`Canvas` image cache. Holds decoded `ImageId`s keyed by the
/// logical source string. Re-creating the cache invalidates every
/// id — the host's owning struct (e.g. `App` in the femtovg
/// backend) recreates the cache when the canvas is rebuilt.
///
/// **Wave 14.7 — animated formats**: GIF / animated WebP / APNG
/// decode into a [`AnimatedEntry`] (`Vec<(ImageId, delay_ms)>` plus
/// total duration). The painter samples the current frame via
/// [`Self::ensure_frame`], passing `now_ms` from the backend's
/// monotonic clock. Static formats (PNG / JPEG / single-frame WebP
/// / BMP / TIFF / SVG) cache as a single `ImageId` and ignore the
/// clock argument.
pub struct ImageCache {
    loader: AssetLoader,
    /// Outer entry: "we have tried"; inner enum: per-source decode
    /// state. Negative caching keeps a missing asset from
    /// re-hitting the filesystem (and re-failing through resvg)
    /// every redraw.
    entries: HashMap<String, CacheEntry>,
}

/// One slot in [`ImageCache::entries`]. `Missing` captures the
/// negative-cache case so retries don't re-walk the filesystem;
/// `Static` and `Animated` carry the live `ImageId`(s).
enum CacheEntry {
    Missing,
    Static(ImageId),
    Animated(AnimatedEntry),
}

/// Multi-frame container decoded once per source. Frames keep their
/// per-frame delay so the painter can advance through GIF / APNG /
/// animated-WebP loops at author-declared cadence.
struct AnimatedEntry {
    frames: Vec<AnimatedFrame>,
    /// Sum of every frame's `delay_ms` — used by `frame_index_at`
    /// to wrap a monotonic clock into a loop position. `0` means
    /// the decoded file declared only zero-duration frames, in
    /// which case we hold on the first frame (avoids div-by-zero).
    total_ms: u32,
}

struct AnimatedFrame {
    id: ImageId,
    /// Per-frame display duration in milliseconds. GIFs encode
    /// delay in centiseconds (1/100s); WebP / APNG carry it as a
    /// fraction with an explicit denominator. Both round to whole
    /// milliseconds here — sub-millisecond timing is below the
    /// repaint cadence anyway.
    delay_ms: u32,
}

impl ImageCache {
    pub fn new(loader: AssetLoader) -> Self {
        Self {
            loader,
            entries: HashMap::new(),
        }
    }

    /// Resolve `source` to an `ImageId`, decoding + uploading on the
    /// first reference. Returns `None` when the loader has no bytes
    /// for the source, or when decode failed. Subsequent hits for
    /// the same source — including the negative case — skip the
    /// loader and decoder; a missing icon doesn't re-hit the
    /// filesystem every frame.
    ///
    /// For animated sources, returns the **first** frame's id.
    /// Callers that want to play the animation use
    /// [`Self::ensure_frame`] with a monotonic clock.
    pub fn ensure<R: Renderer>(&mut self, canvas: &mut Canvas<R>, source: &str) -> Option<ImageId> {
        match self.populate(canvas, source) {
            CacheLookup::Static(id) => Some(*id),
            CacheLookup::Animated(a) => a.frames.first().map(|f| f.id),
            CacheLookup::Missing => None,
        }
    }

    /// Wave 14.7 — same as [`Self::ensure`] but rotates through
    /// animated frames against the supplied monotonic clock. For
    /// static sources `now_ms` is ignored. Returns `None` when the
    /// source decoded to zero frames (treated as Missing).
    pub fn ensure_frame<R: Renderer>(
        &mut self,
        canvas: &mut Canvas<R>,
        source: &str,
        now_ms: u64,
    ) -> Option<ImageId> {
        match self.populate(canvas, source) {
            CacheLookup::Static(id) => Some(*id),
            CacheLookup::Animated(a) => {
                let idx = frame_index_at(a, now_ms);
                a.frames.get(idx).map(|f| f.id)
            }
            CacheLookup::Missing => None,
        }
    }

    /// Wave 14.7 — does any entry hold an animated source? The host
    /// merges this into its per-frame "request a redraw" bit so
    /// GIF / APNG loops keep ticking without an explicit timer.
    pub fn has_animations(&self) -> bool {
        self.entries
            .values()
            .any(|e| matches!(e, CacheEntry::Animated(_)))
    }

    fn populate<R: Renderer>(&mut self, canvas: &mut Canvas<R>, source: &str) -> CacheLookup<'_> {
        if !self.entries.contains_key(source) {
            let bytes = (self.loader)(source);
            let entry = match bytes {
                Some(b) => decode_entry(canvas, source, &b).unwrap_or(CacheEntry::Missing),
                None => CacheEntry::Missing,
            };
            self.entries.insert(source.to_owned(), entry);
        }
        match self.entries.get(source).expect("just inserted") {
            CacheEntry::Static(id) => CacheLookup::Static(id),
            CacheEntry::Animated(a) => CacheLookup::Animated(a),
            CacheEntry::Missing => CacheLookup::Missing,
        }
    }
}

enum CacheLookup<'a> {
    Missing,
    Static(&'a ImageId),
    Animated(&'a AnimatedEntry),
}

/// Wrap `now_ms` into the animation's loop and return the index of
/// the frame that should currently paint. Walks the cumulative
/// delays from frame 0; the linear scan is fine for sensible frame
/// counts (a typical GIF has 10–200 frames). Zero-duration loops
/// hold on frame 0.
fn frame_index_at(a: &AnimatedEntry, now_ms: u64) -> usize {
    frame_index_in_delays(
        a.frames.iter().map(|f| f.delay_ms),
        a.total_ms,
        a.frames.len(),
        now_ms,
    )
}

/// Pure-data helper extracted so the math is testable without
/// allocating real `ImageId`s (femtovg's id type is `NonZero` and
/// rejects zero-init). Takes the per-frame delays as an iterator,
/// the precomputed total duration, and the frame count.
fn frame_index_in_delays<I>(delays: I, total_ms: u32, len: usize, now_ms: u64) -> usize
where
    I: IntoIterator<Item = u32>,
{
    if len == 0 || total_ms == 0 {
        return 0;
    }
    let mut t = (now_ms % total_ms as u64) as u32;
    for (i, delay) in delays.into_iter().enumerate() {
        let step = delay.max(1);
        if t < step {
            return i;
        }
        t = t.saturating_sub(step);
    }
    len - 1
}

fn decode_entry<R: Renderer>(
    canvas: &mut Canvas<R>,
    source: &str,
    bytes: &[u8],
) -> Option<CacheEntry> {
    let ext = source.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "svg" => decode_svg_and_upload(canvas, bytes).map(CacheEntry::Static),
        // Wave 14.7 — animated formats. The `image` crate's
        // `AnimationDecoder` trait yields a frame iterator with
        // per-frame delays. We decode once and upload each frame as
        // its own `ImageId`; the painter rotates between them via
        // `ensure_frame`. Multi-frame GIF / animated WebP / APNG
        // all flow through here; static single-frame files in those
        // formats degrade to the `Static` arm below.
        "gif" => decode_gif_animation(canvas, bytes).or_else(|| {
            canvas
                .load_image_mem(bytes, ImageFlags::empty())
                .ok()
                .map(CacheEntry::Static)
        }),
        "apng" => decode_apng_animation(canvas, bytes).or_else(|| {
            canvas
                .load_image_mem(bytes, ImageFlags::empty())
                .ok()
                .map(CacheEntry::Static)
        }),
        "webp" => decode_webp_animation(canvas, bytes).or_else(|| {
            canvas
                .load_image_mem(bytes, ImageFlags::empty())
                .ok()
                .map(CacheEntry::Static)
        }),
        // Everything else routes through femtovg's `load_image_mem`,
        // which sniffs the format byte and decodes via the `image`
        // crate (PNG + JPEG + BMP + TIFF are enabled in the
        // workspace dep). Unknown extensions still try this path —
        // a misnamed PNG (`.bin`) still loads.
        _ => canvas
            .load_image_mem(bytes, ImageFlags::empty())
            .ok()
            .map(CacheEntry::Static),
    }
}

/// Decode an animated GIF into a sequence of `(ImageId, delay_ms)`
/// frames. Single-frame GIFs return `Some(CacheEntry::Animated{
/// frames: 1, total_ms: <delay or 0> })` — callers can detect via
/// `frames.len() == 1` if they want to treat it as static.
/// Returns `None` when the decode fails (format mismatch, corrupted
/// bytes, zero frames).
fn decode_gif_animation<R: Renderer>(canvas: &mut Canvas<R>, bytes: &[u8]) -> Option<CacheEntry> {
    use image::codecs::gif::GifDecoder;
    use image::AnimationDecoder;
    let decoder = GifDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let frames = decoder.into_frames().collect_frames().ok()?;
    decode_frames_to_entry(canvas, frames)
}

/// APNG animations route through `PngDecoder::apng()` which itself
/// returns an `ApngDecoder` that implements `AnimationDecoder`.
/// Static PNGs fall through this arm via the `_ => Static` branch
/// (the `.apng` extension is the opt-in marker).
fn decode_apng_animation<R: Renderer>(canvas: &mut Canvas<R>, bytes: &[u8]) -> Option<CacheEntry> {
    use image::codecs::png::PngDecoder;
    use image::AnimationDecoder;
    let decoder = PngDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let apng = decoder.apng().ok()?;
    let frames = apng.into_frames().collect_frames().ok()?;
    decode_frames_to_entry(canvas, frames)
}

/// Animated WebP via `WebPDecoder`. Single-frame WebP falls back to
/// the static `load_image_mem` arm.
fn decode_webp_animation<R: Renderer>(canvas: &mut Canvas<R>, bytes: &[u8]) -> Option<CacheEntry> {
    use image::codecs::webp::WebPDecoder;
    use image::AnimationDecoder;
    let decoder = WebPDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let frames = decoder.into_frames().collect_frames().ok()?;
    decode_frames_to_entry(canvas, frames)
}

/// Common upload + cache shape — takes the decoded `Vec<Frame>` and
/// emits a `CacheEntry::Animated` with each frame uploaded once.
/// Returns `None` when the frame list is empty or every individual
/// upload failed.
fn decode_frames_to_entry<R: Renderer>(
    canvas: &mut Canvas<R>,
    frames: Vec<image::Frame>,
) -> Option<CacheEntry> {
    if frames.is_empty() {
        return None;
    }
    let mut out: Vec<AnimatedFrame> = Vec::with_capacity(frames.len());
    let mut total_ms = 0u32;
    for frame in frames {
        let delay = frame.delay();
        let (num, den) = delay.numer_denom_ms();
        let delay_ms = if den == 0 {
            0
        } else {
            (num as f64 / den as f64).round() as u32
        };
        let rgba = frame.into_buffer();
        let dyn_img = image::DynamicImage::ImageRgba8(rgba);
        let src = match ImageSource::try_from(&dyn_img) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let Ok(id) = canvas.create_image(src, ImageFlags::empty()) else {
            continue;
        };
        out.push(AnimatedFrame { id, delay_ms });
        total_ms = total_ms.saturating_add(delay_ms);
    }
    if out.is_empty() {
        None
    } else {
        Some(CacheEntry::Animated(AnimatedEntry {
            frames: out,
            total_ms,
        }))
    }
}

fn decode_svg_and_upload<R: Renderer>(canvas: &mut Canvas<R>, bytes: &[u8]) -> Option<ImageId> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(bytes, &opt).ok()?;
    let size = tree.size().to_int_size();
    let width = size.width().max(1);
    let height = size.height().max(1);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    // resvg / tiny-skia emits **pre-multiplied** RGBA. femtovg's
    // `Rgba` source treats incoming bytes as straight RGBA, so a
    // pre-multiplied buffer paints darker than authored. Convert in
    // place — divide each channel by alpha — before upload.
    let mut data = pixmap.take();
    unpremultiply_rgba(&mut data);
    // Wrap the pixmap as a `DynamicImage::ImageRgba8` so femtovg's
    // `image-loading` adapter (`TryFrom<&DynamicImage>`) can hand
    // back the typed `ImageSource::Rgba(...)` we need for upload —
    // no extra dep on `imgref` / `rgb` here.
    let rgba = image::RgbaImage::from_raw(width, height, data)?;
    let dyn_img = image::DynamicImage::ImageRgba8(rgba);
    let src = ImageSource::try_from(&dyn_img).ok()?;
    canvas.create_image(src, ImageFlags::empty()).ok()
}

/// Convert pre-multiplied RGBA bytes to straight RGBA in place. SVG
/// rasterisers (resvg / tiny-skia) emit pre-multiplied alpha because
/// it composites cheaply; femtovg's [`ImageSource::Rgba`] reader
/// expects straight alpha, so without this conversion every icon
/// paints noticeably darker than the source SVG specifies.
fn unpremultiply_rgba(buf: &mut [u8]) {
    for px in buf.chunks_exact_mut(4) {
        let a = px[3];
        if a == 0 || a == 255 {
            continue;
        }
        let a_f = a as f32 / 255.0;
        px[0] = ((px[0] as f32 / a_f).round().min(255.0)) as u8;
        px[1] = ((px[1] as f32 / a_f).round().min(255.0)) as u8;
        px[2] = ((px[2] as f32 / a_f).round().min(255.0)) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wave 14.7 — `frame_index_in_delays` wraps a monotonic clock
    /// into a loop position. Pure-data test against the extracted
    /// math helper so the `Canvas`/`ImageId` allocator doesn't need
    /// to participate.
    #[test]
    fn frame_index_in_delays_wraps_monotonic_clock_into_loop() {
        // Three-frame entry: 100ms, 200ms, 100ms (total 400ms).
        let delays = [100u32, 200, 100];
        let total = delays.iter().sum::<u32>();
        let len = delays.len();
        // First frame: 0..100ms.
        assert_eq!(frame_index_in_delays(delays, total, len, 0), 0);
        assert_eq!(frame_index_in_delays(delays, total, len, 99), 0);
        // Second frame: 100..300ms.
        assert_eq!(frame_index_in_delays(delays, total, len, 100), 1);
        assert_eq!(frame_index_in_delays(delays, total, len, 299), 1);
        // Third frame: 300..400ms.
        assert_eq!(frame_index_in_delays(delays, total, len, 300), 2);
        assert_eq!(frame_index_in_delays(delays, total, len, 399), 2);
        // Wrap: 400ms == 0ms; 800ms wraps twice.
        assert_eq!(frame_index_in_delays(delays, total, len, 400), 0);
        assert_eq!(frame_index_in_delays(delays, total, len, 800), 0);
    }

    /// Wave 14.7 — zero-duration animations hold on frame 0 instead
    /// of div-by-zero. GIFs declaring 0-cs delays are common
    /// (synthetic exporters that don't realise the spec interprets
    /// 0 as "very fast").
    #[test]
    fn frame_index_in_delays_holds_on_zero_duration_animation() {
        let delays = [0u32];
        assert_eq!(frame_index_in_delays(delays, 0, 1, 0), 0);
        assert_eq!(frame_index_in_delays(delays, 0, 1, 1_000_000), 0);
    }

    /// Empty animations also hold on frame 0 — a defensive guard
    /// against pathological decode output. The painter still treats
    /// a length-0 entry as Missing higher up.
    #[test]
    fn frame_index_in_delays_returns_zero_for_empty_animation() {
        let delays: [u32; 0] = [];
        assert_eq!(frame_index_in_delays(delays, 0, 0, 100), 0);
    }

    #[test]
    fn noop_loader_returns_none_for_every_source() {
        let load = noop_loader();
        assert!(load("anything").is_none());
        assert!(load("").is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn filesystem_loader_resolves_against_each_root_in_order() {
        let dir = tempdir_with_file("seek.txt", b"hit");
        let load = filesystem_loader(vec![dir.path().to_owned()]);
        assert_eq!(load("seek.txt").as_deref(), Some(b"hit".as_slice()));
        assert!(load("missing.txt").is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn unpremultiply_round_trips_opaque_and_zero_alpha_pixels() {
        // Two edge cases the in-place loop short-circuits — make sure
        // we don't corrupt them.
        let mut buf = vec![
            10, 20, 30, 255, // fully opaque: untouched
            10, 20, 30, 0, // fully transparent: untouched
            50, 50, 50, 128, // 50% alpha pre-multiplied → ~100,100,100 straight
        ];
        unpremultiply_rgba(&mut buf);
        assert_eq!(&buf[0..4], &[10, 20, 30, 255]);
        assert_eq!(&buf[4..8], &[10, 20, 30, 0]);
        // 50 / 0.5 = 100 (rounded).
        assert_eq!(&buf[8..12], &[100, 100, 100, 128]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn tempdir_with_file(name: &str, contents: &[u8]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(name), contents).expect("write");
        dir
    }
}
