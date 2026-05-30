//! Loom v3 AST (spec §6).
//!
//! A `.loom` file lowers to a [`LoomFile`] carrying three zones:
//!
//! * `header`       — `# Title` + `key: value` properties
//! * `declarations` — `CHARACTER`, `TRAIT`, `ITEM`, `LOCATION`,
//!   `FACTION`, `STATS`, `TREE`, `GENERATOR`, `SCENE`, `COHORT`
//! * `beats`        — `== knot_name` with a contract + woven body
//!
//! Plus top-level `let` bindings, which can appear anywhere in the
//! declaration zone (spec §12.1).
//!
//! Phase-2 invariant: declaration bodies and `<…>` directives are
//! captured *opaquely* (raw text + span). Later phases lower them
//! into structured sub-ASTs without changing the surrounding shape.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::source::Span;

/// One parsed `.loom` file. Names are flat across the project; the
/// runtime's resolver indexes every `LoomFile` and answers
/// cross-file lookups.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LoomFile {
    pub header: Header,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Header {
    /// The `# Title` line, if present.
    pub title: Option<String>,
    /// `key: value` properties in source order (`entry`, `tags`, …).
    pub properties: IndexMap<String, PropertyValue>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyValue {
    pub value: String,
    pub span: Span,
}

/// Top-level item in a file. Declarations and beats can interleave
/// freely (spec §3: "any declaration may live in any file").
///
/// `Declaration` carries the structured Simulacra / Meridian bodies
/// inline (spec §10, §11) so the enum's footprint is dominated by it;
/// boxing would force every consumer to thread `&Declaration` indirection
/// for no real win. Top-level items are heap-allocated in a `Vec` so the
/// per-variant size delta does not appear in stack frames.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum Item {
    Declaration(Declaration),
    LetBinding(LetBinding),
    Beat(Beat),
}

/// A declaration block — `CHARACTER`, `TRAIT`, `ITEM`, `LOCATION`,
/// `FACTION`, `STATS`, `TREE`, `GENERATOR`, `SCENE`, `COHORT`.
///
/// The raw body is always kept (for inheritance / unknown-kind
/// passthrough). Structured sub-ASTs are attached for kinds the
/// parser knows how to lower (CHARACTER / TRAIT → [`CharacterBody`];
/// STATS → [`StatsBody`]; TREE → [`TreeBody`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub name: String,
    /// `is X, Y, Z` mixin clause on the opener line. Empty if absent.
    pub mixin: Vec<String>,
    /// Raw indented body lines, with their leading whitespace
    /// preserved relative to the opener's column.
    pub body: Vec<RawLine>,
    /// Structured body for CHARACTER and TRAIT (spec §10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<CharacterBody>,
    /// Structured body for STATS (spec §11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<StatsBody>,
    /// Structured body for TREE (spec §11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeBody>,
    /// Structured body for top-level SCENE (spec §12.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<SceneBody>,
    /// Structured body for top-level GENERATOR (spec §12.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<GeneratorBody>,
    /// Structured body for COHORT (spec §13.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cohort: Option<CohortBody>,
    /// Structured body for LOCATION (spec §13.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationBody>,
    /// Structured body for ITEM (spec §9). An ITEM is a typed
    /// composable kind that participates in `is` inheritance just
    /// like CHARACTER / TRAIT (spec §9.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemBody>,
    /// Structured body for FACTION (spec §9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<FactionBody>,
    /// Structured body for PERSON (spec v3 §13.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<PersonBody>,
    /// Structured body for ROSTER (spec v3 §13.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roster: Option<RosterBody>,
    pub span: Span,
}

/// Structured PERSON body (spec v3 §13.2). A PERSON is a real human
/// (or AI agent) — display name, pronouns, device, accessibility,
/// content tolerances. Persons survive individual shows.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PersonBody {
    pub display_name: Option<String>,
    pub pronouns: Option<String>,
    pub email: Option<String>,
    pub device: Option<String>,
    /// `content_tolerance: [no_strobe, no_loud_bells]` — comma-list of bare names.
    pub content_tolerance: Vec<String>,
    /// `accessibility: [step_free]`.
    pub accessibility: Vec<String>,
    pub notes: Option<String>,
    /// All `key: value` lines in source order for round-trip.
    pub properties: IndexMap<String, PropertyValue>,
}

/// Structured ROSTER body (spec v3 §13.3). The lineup for a specific
/// run — who is expected, in what role, in what cohort, with what
/// swing chain.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RosterBody {
    /// ISO-ish date string (the parser does not enforce a calendar).
    pub date: Option<String>,
    /// Soft cap on expected attendees.
    pub capacity: Option<u32>,
    /// `cast` block — role → assignment (`jamie_lee`, `any of [...]`).
    pub cast: Vec<RosterCastEntry>,
    /// `swings` block — role → priority-ordered list of fallback persons.
    pub swings: Vec<RosterSwingEntry>,
    /// `cohorts` block — cohort → `start with: [person, …]`.
    pub cohorts: Vec<RosterCohortEntry>,
    /// `locations` block — location → `start with: [person, …]`.
    pub locations: Vec<RosterLocationEntry>,
    /// `notes` block — free-form `key: value` lines.
    pub notes: IndexMap<String, PropertyValue>,
    /// Every `key: value` on the opener level, for round-trip.
    pub properties: IndexMap<String, PropertyValue>,
}

/// One row of a ROSTER `cast` block.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RosterCastEntry {
    pub role: String,
    pub assignment: RosterAssignment,
    pub span: Span,
}

/// A ROSTER `cast`-block assignment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RosterAssignment {
    /// `Wren := jamie_lee` — concrete binding to a named PERSON.
    Person(String),
    /// `Initiate := any of [audience]` — a pool of acceptable
    /// PERSONs. The synthetic name `audience` matches any walk-up.
    AnyOf(Vec<String>),
    /// Empty / `none` — slot starts as a ghost (no player bound).
    Ghost,
}

/// One row of a ROSTER `swings` block.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RosterSwingEntry {
    pub role: String,
    /// Priority-ordered fallback PERSON names.
    pub fallbacks: Vec<String>,
    pub span: Span,
}

/// One row of a ROSTER `cohorts` block.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RosterCohortEntry {
    pub cohort: String,
    pub start_with: Vec<String>,
    pub span: Span,
}

/// One row of a ROSTER `locations` block.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RosterLocationEntry {
    pub location: String,
    pub start_with: Vec<String>,
    pub span: Span,
}

/// `init(args)` constructor body — recognised inside every class-like
/// declaration (CHARACTER / ROLE / TRAIT / STATS / ITEM / FACTION /
/// PERSON / ROSTER / COHORT / LOCATION). Spec v3 §9.6.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InitDecl {
    /// Parameter names in declaration order. The default-value tail
    /// (`= none`) is preserved on each entry.
    pub params: Vec<InitParam>,
    pub body: Vec<RawLine>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InitParam {
    pub name: String,
    /// Raw type spelling (`int?`, `string`, …), if given.
    pub raw_type: Option<String>,
    /// Raw default expression text (`none`, `0`, …), if given.
    pub default: Option<String>,
}

/// `method name(args)` — a callable on the instance. Spec v3 §9.6.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MethodDecl {
    pub name: String,
    pub params: Vec<InitParam>,
    /// Inline `method exhausted? = self.health < ...` form lifts the
    /// right-hand expression here. When set, `body` is empty.
    pub inline_expr: Option<String>,
    pub body: Vec<RawLine>,
    pub span: Span,
}

/// A constructor call in a property-value position — `Combat(strength:
/// 12, agility: 14)`. Used as the structured form of a `stats:` slot
/// or any other typed slot that holds an embedded class instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstructorCall {
    /// Class name on the call — `Combat`, `LootBag`, …
    pub class: String,
    /// Named args in source order.
    pub args: IndexMap<String, String>,
    pub span: Span,
}

/// Structured ITEM body (spec §9). ITEMs are kinds with
/// inheritance; their bodies are typed property lists.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ItemBody {
    /// `is X, Y` parents — same shape as CHARACTER mixins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inherits: Vec<String>,
    pub properties: Vec<Property>,
}

/// Structured FACTION body (spec §9). Same shape as [`ItemBody`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FactionBody {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inherits: Vec<String>,
    pub properties: Vec<Property>,
}

/// A typed property — `name: <slot-type> [= default]` — as it
/// appears inside ITEM / FACTION / CHARACTER / TRAIT bodies
/// (spec §8).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Property {
    pub name: String,
    /// Structured type, when the parser recognises the shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_type: Option<SlotType>,
    /// Raw default expression text, parsed lazily by the runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Raw type spelling — kept so unknown shapes still round-trip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_type: Option<String>,
    pub span: Span,
}

/// Typed-slot grammar (spec §8). `Any` / `AnyOf` / `Range` mark
/// required holes; `Optional` / `ListOf` / `Concrete` / `Sum`
/// resolve to a value.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SlotType {
    /// `any` — required, free-form.
    Any,
    /// `any of LOCATION` — required, narrowed to a kind.
    AnyOf(String),
    /// `range LO to HI [ = N ]`. Also accepted without the leading
    /// `range` keyword: `0 to 100 = 50`.
    Range {
        lo: f64,
        hi: f64,
        default: Option<f64>,
    },
    /// Bare type name (`LootBag`, `int`, `text`).
    Concrete(String),
    /// `unknown | suspects | confirmed` — closed enumeration.
    Sum(Vec<String>),
    /// `text?` — the inner type with an implicit null.
    Optional(Box<SlotType>),
    /// `list of RUMOUR`.
    ListOf(Box<SlotType>),
    /// `map of KEY to VALUE`.
    MapOf {
        key: Box<SlotType>,
        value: Box<SlotType>,
    },
}

impl SlotType {
    /// Required-slot check (spec §8). A slot is *required* when it
    /// names an unfilled `any`-shaped hole.
    pub fn is_required_hole(&self, has_default: bool) -> bool {
        if has_default {
            return false;
        }
        match self {
            Self::Any | Self::AnyOf(_) => true,
            Self::Range { default, .. } => default.is_none(),
            Self::Optional(_) => false,
            _ => false,
        }
    }
}

/// Structured COHORT body (spec §13.1). A cohort is a named group
/// of participants — `Initiates`, `Singers`, … — that broadcast
/// scopes and enroll directives target.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CohortBody {
    /// `label:` — programme / booth-facing display name.
    pub label: Option<String>,
    /// Soft cap on enrolled participants; `None` is unbounded.
    pub capacity: Option<u32>,
    /// All `key: value` properties in source order, including the
    /// recognised `label:` / `capacity:` keys (kept for round-trip).
    pub properties: IndexMap<String, PropertyValue>,
}

/// Structured LOCATION body (spec §13.1). A location is a named
/// place the show treats as first-class: ambient cues, capacity,
/// nesting (`contains:`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocationBody {
    pub label: Option<String>,
    /// `ambient: bell-loop` — ambient cue tag bound to this place.
    pub ambient: Option<String>,
    /// `contains: BellTower, Nave` — child locations (parsed as a
    /// comma-separated list of bare names).
    pub contains: Vec<String>,
    /// `capacity:` — soft cap on co-located participants.
    pub capacity: Option<u32>,
    pub properties: IndexMap<String, PropertyValue>,
}

/// One `(improv duration: 45s, advance on: any [pedal, …])`
/// parenthetical attached to a dialogue cue (spec §13.3).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImprovDirective {
    /// Cap on the beat's duration. `None` means "wait for a signal
    /// indefinitely" — practical only with `quorum: all`.
    pub duration: Option<ImprovDuration>,
    /// Quorum semantics — `all`, `any` (default), or `quorum(N)`.
    pub quorum: QuorumOp,
    /// Signals that may advance the beat.
    pub advance_on: Vec<AdvanceSignal>,
    pub span: Span,
}

/// Numeric duration `45s` / `200ms` / `2m` (spec §13.3).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ImprovDuration {
    pub value: f64,
    pub unit: ImprovDurationUnit,
}

impl ImprovDuration {
    /// Convert to a wall-clock `Duration`.
    pub fn to_std(self) -> std::time::Duration {
        let secs = match self.unit {
            ImprovDurationUnit::Ms => self.value / 1000.0,
            ImprovDurationUnit::Seconds => self.value,
            ImprovDurationUnit::Minutes => self.value * 60.0,
        };
        std::time::Duration::from_secs_f64(secs.max(0.0))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImprovDurationUnit {
    Ms,
    #[default]
    Seconds,
    Minutes,
}

/// One signal that can advance an improv beat (spec §13.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdvanceSignal {
    /// `pedal` — stage pedal / button press.
    Pedal,
    /// `speech(anchor phrase)` — speech-recognition keyword.
    Speech { anchor: String },
    /// `gesture(Bow)` — named gesture recogniser.
    Gesture { name: String },
}

/// `all` / `any` / `quorum(N)` advance semantics (spec §13.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuorumOp {
    /// Every signal in `advance_on` must arrive before the beat
    /// advances.
    All,
    /// The first matching signal wins (default).
    #[default]
    Any,
    /// `quorum(N)` — N matching signals required.
    N(u32),
}

/// Structured CHARACTER / TRAIT body (spec §10).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CharacterBody {
    /// Free-form property lines: `voice: female_alto`, `hp: 80`,
    /// `loot: goblin_pouch`, including the special `stats: Combat`
    /// reference that is also surfaced as [`Self::stats_profile`].
    pub properties: IndexMap<String, PropertyValue>,
    /// `stats: Combat` — name of the STATS profile this character
    /// instantiates. Sugar over `properties["stats"]`.
    pub stats_profile: Option<String>,
    /// Structured constructor call form — `stats: Combat(strength: 12)`.
    /// When present, `stats_profile` carries the class name and this
    /// holds the named arguments (spec v3 §9.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats_ctor: Option<ConstructorCall>,
    /// `init(args)` constructor body, when declared (spec v3 §9.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub init: Option<InitDecl>,
    /// `method name(args)` callables on the instance (spec v3 §9.6).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<MethodDecl>,
    /// `trusts X: N of M [mirror …]` etc.
    pub disposition: Vec<DispositionAxis>,
    /// `reacts <cond> → <tag>`.
    pub reacts: Vec<ReactClause>,
    /// `knows:` block, one entry per typed slot.
    pub knowledge: Vec<KnowledgeField>,
    /// `goal name` blocks.
    pub goals: Vec<GoalDecl>,
    /// `on <event>` hook blocks.
    pub hooks: Vec<HookDecl>,
    /// `generator name` blocks declared inside the character.
    pub generators: Vec<GeneratorDecl>,
    /// Typed-slot view of [`Self::properties`] (spec §8). Each
    /// entry mirrors a `key: value` line from the body but
    /// carries the structured [`SlotType`] when the parser
    /// recognised the right-hand spelling. Required (`any` /
    /// `any of …` / `range … to …`) slots without an inherited
    /// fill mark the character as abstract (spec §8 + §9).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub typed_properties: Vec<Property>,
}

/// `trusts Player: 30 of 100 [mirror Player.trusts.Wren]` (spec §10.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispositionAxis {
    /// `trusts` / `respects` / `fears` — the verb token.
    pub verb: String,
    /// The target character name.
    pub target: String,
    pub current: f64,
    pub max: f64,
    pub mirror: Option<String>,
    pub span: Span,
}

/// `reacts trust > 60 → warm` (spec §10.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReactClause {
    pub condition: String,
    pub tag: String,
    pub span: Span,
}

/// One row of a `knows:` block (spec §10.2).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnowledgeField {
    pub name: String,
    /// Raw type spelling — `bool`, `text?`, `unknown | suspects | confirmed`.
    pub type_spec: String,
    /// Raw default expression text, if `= …` was given.
    pub default: Option<String>,
    pub span: Span,
}

/// A `goal name` block (spec §10.3).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GoalDecl {
    pub name: String,
    pub priority: Option<f64>,
    pub active_when: Option<String>,
    pub completes_when: Option<String>,
    pub fails_when: Option<String>,
    pub drives: Option<String>,
    pub on_complete: Option<String>,
    pub on_fail: Option<String>,
    pub span: Span,
}

/// An `on <event>` hook (spec §10.4).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookDecl {
    /// The leading event clause, normalised — whitespace collapsed,
    /// `on ` prefix stripped. Examples: `meeting Player`,
    /// `trust passes 80`, `cue bell_strike_loud`, `Time.hour == 22`,
    /// `Participant enters Lighthouse`, `event bell_acknowledged`.
    pub event: String,
    /// Indented body — raw lines preserved so playhead / scheduler
    /// can lower them later (typically diverts + directives).
    pub body: Vec<RawLine>,
    /// `on <event>: none` suppression marker (spec §9.5). When a
    /// child declares this form, the bundle's inheritance merge
    /// drops any inherited hook whose event clause matches.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
    pub span: Span,
}

/// A `generator name` block — character-bound or top-level (spec §10.5, §12.4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GeneratorDecl {
    pub name: String,
    /// `tier: ambient | active | focal`.
    pub tier: Option<String>,
    pub priority: Option<f64>,
    /// Raw indented body lines (the at/every/when/loop machinery is
    /// lowered by the scheduler at runtime).
    pub body: Vec<RawLine>,
    pub span: Span,
}

/// Structured top-level SCENE body (spec §12.3).
///
/// A SCENE is a multi-step labelled coroutine. The default
/// (unnamed) state collects body items that appear before any
/// labelled sub-state opener; `states` carries each labelled state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SceneBody {
    /// Parameter list from the opener line, e.g.
    /// `SCENE patrol(character, route)` → `["character", "route"]`.
    pub params: Vec<String>,
    /// `tier: focal | active | ambient` (spec §12.5).
    pub tier: Option<String>,
    /// `priority: <number>` (spec §12.5).
    pub priority: Option<f64>,
    /// Body items that appear before any labelled state opener —
    /// the implicit entry sequence. For single-state scenes this is
    /// the whole body.
    pub entry: Vec<RawLine>,
    /// Labelled inner states (`approach`, `examine`, `confront`, …).
    pub states: Vec<SceneState>,
}

/// One labelled state inside a [`SceneBody`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SceneState {
    pub name: String,
    pub body: Vec<RawLine>,
    pub span: Span,
}

/// Structured top-level GENERATOR body (spec §12.4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GeneratorBody {
    pub tier: Option<String>,
    pub priority: Option<f64>,
    /// `on boot` / `on <event>` start gating. When `None` the
    /// generator is spawned automatically when the bundle loads.
    pub start_when: Option<String>,
    /// Raw indented body — the at/every/loop machinery is lowered
    /// by the coroutine runtime.
    pub body: Vec<RawLine>,
}

/// Structured STATS body (spec §11).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StatsBody {
    pub attributes: Vec<AttributeDecl>,
    pub axes: Vec<AxisDecl>,
    pub pools: Vec<PoolDecl>,
    pub stats: Vec<StatExprDecl>,
    /// `init(args)` constructor (spec v3 §9.6). When absent, a
    /// synthesised constructor takes named args for each writable
    /// attribute and applies them in source order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub init: Option<InitDecl>,
    /// `method name(args)` callables (spec v3 §9.6).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<MethodDecl>,
}

/// `attribute strength = 10, range 1 to 30` (spec §11).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttributeDecl {
    pub name: String,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub span: Span,
}

/// `axis name { mode: …, curve: … }` (spec §11).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AxisDecl {
    pub name: String,
    /// `xp_curve` / `use_tracking` / `point_buy` / `milestone` /
    /// `narrative_trigger` / `sdk_controlled`.
    pub mode: Option<String>,
    pub curve: Option<String>,
    /// `on advance: …` — raw directive text.
    pub on_advance: Option<String>,
    /// `milestones: tutorial, novice, …` — ordered milestone names
    /// for `milestone`-mode axes (spec §11). Empty when omitted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub milestones: Vec<String>,
    pub span: Span,
}

/// `pool name { max:, regen:, cost: }` (spec §11).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PoolDecl {
    pub name: String,
    pub max: Option<String>,
    pub regen: Option<String>,
    pub cost: Option<String>,
    pub span: Span,
}

/// `stat name = expr` — computed stat (spec §11).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatExprDecl {
    pub name: String,
    pub expression: String,
    pub span: Span,
}

/// Structured TREE body (spec §11).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TreeBody {
    pub nodes: Vec<TreeNodeDecl>,
}

/// `node name { cost, requires, effect }` (spec §11).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TreeNodeDecl {
    pub name: String,
    pub cost: Option<String>,
    pub requires: Option<String>,
    /// `effect: …` lines — there can be more than one.
    pub effects: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeclarationKind {
    Character,
    /// Friendly alias for [`Self::Character`] (spec v3 §13). Parses
    /// identically; bundles materialise into the same role registry.
    Role,
    Trait,
    Item,
    Location,
    Faction,
    Stats,
    Tree,
    Generator,
    Scene,
    Cohort,
    /// A real human (or AI agent) the show knows about (spec v3 §13.2).
    Person,
    /// A planned line-up for a specific run (spec v3 §13.3).
    Roster,
}

impl DeclarationKind {
    /// The screaming-snake spelling that appears in source.
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Character => "CHARACTER",
            Self::Role => "ROLE",
            Self::Trait => "TRAIT",
            Self::Item => "ITEM",
            Self::Location => "LOCATION",
            Self::Faction => "FACTION",
            Self::Stats => "STATS",
            Self::Tree => "TREE",
            Self::Generator => "GENERATOR",
            Self::Scene => "SCENE",
            Self::Cohort => "COHORT",
            Self::Person => "PERSON",
            Self::Roster => "ROSTER",
        }
    }

    pub fn from_keyword(word: &str) -> Option<Self> {
        Some(match word {
            "CHARACTER" => Self::Character,
            "ROLE" => Self::Role,
            "TRAIT" => Self::Trait,
            "ITEM" => Self::Item,
            "LOCATION" => Self::Location,
            "FACTION" => Self::Faction,
            "STATS" => Self::Stats,
            "TREE" => Self::Tree,
            "GENERATOR" => Self::Generator,
            "SCENE" => Self::Scene,
            "COHORT" => Self::Cohort,
            "PERSON" => Self::Person,
            "ROSTER" => Self::Roster,
            _ => return None,
        })
    }

    /// True for kinds that share the CHARACTER body shape — properties,
    /// hooks, generators, stats inclusion, disposition, knowledge. ROLE
    /// is a strict alias.
    pub fn is_role_like(self) -> bool {
        matches!(self, Self::Character | Self::Role)
    }
}

/// Top-level reactive `let` binding (spec §12.1).
///
/// Phase-2: the expression is stored as raw text. The expression
/// parser + reactive graph wiring lands with the runtime crate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LetBinding {
    pub name: String,
    pub expression: String,
    pub span: Span,
}

/// A beat — `== knot_name` plus contract + body (spec §6).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Beat {
    pub name: String,
    /// Declared parameter names — `== ask_about(topic, NPC)` →
    /// `["topic", "NPC"]` (spec §8). Empty when the knot was
    /// declared without parentheses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    /// `cast:`, `setting:`, `with topic:`, … contract properties.
    pub contract: IndexMap<String, PropertyValue>,
    pub body: Vec<BodyItem>,
    pub span: Span,
}

/// One unit inside a beat body. Mixed freely; ordering matters
/// because the playhead reads top-to-bottom (spec §4).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BodyItem {
    /// Fountain-style scene heading — `INT. LIGHTHOUSE - DAWN`.
    /// Director / reader-facing; not structural (spec §6).
    SceneHeading(Located<String>),
    /// Flush-left prose paragraph.
    Action(Located<String>),
    /// A speaker block: SPEAKER line + optional `(parenthetical)` +
    /// indented dialogue lines.
    Dialogue(DialogueBlock),
    /// `* …` (once-only) or `+ …` (sticky) choice.
    Choice(Choice),
    /// `-> target`, `<-`, or `-> END`.
    Divert(Divert),
    /// Raw `<kind: args>` directive — interpreted by the runtime.
    Directive(Directive),
    /// Triple-backtick production-metadata fence (spec §15).
    Metadata(Located<String>),
    /// `<if: cond> … <else if: cond> … <else> …` syntactic form (spec §14.2).
    Conditional(Conditional),
    /// `<match: expr>` multi-arm dispatch (spec §14.2).
    Match(MatchBlock),
    /// `<each visit>` with `first` / `then` / `finally` arms (spec §14.2).
    EachVisit(EachVisit),
    /// `<after: cond> … <otherwise> …` state-morphing form (spec §14.2).
    AfterMorph(AfterMorph),
    /// `<let: name = expr>` inline lexical binding (spec §12.1, §14.2).
    InlineLet(InlineLet),
    /// Any other block-opening `<kind: args>` directive that carries
    /// an indented body (e.g. `<broadcast: …>`). The body runs after
    /// the directive's side effects.
    DirectiveBlock(DirectiveBlock),
    /// `slot: <name>` placeholder inside a beat body (spec §7 +
    /// §16). At play time the playhead replaces this with the
    /// call-site-provided body from [`Divert::To::slots`].
    SlotPlaceholder(SlotPlaceholder),
}

/// One `<match: expr>` chain — the first arm whose bare-word pattern
/// matches the scrutinee's display form wins (spec §14.2).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchBlock {
    pub scrutinee: String,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: String,
    pub body: Vec<BodyItem>,
    pub span: Span,
}

/// `<each visit>` block with `first` / `then` / `finally` arms.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EachVisit {
    pub first: Vec<BodyItem>,
    pub then: Vec<BodyItem>,
    pub finally: Vec<BodyItem>,
    pub span: Span,
}

/// `<after: cond> … <otherwise> …` state-morph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AfterMorph {
    pub condition: String,
    pub after: Vec<BodyItem>,
    pub otherwise: Vec<BodyItem>,
    pub span: Span,
}

/// `<let: name = expr>` inline binding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlineLet {
    pub name: String,
    pub expression: String,
    pub span: Span,
}

/// `slot: answer` placeholder (spec §7 + §16). Expanded by the
/// playhead at the point the enclosing beat was diverted to.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlotPlaceholder {
    pub name: String,
    pub span: Span,
}

/// One `<if:>` chain — first true arm wins (spec §14.2).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conditional {
    pub arms: Vec<ConditionalArm>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConditionalArm {
    /// `None` for the trailing `<else>` arm.
    pub condition: Option<String>,
    pub body: Vec<BodyItem>,
    pub span: Span,
}

/// A directive that opens an indented body — `<broadcast: …>`,
/// `<for: …>`, … (spec §14). The body lowers after the directive's
/// dispatch returns.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectiveBlock {
    pub directive: Directive,
    pub body: Vec<BodyItem>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DialogueBlock {
    /// Joined `|`-separated speaker line — `DOCKHAND | FISHER` (kept
    /// for back-compat with consumers that surface a single
    /// performer string). The split list lives in [`Self::speakers`].
    pub speaker: String,
    /// Split list of all addressed performers (spec §16). For a
    /// single-speaker cue this is `vec![speaker.clone()]`; for a
    /// multi-speaker cue like `DOCKHAND | FISHER` it carries both.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speakers: Vec<String>,
    /// Inline `(parenthetical)` on the line directly under the
    /// speaker. Additional inline parens inside the dialogue body
    /// appear in `lines` as `DialogueLine::Parenthetical`.
    pub parenthetical: Option<String>,
    /// `(improv duration: 45s, advance on: any [pedal, …])` block
    /// attached to the speaker (spec §13.3). Parsed out of the
    /// leading parenthetical when it opens with `improv`. The
    /// trailing `(directions to the performer)` lives in
    /// [`Self::parenthetical`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub improv: Option<ImprovDirective>,
    pub lines: Vec<DialogueLine>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum DialogueLine {
    Text(Located<String>),
    Parenthetical(Located<String>),
    Directive(Directive),
    Divert(Divert),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Choice {
    pub sticky: bool,
    /// Visible choice text — everything before the optional
    /// `[...]` suppression.
    pub text: String,
    /// Ink-style `[hidden]` tail. Played as narration after the
    /// choice is taken; not shown in the choice prompt (spec §5).
    pub suppressed: Option<String>,
    /// Indented continuation — usually a single `Divert`.
    pub body: Vec<BodyItem>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Divert {
    /// `-> name` or `-> Lighthouse/ringing` or `-> name with k: v, …`.
    ///
    /// `slots` carries any answer-slot fill (spec §7 + §16) — an
    /// indented `answer:`-style block following the divert. The
    /// playhead pushes these onto a per-frame slot map when the
    /// divert is taken so `slot: <name>` placeholders inside the
    /// target beat can expand them.
    To {
        target: DivertTarget,
        params: IndexMap<String, String>,
        #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
        slots: IndexMap<String, Vec<BodyItem>>,
        /// `-> name as Participant` (spec §13.1). Inside the
        /// invoked beat, bare identifiers like `trust` resolve
        /// against the scoped entity's namespace
        /// (`Participant.trust`) rather than a show-global.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope_as: Option<String>,
        span: Span,
    },
    /// `-> (name) ->` tunnel call (spec §7).
    Tunnel { target: DivertTarget, span: Span },
    /// `<-` tunnel return.
    Return { span: Span },
    /// `-> END` — terminate the playhead.
    End { span: Span },
}

/// A divert reference. The file qualifier supports both
/// `folder/beat` (folder hint, spec §7) and `cast/Wren#knot` (file
/// + explicit knot, spec §7 "#-form").
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DivertTarget {
    /// Optional path qualifier — the slash-separated prefix before
    /// the final segment, or the file part before `#`.
    pub qualifier: Option<String>,
    /// The bare target name — beat name, file name, or knot name.
    pub name: String,
    /// Set when the source used the `#knot` form.
    pub knot: Option<String>,
}

/// Raw `<kind: args>` directive. Phase-2 keeps the body opaque; the
/// runtime's Luau bridge tokenises it on dispatch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Directive {
    pub raw: String,
    pub span: Span,
}

/// Generic span-carrying wrapper.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Located<T> {
    pub value: T,
    pub span: Span,
}

/// A raw indented line, used inside declaration bodies until the
/// next-phase sub-grammars consume them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawLine {
    pub indent: u32,
    pub text: String,
    pub span: Span,
}
