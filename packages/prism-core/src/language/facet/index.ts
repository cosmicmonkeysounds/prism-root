// ── Facet System ──────────────────────────────────────────────────────────
// FileMaker Pro-inspired builder primitives: visual projections, automation
// sequencing, spell checking, prose serialization, and multi-language codegen.

// ── Facet Parser (YAML/JSON ↔ typed field records) ───────────────────────
export {
  detectFormat,
  parseValues,
  serializeValues,
  inferFields,
} from "./facet-parser.js";
export type { SourceFormat } from "./facet-parser.js";

// ── Facet Schema (layout parts, field/portal slots, definitions) ─────────
export type {
  FacetLayout,
  FacetLayoutMode,
  LayoutPartKind,
  LayoutPart,
  SpatialRect,
  ConditionalFormat,
  FieldSlot,
  PortalSlot,
  TextSlot,
  DrawingShape,
  DrawingSlot,
  ContainerSlot,
  TabGroup,
  TabSlot,
  PopoverSlot,
  SlideSlot,
  FacetSlot,
  SummaryField,
  PageOrientation,
  PageSize,
  PageMargins,
  PrintConfig,
  FacetDefinition,
} from "./facet-schema.js";
export {
  createFacetDefinition,
  createPrintConfig,
  FacetDefinitionBuilder,
  facetDefinitionBuilder,
} from "./facet-schema.js";

// ── Value Lists (static + dynamic constrained field input) ──────────────
export type {
  ValueListItem,
  StaticValueListSource,
  DynamicValueListSource,
  ValueListSource,
  ValueListDisplay,
  ValueList,
  ValueListResolver,
  ValueListListener,
  ValueListRegistry,
} from "./value-list.js";
export {
  createStaticValueList,
  createDynamicValueList,
  resolveValueList,
  createValueListRegistry,
} from "./value-list.js";

// ── Spatial Layout (pure geometry helpers for free-form layouts) ─────────
export type {
  ComputedBand,
  Alignment,
} from "./spatial-layout.js";
export {
  computePartBands,
  snapToGrid,
  alignSlots,
  distributeSlots,
  detectOverlaps,
  slotHitTest,
  partForY,
  clampToBand,
  sortByZIndex,
} from "./spatial-layout.js";

// ── Spell Engine ─────────────────────────────────────────────────────────
export type {
  DictionaryData,
  DictionaryProvider,
  PersonalDictionaryStorage,
  ExtractedWord,
  SpellCheckBackend,
  SpellCheckerConfig,
  SpellCheckEvent,
  SpellCheckEventListener,
} from "./spell-engine.js";
export {
  SpellCheckRegistry,
  SpellChecker,
  extractWords,
  PersonalDictionary,
  MemoryDictionaryStorage,
  SpellCheckerBuilder,
  spellCheckerBuilder,
  createUrlDictionaryProvider,
  createStaticDictionaryProvider,
  createLazyDictionaryProvider,
  URL_FILTER,
  EMAIL_FILTER,
  ALL_CAPS_FILTER,
  CAMEL_CASE_FILTER,
  ALPHANUMERIC_FILTER,
  FILE_PATH_FILTER,
  INLINE_CODE_FILTER,
  SYNTAX_CODE_FILTER,
  WIKI_LINK_FILTER,
  SINGLE_CHAR_FILTER,
  createDelimiterFilter,
  createSyntaxFilter,
  MockSpellCheckBackend,
} from "./spell-engine.js";
export type { MockSpellCheckConfig } from "./spell-engine.js";

// ── Prose Codec (Markdown ↔ structured nodes) ───────────────────────────
export type { ProseNode, ProseMark } from "./prose-codec.js";
export { markdownToNodes, nodesToMarkdown } from "./prose-codec.js";

// ── Facet Runtime (conditional formatting, merge fields, value list resolver) ──
export type {
  ComputedStyle,
  ValueListDataSource,
} from "./facet-runtime.js";
export {
  evaluateConditionalFormats,
  computeFieldStyle,
  interpolateMergeFields,
  renderTextSlot,
  createCollectionValueListResolver,
  getValueListId,
  getBoundFields,
} from "./facet-runtime.js";

// ── Script Steps (FileMaker-style visual scripting → Luau) ────────────────
export type {
  ScriptStepKind,
  StepKindMeta,
  ScriptStep,
  VisualScript,
  StepsLuauEmitResult,
} from "./script-steps.js";
export {
  STEP_KINDS,
  getStepMeta,
  createStep,
  createVisualScript,
  emitStepsLuau,
  emitStepsLuauWithMap,
  validateSteps,
  getStepCategories,
} from "./script-steps.js";

// ── Facet ↔ GraphObject adapter (unified registry bridge) ───────────────
export type { FacetObjectLike } from "./facet-object-adapter.js";
export {
  FACET_DEF_TYPE,
  facetDefFromObject,
  objectPatchFromFacetDef,
  isFacetDefObject,
} from "./facet-object-adapter.js";

// ── Sequencer (visual condition/script builder → Luau) ────────────────────
export type {
  SequencerSubjectKind,
  SequencerSubject,
  SequencerOperator,
  SequencerCombinator,
  SequencerConditionClause,
  SequencerConditionState,
  SequencerActionKind,
  SequencerScriptStep,
  SequencerScriptState,
} from "./sequencer-types.js";
export { emitConditionLuau, emitScriptLuau } from "./sequencer-types.js";

// ── Facet Builders (Luau codegen helpers for plugin patterns) ───────────
export type {
  BrowserViewColumn,
  BrowserViewConfig,
  CollectionRuleConfig,
  StatsOperation,
  StatsFieldConfig,
  StatsCommandConfig,
  MenuItemConfig,
  CommandConfig,
} from "./facet-builders.js";
export {
  luauBrowserView,
  luauCollectionRule,
  luauStatsCommand,
  luauMenuItem,
  luauCommand,
} from "./facet-builders.js";

// ── Emitters (SchemaModel → TS/JS/C#/Luau/JSON/YAML/TOML) ────────────────
export type {
  SchemaField,
  SchemaInterface,
  SchemaEnum,
  SchemaDeclaration,
  SchemaModel,
} from "./emitters.js";
export {
  TypeScriptWriter,
  JavaScriptWriter,
  CSharpWriter,
  LuauWriter,
  JsonWriter,
  YamlWriter,
  TomlWriter,
} from "./emitters.js";
