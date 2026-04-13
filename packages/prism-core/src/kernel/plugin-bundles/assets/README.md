# plugin-bundles/assets

Asset Management bundle. Registers media assets, content items, scanned documents, and user-defined collections as new Flux entity types, with edges linking them into collections, derivations, and attachments on other objects.

```ts
import { createAssetsBundle } from "@prism/core/plugin-bundles";
```

## What it registers

- **Categories** (`ASSETS_CATEGORIES`): `assets:media`, `assets:content`, `assets:collections`.
- **Entity types** (`ASSETS_TYPES`): `MediaAsset`, `ContentItem`, `ScannedDoc`, `Collection`.
- **Edges** (`ASSETS_EDGES`): `in-collection`, `derived-from`, `attached-to`.
- **Status enums**: `MEDIA_STATUSES`, `CONTENT_STATUSES`, `SCAN_STATUSES`, `MEDIA_KINDS` (image/video/audio/document/archive/other).
- **Plugin contributions**: views, commands, and activity-bar entries for the Assets app.

## Key exports

- `createAssetsBundle()` — self-registering `PluginBundle` consumed by `installPluginBundles()`.
- `createAssetsRegistry()` — lower-level `AssetsRegistry` for direct access to entity/edge defs, automation presets, and the `PrismPlugin`.
- Constants: `ASSETS_CATEGORIES`, `ASSETS_TYPES`, `ASSETS_EDGES`, `MEDIA_STATUSES`, `CONTENT_STATUSES`, `SCAN_STATUSES`, `MEDIA_KINDS`.
- Types: `AssetsCategory`, `AssetsEntityType`, `AssetsEdgeType`, `AssetsRegistry`.

## Usage

```ts
import { createAssetsBundle } from "@prism/core/plugin-bundles";
import { installPluginBundles } from "@prism/core/plugin-bundles";

const uninstall = installPluginBundles([createAssetsBundle()], {
  objectRegistry,
  pluginRegistry,
});
```
