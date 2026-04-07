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
  LayoutPartKind,
  LayoutPart,
  FieldSlot,
  PortalSlot,
  FacetSlot,
  SummaryField,
  FacetDefinition,
} from "./facet-schema.js";
export {
  createFacetDefinition,
  FacetDefinitionBuilder,
  facetDefinitionBuilder,
} from "./facet-schema.js";

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

// ── Sequencer (visual condition/script builder → Lua) ────────────────────
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
export { emitConditionLua, emitScriptLua } from "./sequencer-types.js";

// ── Facet Builders (Lua codegen helpers for plugin patterns) ────────────
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
  luaBrowserView,
  luaCollectionRule,
  luaStatsCommand,
  luaMenuItem,
  luaCommand,
} from "./facet-builders.js";

// ── Emitters (SchemaModel → TS/JS/C#/Lua/JSON/YAML/TOML) ────────────────
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
  LuaWriter,
  JsonWriter,
  YamlWriter,
  TomlWriter,
} from "./emitters.js";
