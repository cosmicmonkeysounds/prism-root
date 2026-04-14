# Image

An **Image** block renders a single `<img>` with configurable source, alt text, fit, and sizing.

## Fields

- **src** — image URL. Supports remote URLs, vault-managed blobs (via the Assets panel), and data URIs.
- **alt** — accessibility description. **Always fill this in.** Leave it blank only if the image is decorative.
- **fit** — `cover`, `contain`, `fill`, or `scale-down`. Controls how the image fills its container.
- **aspectRatio** — optional fixed ratio (e.g. `16 / 9`). Reserve layout space and prevent content shift on load.
- **rounded** — corner radius, from `none` to `full` (circle).
- **shadow** — optional drop shadow preset.

## Sourcing images

Three ways to get an image URL into the `src` field:

1. **Paste a URL** — works for any remote image.
2. **Drag from your file system** — Prism imports it into the vault via the Assets pipeline and sets `src` to the resulting `prism://` URL.
3. **Pick from the Assets panel** — `Shift+F` opens the VFS browser; select an image to copy its blob URL.

## Accessibility

Every non-decorative image **must** have meaningful `alt` text. Screen readers announce alt text in place of the image, and search engines use it for indexing. "Image" or "Photo" is not meaningful — describe what the image actually shows.

## Performance

- Prefer `aspectRatio` over fixed heights — it prevents layout shift when the image loads.
- Use `fit: cover` for hero backgrounds, `fit: contain` for logos, `fit: scale-down` for icons.
- Images over ~1 MB are flagged in the Assets panel. Consider optimizing them externally before import.
