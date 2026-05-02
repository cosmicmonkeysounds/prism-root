# ADR-004: Composition Patterns

**Status:** Accepted  
**Date:** 2026-04-19

## Context

Prism's builder (`prism-builder`) ships 17 built-in components with a
`ComponentRegistry` DI surface, a serializable `BuilderDocument` tree,
Taffy-backed layout, and dual Slint/HTML render walkers. The system
handles static documents well but lacks patterns that creation tools
like Godot, Unity, and Figma provide for reuse, composition, theming,
interaction, and augmentation.

## Decision

Introduce five composition systems into `prism-builder`. All
five are data-model-first: they extend `Node` and `BuilderDocument`
with serde-serializable fields, integrate into the existing render
walkers, and keep the `Component` trait object-safe.

### 1. Resources

Typed, shareable data objects referenced by ID rather than inlined.
Inspired by Unity `ScriptableObject` / Godot `Resource`.

- `ResourceDef { id, kind, label, description, data }` stored on
  `BuilderDocument::resources`.
- `ResourceKind` enum: `StylePreset`, `ColorPalette`, `TypographyScale`,
  `AnimationCurve`, `DataSource`, `MediaAsset`, `IconSet`.
- Node props reference resources via `{ "$ref": "resource:<id>" }`.
- The render walker resolves references before passing props to the
  component, so components are unaware of the indirection.

### 2. Modifiers

Attachable behavior descriptors on any node — cross-cutting concerns
that don't change a node's component type. Inspired by Unity
`AddComponent` / Godot composition nodes.

- `Modifier { kind, props }` stored on `Node::modifiers`.
- `ModifierKind` enum: `ScrollOverflow`, `HoverEffect`,
  `EnterAnimation`, `ResponsiveVisibility`, `Tooltip`,
  `AccessibilityOverride`.
- Each kind has a schema (`modifier_schema(kind)`) for the property
  panel.
- The render walker chains modifiers as nested wrappers around the
  component's output (e.g., `ScrollOverflow` emits `Flickable {}` in
  Slint, `<div style="overflow:auto">` in HTML).

### 3. Variants

Named bundles of property overrides on a component, selected per
axis. Inspired by Figma Variants / Unity Prefab Variants.

- `VariantAxis { key, label, options }` declared by components via a
  new `Component::variants()` trait method (default empty).
- `VariantOption { value, label, overrides }` — each option carries a
  `Value` map of props to merge.
- The render walker applies variant resolution: base props -> variant
  overrides -> instance overrides, before calling `render_slint`.

### 4. Signals & Connections

User-authorable event wiring between nodes. Inspired by Godot signals.

- `SignalDef { name, description, payload }` declared by components
  via a new `Component::signals()` trait method (default empty).
- `Connection { id, source_node, signal, target_node, action, params }`
  stored on `BuilderDocument::connections`.
- `ActionKind` enum: `SetProperty`, `ToggleVisibility`, `NavigateTo`,
  `PlayAnimation`, `EmitSignal`, `Custom`.
- Built-in components declare their signals: `button` emits `clicked`,
  `form` emits `submitted`, `input` emits `changed`/`focused`/`blurred`,
  etc.

### 5. Prefabs (Compound Components) → Inline Templates + Component Promotion

User-authored component templates made from existing node subtrees.
Inspired by Unity Prefabs / Godot Scenes-as-Resources.

- `PrefabDef { id, label, description, root, exposed, variants,
  thumbnail }` stored on `BuilderDocument::prefabs`.
- `ExposedSlot { key, target_node, target_prop, spec }` — pins an
  inner node's prop as an instance-editable field.
- `PrefabComponent` wraps a `PrefabDef` and implements `Component`.
  At render time it clones the inner subtree, applies instance
  overrides onto exposed slot targets, then recurses through the
  walker.
- Registered into `ComponentRegistry` like any built-in — once
  registered, prefab instances are indistinguishable from built-in
  component nodes.

**Evolution (see `docs/dev/data-template-system.md`):** The prefab
system remains as internal storage, but the user-facing model is
shifting. Facets no longer require a separate prefab as a mandatory
intermediary. Instead:

- **Inline templates** (`FacetTemplate::Inline`) let a facet own its
  item template directly as a `Node` subtree, editable in-place on the
  builder canvas. Bindings use `{{field}}` expressions in node props
  instead of the three-layer `FacetBinding` → `ExposedSlot` → prop chain.
- **Component promotion** ("Save as Component") extracts an inline
  template into a registered component when reuse is needed. The facet
  switches to `FacetTemplate::ComponentRef`.
- **Scalar output** (`FacetOutput::Scalar`) lets non-list facets
  (Aggregate, single-result Lookup/Script) bind a computed value
  directly to a target widget prop without any template.

`PrefabDef` and `ExposedSlot` remain as the implementation mechanism
for promoted components — the user just never thinks in those terms.

## Consequences

- `Node` gains a `modifiers: Vec<Modifier>` field (serde-default
  empty, backward-compatible with existing documents).
- `BuilderDocument` gains `resources`, `connections`, and `prefabs`
  fields (all serde-default empty, backward-compatible).
- `Component` trait gains `signals()` and `variants()` methods with
  default empty implementations (no breaking change for existing
  impls).
- `HtmlBlock` trait gains matching `signals()` and `variants()`.
- Render walkers (`render_child` on both contexts) gain
  resource-resolution, variant-application, and modifier-wrapping
  steps, transparent to component implementations.
- Five new modules in `prism-builder`: `resource`, `modifier`,
  `variant`, `signal`, `prefab`.
