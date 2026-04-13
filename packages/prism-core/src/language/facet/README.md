# facet/

The Facet System — FileMaker Pro-inspired builder primitives for visual projections, automation sequencing, spell checking, prose serialization, and multi-language codegen. A Facet is a layout-driven view of an entity (form/list/table/report/card) with layout parts, field slots, portals, conditional formatting, value lists, and attached visual scripts.

```ts
import { createFacetDefinition, facetDefinitionBuilder, TypeScriptWriter } from '@prism/core/facet';
```

## Key exports

- `createFacetDefinition(id, objectType, layout)` / `facetDefinitionBuilder(...)` / `FacetDefinitionBuilder` — build `FacetDefinition`s (layout parts, field/portal/text/drawing/container/tab/popover/slide slots, conditional formats, summary fields, print config).
- `createPrintConfig` + `PrintConfig`, `PageOrientation`, `PageSize`, `PageMargins` — print/page geometry.
- `FacetLayout` / `FacetLayoutMode` / `LayoutPartKind` / `LayoutPart` and all slot types — the schema.
- `parseValues` / `serializeValues` / `detectFormat` / `inferFields` / `SourceFormat` — FacetParser (YAML/JSON ↔ typed field records).
- Value Lists: `createStaticValueList`, `createDynamicValueList`, `resolveValueList`, `createValueListRegistry`, plus `ValueList`, `ValueListSource`, `ValueListResolver`, `ValueListRegistry`.
- Spatial layout helpers: `computePartBands`, `snapToGrid`, `alignSlots`, `distributeSlots`, `detectOverlaps`, `slotHitTest`, `partForY`, `clampToBand`, `sortByZIndex`.
- Spell Engine: `SpellChecker`, `SpellCheckerBuilder`, `spellCheckerBuilder`, `SpellCheckRegistry`, `PersonalDictionary`, `MemoryDictionaryStorage`, `extractWords`, dictionary providers (`createUrlDictionaryProvider` / `createStaticDictionaryProvider` / `createLazyDictionaryProvider`), built-in token filters (`URL_FILTER`, `EMAIL_FILTER`, `ALL_CAPS_FILTER`, `CAMEL_CASE_FILTER`, `ALPHANUMERIC_FILTER`, `FILE_PATH_FILTER`, `INLINE_CODE_FILTER`, `SYNTAX_CODE_FILTER`, `WIKI_LINK_FILTER`, `SINGLE_CHAR_FILTER`), `createDelimiterFilter`, `createSyntaxFilter`, and `MockSpellCheckBackend` for tests.
- Prose codec: `markdownToNodes` / `nodesToMarkdown` + `ProseNode` / `ProseMark`.
- Facet runtime: `evaluateConditionalFormats`, `computeFieldStyle`, `interpolateMergeFields`, `renderTextSlot`, `createCollectionValueListResolver`, `getValueListId`, `getBoundFields`.
- Visual scripts: `STEP_KINDS`, `getStepMeta`, `createStep`, `createVisualScript`, `emitStepsLuau`, `emitStepsLuauWithMap`, `validateSteps`, `getStepCategories` + `ScriptStep` / `VisualScript` / `ScriptStepKind`.
- Sequencer: `emitConditionLuau`, `emitScriptLuau` + `SequencerConditionState` / `SequencerScriptState` and subject/operator/action enums.
- Facet builders: `luauBrowserView`, `luauCollectionRule`, `luauStatsCommand`, `luauMenuItem`, `luauCommand` — Luau codegen helpers for common plugin patterns.
- Language writers: `TypeScriptWriter`, `JavaScriptWriter`, `CSharpWriter`, `LuauWriter`, `JsonWriter`, `YamlWriter`, `TomlWriter` — `SchemaModel` → target source.
- `createFacetStore` / `FacetStore` / `FacetStoreSnapshot` — persistent registry of FacetDefinitions, scripts, and value lists.

## Usage

```ts
import { facetDefinitionBuilder } from '@prism/core/facet';

const facet = facetDefinitionBuilder('contact-form', 'contact', 'form')
  .name('Contact Form')
  .addPart({ kind: 'header' })
  .addField({ fieldPath: 'name', part: 'header', order: 0 })
  .addPortal({
    relationshipId: 'invoiced-to',
    displayFields: ['amount', 'date'],
    part: 'body',
    order: 1,
  })
  .build();
```
