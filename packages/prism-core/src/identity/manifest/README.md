# identity/manifest

`PrismManifest` — the on-disk `.prism.json` definition of a Prism workspace — plus the FileMaker-style access control layer that rides on top of it.

Prism's glossary matters here: a **Vault** is the encrypted local directory (the physical boundary), a **Collection** is a typed CRDT array holding actual data, a **Manifest** is a named set of *weak references* to Collections with shell configuration (it points to data, it never contains data), and the **Shell** renders whatever the manifest references. "Workspace" = manifest + vault + shell, never the data itself. Multiple manifests can reference the same collections with different filters.

```ts
import { defaultManifest, createPrivilegeSet } from "@prism/core/manifest";
```

## Key exports

- `defaultManifest(name, id)` — new `PrismManifest` with Loro storage, `@prism/core` schema module, and `private` visibility.
- `parseManifest(json)` / `serialiseManifest(manifest)` / `validateManifest(manifest)` — JSON round-trip and structural validation returning `ManifestValidationError[]`.
- `addCollection`, `removeCollection`, `updateCollection`, `getCollection` — immutable helpers for the `collections` (weak `CollectionRef`) list.
- `MANIFEST_FILENAME` (`.prism.json`), `MANIFEST_VERSION` (`"1"`).
- `createPrivilegeSet(id, name, options)` — build a `PrivilegeSet` with collection/field/layout/script permissions and optional row-level `recordFilter`.
- `getCollectionPermission`, `getFieldPermission`, `getLayoutPermission`, `getScriptPermission`, `canRead`, `canWrite` — pure evaluation helpers over a `PrivilegeSet`.
- `createPrivilegeEnforcer(privilegeSet)` — runtime `PrivilegeEnforcer` that filters objects by row-level security, redacts hidden fields, and answers `canRead` / `canWrite` / `canEditField` / `canSeeLayout`.
- Types: `PrismManifest`, `StorageConfig` (`loro` | `memory` | `fs`), `SchemaConfig`, `SyncConfig`, `SyncMode`, `CollectionRef`, `ManifestVisibility`, `PrivilegeSet`, `PrivilegeSetOptions`, `RoleAssignment`, `CollectionPermission`, `FieldPermission`, `LayoutPermission`, `ScriptPermission`, `PrivilegeContext`, `PrivilegeEnforcer`, `ManifestValidationError`.

## Usage

```ts
import {
  defaultManifest,
  addCollection,
  serialiseManifest,
} from "@prism/core/manifest";

let manifest = defaultManifest("JJM Productions", "mf-jjm-prod");
manifest = addCollection(manifest, {
  id: "contacts",
  name: "Contacts",
  objectTypes: ["Contact"],
  tags: ["work"],
});

const json = serialiseManifest(manifest); // write to .prism.json
```
