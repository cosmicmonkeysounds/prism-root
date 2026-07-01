//! Loom v3 AST (spec §6).
//!
//! A `.loom` file lowers to a `LoomFile` carrying a header zone plus a
//! flat list of top-level `items` (declarations, `let` bindings, beats).
//!
//! Rust enums become discriminated unions tagged with `kind`. Rust
//! `IndexMap<String, V>` becomes a JS `Map<string, V>` (insertion order
//! preserved). `Option<T>` becomes `T | null`.

import type { Span } from "./source.ts";

// ---------------------------------------------------------------------
// File / header
// ---------------------------------------------------------------------

export interface LoomFile {
  header: Header;
  items: Item[];
}

export interface Header {
  /** The `# Title` line, if present. */
  title: string | null;
  /** `key: value` properties in source order. */
  properties: Map<string, PropertyValue>;
  span: Span;
}

export interface PropertyValue {
  value: string;
  span: Span;
}

/** Top-level item in a file. Declarations and beats interleave freely. */
export type Item =
  | { kind: "declaration"; value: Declaration }
  | { kind: "letBinding"; value: LetBinding }
  | { kind: "beat"; value: Beat };

// ---------------------------------------------------------------------
// Declarations
// ---------------------------------------------------------------------

export interface Declaration {
  kind: DeclarationKind;
  name: string;
  /** `is X, Y, Z` mixin clause on the opener line. Empty if absent. */
  mixin: string[];
  /** Raw indented body lines. */
  body: RawLine[];
  character: CharacterBody | null;
  stats: StatsBody | null;
  tree: TreeBody | null;
  scene: SceneBody | null;
  generator: GeneratorBody | null;
  cohort: CohortBody | null;
  location: LocationBody | null;
  item: ItemBody | null;
  faction: FactionBody | null;
  person: PersonBody | null;
  roster: RosterBody | null;
  space: SpaceBody | null;
  channel: ChannelBody | null;
  span: Span;
}

/** Fresh `Declaration` with every structured body slot null. */
export function emptyDeclaration(
  kind: DeclarationKind,
  name: string,
  mixin: string[],
  body: RawLine[],
  span: Span,
): Declaration {
  return {
    kind,
    name,
    mixin,
    body,
    character: null,
    stats: null,
    tree: null,
    scene: null,
    generator: null,
    cohort: null,
    location: null,
    item: null,
    faction: null,
    person: null,
    roster: null,
    space: null,
    channel: null,
    span,
  };
}

export type DeclarationKind =
  | "character"
  | "role"
  | "trait"
  | "item"
  | "location"
  | "faction"
  | "stats"
  | "tree"
  | "generator"
  | "scene"
  | "cohort"
  | "person"
  | "roster"
  | "space"
  | "channel";

const DECLARATION_KEYWORDS: ReadonlyArray<[string, DeclarationKind]> = [
  ["CHARACTER", "character"],
  ["ROLE", "role"],
  ["TRAIT", "trait"],
  ["ITEM", "item"],
  ["LOCATION", "location"],
  ["FACTION", "faction"],
  ["STATS", "stats"],
  ["TREE", "tree"],
  ["GENERATOR", "generator"],
  ["SCENE", "scene"],
  ["COHORT", "cohort"],
  ["PERSON", "person"],
  ["ROSTER", "roster"],
  ["SPACE", "space"],
  ["CHANNEL", "channel"],
];

const KEYWORD_TO_KIND = new Map<string, DeclarationKind>(DECLARATION_KEYWORDS);
const KIND_TO_KEYWORD = new Map<DeclarationKind, string>(
  DECLARATION_KEYWORDS.map(([word, kind]) => [kind, word]),
);

/** The screaming-snake keyword that appears in source. */
export function declarationKeyword(kind: DeclarationKind): string {
  return KIND_TO_KEYWORD.get(kind)!;
}

export function declarationKindFromKeyword(word: string): DeclarationKind | null {
  return KEYWORD_TO_KIND.get(word) ?? null;
}

/** True for kinds that share the CHARACTER body shape. ROLE is an alias. */
export function isRoleLike(kind: DeclarationKind): boolean {
  return kind === "character" || kind === "role";
}

export function declarationsWithKind(): ReadonlyArray<[string, DeclarationKind]> {
  return DECLARATION_KEYWORDS;
}

// ---------------------------------------------------------------------
// PERSON / ROSTER (spec v3 §13)
// ---------------------------------------------------------------------

export interface PersonBody {
  displayName: string | null;
  pronouns: string | null;
  email: string | null;
  device: string | null;
  contentTolerance: string[];
  accessibility: string[];
  notes: string | null;
  properties: Map<string, PropertyValue>;
}

export function emptyPersonBody(): PersonBody {
  return {
    displayName: null,
    pronouns: null,
    email: null,
    device: null,
    contentTolerance: [],
    accessibility: [],
    notes: null,
    properties: new Map(),
  };
}

export interface RosterBody {
  date: string | null;
  capacity: number | null;
  cast: RosterCastEntry[];
  swings: RosterSwingEntry[];
  cohorts: RosterCohortEntry[];
  locations: RosterLocationEntry[];
  notes: Map<string, PropertyValue>;
  properties: Map<string, PropertyValue>;
}

export function emptyRosterBody(): RosterBody {
  return {
    date: null,
    capacity: null,
    cast: [],
    swings: [],
    cohorts: [],
    locations: [],
    notes: new Map(),
    properties: new Map(),
  };
}

export interface RosterCastEntry {
  role: string;
  assignment: RosterAssignment;
  span: Span;
}

export type RosterAssignment =
  | { kind: "person"; name: string }
  | { kind: "anyOf"; pool: string[] }
  | { kind: "ghost" };

export interface RosterSwingEntry {
  role: string;
  fallbacks: string[];
  span: Span;
}

export interface RosterCohortEntry {
  cohort: string;
  startWith: string[];
  span: Span;
}

export interface RosterLocationEntry {
  location: string;
  startWith: string[];
  span: Span;
}

// ---------------------------------------------------------------------
// Class layer — init / method / constructor call (spec v3 §9.6)
// ---------------------------------------------------------------------

export interface InitDecl {
  params: InitParam[];
  body: RawLine[];
  span: Span;
}

export interface InitParam {
  name: string;
  rawType: string | null;
  default: string | null;
}

export interface MethodDecl {
  name: string;
  params: InitParam[];
  inlineExpr: string | null;
  body: RawLine[];
  span: Span;
}

export interface ConstructorCall {
  class: string;
  args: Map<string, string>;
  span: Span;
}

// ---------------------------------------------------------------------
// ITEM / FACTION (spec §9) + typed slots (spec §8)
// ---------------------------------------------------------------------

export interface ItemBody {
  inherits: string[];
  properties: Property[];
}

export interface FactionBody {
  inherits: string[];
  properties: Property[];
}

export interface Property {
  name: string;
  slotType: SlotType | null;
  default: string | null;
  rawType: string | null;
  span: Span;
}

export type SlotType =
  | { kind: "any" }
  | { kind: "anyOf"; name: string }
  | { kind: "range"; lo: number; hi: number; default: number | null }
  | { kind: "concrete"; name: string }
  | { kind: "sum"; variants: string[] }
  | { kind: "optional"; inner: SlotType }
  | { kind: "listOf"; inner: SlotType }
  | { kind: "mapOf"; key: SlotType; value: SlotType };

/** Required-slot check (spec §8). */
export function slotTypeIsRequiredHole(slot: SlotType, hasDefault: boolean): boolean {
  if (hasDefault) return false;
  switch (slot.kind) {
    case "any":
    case "anyOf":
      return true;
    case "range":
      return slot.default === null;
    default:
      return false;
  }
}

// ---------------------------------------------------------------------
// COHORT / LOCATION (spec §13.1)
// ---------------------------------------------------------------------

export interface CohortBody {
  label: string | null;
  capacity: number | null;
  properties: Map<string, PropertyValue>;
}

export function emptyCohortBody(): CohortBody {
  return { label: null, capacity: null, properties: new Map() };
}

export interface LocationBody {
  label: string | null;
  ambient: string | null;
  contains: string[];
  capacity: number | null;
  properties: Map<string, PropertyValue>;
}

export function emptyLocationBody(): LocationBody {
  return { label: null, ambient: null, contains: [], capacity: null, properties: new Map() };
}

// ---------------------------------------------------------------------
// SPACE / CHANNEL — authored chatrooms (Discord spaces + Slack threads)
// ---------------------------------------------------------------------

/** A channel's access/behaviour kind. `open` = anyone; `private` = invite
 *  only; `faction` = scoped to a faction's members; `group` / `dm` = a fixed
 *  declared member set. */
export type ChannelKindWord = "open" | "private" | "faction" | "group" | "dm";

export interface ChannelBody {
  /** The channel's id (the `CHANNEL <name>` opener). */
  name: string;
  label: string | null;
  /** open | private | faction | group | dm — defaults to `open`. */
  kind: ChannelKindWord;
  /** Owning space id (set from the enclosing SPACE or a `space:` property). */
  space: string | null;
  /** For `kind: faction` — the faction whose members this channel serves. */
  faction: string | null;
  /** Declared initial members (ids / character names) for group/private/dm. */
  members: string[];
  /** Who may invite into a private channel (`members` | `anyone` | a role). */
  invite: string | null;
  // --- channel-type rules (resolved by the registry at compile time) ------
  /** A registered channel-type preset name (`announcement`, …). */
  type: string | null;
  /** Post policy override (`everyone` | `members` | `faction` | `none` | `role X`). */
  post: string | null;
  /** Threading override (`on` | `off`). */
  threads: string | null;
  /** Broadcast cues mirrored here (`*` or a comma list). */
  routes: string | null;
  slow: string | null;
  ephemeral: string | null;
  properties: Map<string, PropertyValue>;
  span: Span;
}

export function emptyChannelBody(name: string, span: Span): ChannelBody {
  return {
    name,
    label: null,
    kind: "open",
    space: null,
    faction: null,
    members: [],
    invite: null,
    type: null,
    post: null,
    threads: null,
    routes: null,
    slow: null,
    ephemeral: null,
    properties: new Map(),
    span,
  };
}

export interface SpaceBody {
  label: string | null;
  /** Channels declared nested inside this SPACE, in source order. */
  channels: ChannelBody[];
  properties: Map<string, PropertyValue>;
}

export function emptySpaceBody(): SpaceBody {
  return { label: null, channels: [], properties: new Map() };
}

// ---------------------------------------------------------------------
// Improv directive (spec §13.3)
// ---------------------------------------------------------------------

export interface ImprovDirective {
  duration: ImprovDuration | null;
  quorum: QuorumOp;
  advanceOn: AdvanceSignal[];
  span: Span;
}

export type ImprovDurationUnit = "ms" | "seconds" | "minutes";

export interface ImprovDuration {
  value: number;
  unit: ImprovDurationUnit;
}

/** Convert an improv duration to milliseconds. */
export function improvDurationToMillis(d: ImprovDuration): number {
  const secs =
    d.unit === "ms" ? d.value / 1000 : d.unit === "minutes" ? d.value * 60 : d.value;
  return Math.max(0, secs) * 1000;
}

export type AdvanceSignal =
  | { kind: "pedal" }
  | { kind: "speech"; anchor: string }
  | { kind: "gesture"; name: string };

export type QuorumOp =
  | { kind: "all" }
  | { kind: "any" }
  | { kind: "n"; value: number };

export const QUORUM_ALL: QuorumOp = { kind: "all" };
export const QUORUM_ANY: QuorumOp = { kind: "any" };

// ---------------------------------------------------------------------
// CHARACTER / TRAIT (spec §10)
// ---------------------------------------------------------------------

export interface CharacterBody {
  properties: Map<string, PropertyValue>;
  statsProfile: string | null;
  statsCtor: ConstructorCall | null;
  init: InitDecl | null;
  methods: MethodDecl[];
  disposition: DispositionAxis[];
  reacts: ReactClause[];
  knowledge: KnowledgeField[];
  goals: GoalDecl[];
  hooks: HookDecl[];
  generators: GeneratorDecl[];
  typedProperties: Property[];
}

export function emptyCharacterBody(): CharacterBody {
  return {
    properties: new Map(),
    statsProfile: null,
    statsCtor: null,
    init: null,
    methods: [],
    disposition: [],
    reacts: [],
    knowledge: [],
    goals: [],
    hooks: [],
    generators: [],
    typedProperties: [],
  };
}

export interface DispositionAxis {
  verb: string;
  target: string;
  current: number;
  max: number;
  mirror: string | null;
  span: Span;
}

export interface ReactClause {
  condition: string;
  tag: string;
  span: Span;
}

export interface KnowledgeField {
  name: string;
  typeSpec: string;
  default: string | null;
  span: Span;
}

export interface GoalDecl {
  name: string;
  priority: number | null;
  activeWhen: string | null;
  completesWhen: string | null;
  failsWhen: string | null;
  drives: string | null;
  onComplete: string | null;
  onFail: string | null;
  span: Span;
}

export function emptyGoalDecl(name: string, span: Span): GoalDecl {
  return {
    name,
    priority: null,
    activeWhen: null,
    completesWhen: null,
    failsWhen: null,
    drives: null,
    onComplete: null,
    onFail: null,
    span,
  };
}

export interface HookDecl {
  event: string;
  body: RawLine[];
  suppressed: boolean;
  span: Span;
}

export interface GeneratorDecl {
  name: string;
  tier: string | null;
  priority: number | null;
  body: RawLine[];
  span: Span;
}

export function emptyGeneratorDecl(name: string, span: Span): GeneratorDecl {
  return { name, tier: null, priority: null, body: [], span };
}

// ---------------------------------------------------------------------
// SCENE / GENERATOR (spec §12)
// ---------------------------------------------------------------------

export interface SceneBody {
  params: string[];
  tier: string | null;
  priority: number | null;
  entry: RawLine[];
  states: SceneState[];
}

export function emptySceneBody(params: string[]): SceneBody {
  return { params, tier: null, priority: null, entry: [], states: [] };
}

export interface SceneState {
  name: string;
  body: RawLine[];
  span: Span;
}

export interface GeneratorBody {
  tier: string | null;
  priority: number | null;
  startWhen: string | null;
  body: RawLine[];
}

export function emptyGeneratorBody(): GeneratorBody {
  return { tier: null, priority: null, startWhen: null, body: [] };
}

// ---------------------------------------------------------------------
// STATS / TREE (spec §11)
// ---------------------------------------------------------------------

export interface StatsBody {
  attributes: AttributeDecl[];
  axes: AxisDecl[];
  pools: PoolDecl[];
  stats: StatExprDecl[];
  init: InitDecl | null;
  methods: MethodDecl[];
}

export function emptyStatsBody(): StatsBody {
  return { attributes: [], axes: [], pools: [], stats: [], init: null, methods: [] };
}

export interface AttributeDecl {
  name: string;
  default: number;
  min: number;
  max: number;
  span: Span;
}

export interface AxisDecl {
  name: string;
  mode: string | null;
  curve: string | null;
  onAdvance: string | null;
  milestones: string[];
  span: Span;
}

export function emptyAxisDecl(name: string, span: Span): AxisDecl {
  return { name, mode: null, curve: null, onAdvance: null, milestones: [], span };
}

export interface PoolDecl {
  name: string;
  max: string | null;
  regen: string | null;
  cost: string | null;
  span: Span;
}

export function emptyPoolDecl(name: string, span: Span): PoolDecl {
  return { name, max: null, regen: null, cost: null, span };
}

export interface StatExprDecl {
  name: string;
  expression: string;
  span: Span;
}

export interface TreeBody {
  nodes: TreeNodeDecl[];
}

export interface TreeNodeDecl {
  name: string;
  cost: string | null;
  requires: string | null;
  effects: string[];
  span: Span;
}

export function emptyTreeNodeDecl(name: string, span: Span): TreeNodeDecl {
  return { name, cost: null, requires: null, effects: [], span };
}

// ---------------------------------------------------------------------
// let bindings + beats
// ---------------------------------------------------------------------

export interface LetBinding {
  name: string;
  expression: string;
  span: Span;
}

export interface Beat {
  name: string;
  params: string[];
  contract: Map<string, PropertyValue>;
  body: BodyItem[];
  span: Span;
}

// ---------------------------------------------------------------------
// Beat body items
// ---------------------------------------------------------------------

export type BodyItem =
  | { kind: "sceneHeading"; value: Located<string> }
  | { kind: "action"; value: Located<string> }
  | { kind: "dialogue"; value: DialogueBlock }
  | { kind: "choice"; value: Choice }
  | { kind: "divert"; value: Divert }
  | { kind: "directive"; value: Directive }
  | { kind: "metadata"; value: Located<string> }
  | { kind: "conditional"; value: Conditional }
  | { kind: "match"; value: MatchBlock }
  | { kind: "eachVisit"; value: EachVisit }
  | { kind: "afterMorph"; value: AfterMorph }
  | { kind: "inlineLet"; value: InlineLet }
  | { kind: "directiveBlock"; value: DirectiveBlock }
  | { kind: "slotPlaceholder"; value: SlotPlaceholder };

export interface MatchBlock {
  scrutinee: string;
  arms: MatchArm[];
  span: Span;
}

export interface MatchArm {
  pattern: string;
  body: BodyItem[];
  span: Span;
}

export interface EachVisit {
  first: BodyItem[];
  then: BodyItem[];
  finally: BodyItem[];
  span: Span;
}

export interface AfterMorph {
  condition: string;
  after: BodyItem[];
  otherwise: BodyItem[];
  span: Span;
}

export interface InlineLet {
  name: string;
  expression: string;
  span: Span;
}

export interface SlotPlaceholder {
  name: string;
  span: Span;
}

export interface Conditional {
  arms: ConditionalArm[];
  span: Span;
}

export interface ConditionalArm {
  /** `null` for the trailing `<else>` arm. */
  condition: string | null;
  body: BodyItem[];
  span: Span;
}

export interface DirectiveBlock {
  directive: Directive;
  body: BodyItem[];
  span: Span;
}

export interface DialogueBlock {
  speaker: string;
  speakers: string[];
  parenthetical: string | null;
  improv: ImprovDirective | null;
  /**
   * The speaker's block body — the *same* `BodyItem[]` a beat uses, so every
   * control-flow form (`<if>` / `<match>` / `<each visit>` / `<after>` /
   * diverts / `<let>` / choices / directives) works inside dialogue with no
   * special-casing. A bare text line is `action`; when executed under a
   * speaker the runtime emits it as that speaker's spoken line rather than
   * narration. Coalescing of wrapped prose still happens at scan time.
   */
  body: BodyItem[];
  span: Span;
}

export interface Choice {
  sticky: boolean;
  text: string;
  suppressed: string | null;
  body: BodyItem[];
  span: Span;
}

export type Divert =
  | {
      kind: "to";
      target: DivertTarget;
      params: Map<string, string>;
      slots: Map<string, BodyItem[]>;
      scopeAs: string | null;
      span: Span;
    }
  | { kind: "tunnel"; target: DivertTarget; span: Span }
  | { kind: "return"; span: Span }
  | { kind: "end"; span: Span };

/** The span carried by any divert variant. */
export function divertSpan(d: Divert): Span {
  return d.span;
}

export interface DivertTarget {
  qualifier: string | null;
  name: string;
  knot: string | null;
}

export interface Directive {
  raw: string;
  span: Span;
}

export interface Located<T> {
  value: T;
  span: Span;
}

export interface RawLine {
  indent: number;
  text: string;
  span: Span;
}
