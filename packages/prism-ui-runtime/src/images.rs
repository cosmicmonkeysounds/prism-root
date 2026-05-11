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
pub struct ImageCache {
    loader: AssetLoader,
    // Outer entry: "we have tried"; inner Option: "decode succeeded".
    // Negative caching keeps a missing icon from re-hitting the
    // filesystem (and re-failing through resvg) every redraw.
    entries: HashMap<String, Option<ImageId>>,
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
    pub fn ensure<R: Renderer>(&mut self, canvas: &mut Canvas<R>, source: &str) -> Option<ImageId> {
        if let Some(slot) = self.entries.get(source) {
            return *slot;
        }
        let bytes = (self.loader)(source);
        let id = bytes.and_then(|b| decode_and_upload(canvas, source, &b));
        self.entries.insert(source.to_owned(), id);
        id
    }
}

fn decode_and_upload<R: Renderer>(
    canvas: &mut Canvas<R>,
    source: &str,
    bytes: &[u8],
) -> Option<ImageId> {
    let ext = source.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "svg" => decode_svg_and_upload(canvas, bytes),
        // Everything else routes through femtovg's `load_image_mem`,
        // which sniffs the format byte and decodes via the `image`
        // crate (PNG + JPEG are enabled in the workspace dep). The
        // mapping is deliberately permissive: extensions like `webp`
        // would fall through here and either decode (if the feature
        // is on) or return `None` (decode error → cache miss).
        _ => canvas.load_image_mem(bytes, ImageFlags::empty()).ok(),
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
