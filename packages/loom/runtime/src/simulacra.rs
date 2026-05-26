//! Simulacra — characters as small simulations (spec §10).
//!
//! A `CHARACTER` declaration lowers to a [`CharacterState`] holding:
//! * a property bag (`voice`, `hp`, …),
//! * a disposition map keyed by the *other* character's name and the
//!   verb (`trusts` / `respects` / `fears`),
//! * a knowledge dictionary (`Wren.knows.met_player`),
//! * a set of goals as tiny state machines,
//! * a hook list (event-driven reactions), and
//! * optional [`StatsInstance`] when the character includes a
//!   `stats: Profile` declaration.
//!
//! Hooks are dispatched lazily — the playhead asks the character
//! store to drain any matched hooks at each yield point (spec §10.4:
//! "hooks never preempt the playhead").

use std::collections::{BTreeMap, HashMap, HashSet};

use loom_parser::ast::{CharacterBody, GoalDecl, KnowledgeField, RawLine};

use crate::expr::{self, ExprError, Value, World};
use crate::meridian::{StatsInstance, StatsProfile};

/// One character's complete runtime state.
#[derive(Clone, Debug, Default)]
pub struct CharacterState {
    pub name: String,
    pub inherits: Vec<String>,
    pub properties: BTreeMap<String, String>,
    /// `(verb, target)` → axis value.
    pub disposition: BTreeMap<(String, String), AxisValue>,
    pub knowledge: BTreeMap<String, Value>,
    /// Declared knowledge type spellings, captured from each
    /// [`KnowledgeField::type_spec`]. Read at write time so sum-typed
    /// (`unknown | suspects | confirmed`) and `bool` slots reject
    /// out-of-band values (spec §10.2).
    pub knowledge_schema: BTreeMap<String, String>,
    pub goals: Vec<GoalState>,
    pub hooks: Vec<HookSubscription>,
    pub stats: Option<StatsInstance>,
    /// `tag → true` per active `reacts` clause (spec §10.1).
    pub reacts_tags: HashSet<String>,
    /// Unlocked tree nodes by name. Cross-references resolved by the
    /// runtime — the character doesn't own the tree definition.
    pub unlocked_nodes: BTreeMap<String, bool>,
    /// Edge-trigger memory for `on … passes N` style hooks. Keyed by
    /// the hook event string; value is the last numeric reading we
    /// observed.
    pub threshold_memory: BTreeMap<String, f64>,
    /// `reacts` clauses stored so the character can re-evaluate its
    /// own tags whenever disposition / knowledge changes.
    pub react_predicates: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AxisValue {
    pub current: f64,
    pub max: f64,
}

/// A goal's lifecycle (spec §10.3).
#[derive(Clone, Debug, Default)]
pub struct GoalState {
    pub decl: GoalDecl,
    pub status: GoalStatus,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GoalStatus {
    #[default]
    Dormant,
    Pursuing,
    Complete,
    Failed,
}

/// One subscribed hook — what it reacts to and what it runs.
#[derive(Clone, Debug)]
pub struct HookSubscription {
    pub event: String,
    pub body: Vec<RawLine>,
    /// `true` after the first time the hook fires for `on meeting X`
    /// style once-only triggers. Pure edge-triggered hooks use
    /// `threshold_memory` on the character instead.
    pub one_shot_fired: bool,
}

impl CharacterState {
    /// Compile a character from its parsed [`CharacterBody`].
    /// `inherits` carries the `is X, Y` clause from the declaration
    /// opener (resolution into parent state is out of scope here —
    /// the runtime caller does that pass and merges the resulting
    /// properties into ours before calling [`Self::compile_body`]).
    pub fn compile(
        name: impl Into<String>,
        inherits: Vec<String>,
        body: &CharacterBody,
        profiles: &HashMap<String, StatsProfile>,
        world: &World,
    ) -> Self {
        let mut state = CharacterState {
            name: name.into(),
            inherits,
            ..CharacterState::default()
        };
        state.compile_body(body, profiles, world);
        state
    }

    /// Apply a body's contents on top of existing state. Used both
    /// for the initial compile and (later) for inheritance merging.
    pub fn compile_body(
        &mut self,
        body: &CharacterBody,
        profiles: &HashMap<String, StatsProfile>,
        world: &World,
    ) {
        for (k, v) in &body.properties {
            self.properties.insert(k.clone(), v.value.clone());
        }
        for d in &body.disposition {
            self.disposition.insert(
                (d.verb.clone(), d.target.clone()),
                AxisValue {
                    current: d.current,
                    max: d.max,
                },
            );
        }
        for k in &body.knowledge {
            self.knowledge.insert(k.name.clone(), seed_knowledge(k));
            self.knowledge_schema
                .insert(k.name.clone(), k.type_spec.clone());
        }
        for g in &body.goals {
            self.goals.push(GoalState {
                decl: g.clone(),
                status: GoalStatus::Dormant,
            });
        }
        for h in &body.hooks {
            self.hooks.push(HookSubscription {
                event: h.event.clone(),
                body: h.body.clone(),
                one_shot_fired: false,
            });
        }
        if let Some(profile_name) = &body.stats_profile {
            if let Some(profile) = profiles.get(profile_name) {
                self.stats = Some(StatsInstance::from_profile(profile, world));
            }
        }
        // Stash the predicates so `refresh_own_reacts` can recompute
        // the tag set after any later mutation.
        self.react_predicates = body_reacts_predicates(body);
        let preds = self.react_predicates.clone();
        self.refresh_reacts(&preds, world);
    }

    /// Recompute tags using the predicates the character was compiled
    /// with — convenient for runtime callers that don't carry the
    /// `CharacterBody` around.
    pub fn refresh_own_reacts(&mut self, world: &World) {
        let preds = self.react_predicates.clone();
        self.refresh_reacts(&preds, world);
    }

    /// Re-evaluate `reacts <cond> -> <tag>` clauses against `world`.
    /// Called by the playhead after every world mutation that might
    /// touch a dependency.
    pub fn refresh_reacts(&mut self, predicates: &[(String, String)], world: &World) {
        let scoped = self.scoped_world(world);
        let mut tags = HashSet::new();
        for (cond, tag) in predicates {
            let parsed = match expr::parse(cond) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let value = expr::eval(&parsed, &scoped, &mut |n, _| {
                Err(ExprError::UnknownFunction(n.into()))
            });
            if value.map(|v| v.truthy()).unwrap_or(false) {
                tags.insert(tag.clone());
            }
        }
        self.reacts_tags = tags;
    }

    /// True if `tag` is currently asserted by a `reacts` clause.
    pub fn has_tag(&self, tag: &str) -> bool {
        self.reacts_tags.contains(tag)
    }

    /// Publish this character's namespace into the world so dotted
    /// paths (`Wren.trusts.Player`, `Wren.knows.met_player`,
    /// `Wren.strength`, `Wren.health`) resolve.
    pub fn publish(&self, world: &mut World) {
        let prefix = self.name.clone();
        for (k, v) in &self.properties {
            // Property values are raw text — try to coerce simple
            // numeric / bool spellings, otherwise leave as string.
            world.set(format!("{prefix}.{k}"), parse_property_value(v));
        }
        for ((verb, target), axis) in &self.disposition {
            world.set(
                format!("{prefix}.{verb}.{target}"),
                Value::Number(axis.current),
            );
            world.set(
                format!("{prefix}.{verb}.{target}.max"),
                Value::Number(axis.max),
            );
        }
        for (k, v) in &self.knowledge {
            world.set(format!("{prefix}.knows.{k}"), v.clone());
        }
        for (node, on) in &self.unlocked_nodes {
            world.set(format!("{prefix}.tree.{node}"), Value::Bool(*on));
        }
        if let Some(stats) = &self.stats {
            stats.publish(&prefix, world);
        }
    }

    /// Run the goal state machine once. Predicates re-evaluated
    /// against `world`. Returns the names of goals that just
    /// completed or failed (so the caller can lower `on_complete` /
    /// `on_fail` bodies).
    pub fn tick_goals(&mut self, world: &World) -> Vec<GoalEvent> {
        let scoped = self.scoped_world(world);
        let mut events = Vec::new();
        for goal in &mut self.goals {
            let name = goal.decl.name.clone();
            let pred = |src: &Option<String>| -> bool {
                match src {
                    None => false,
                    Some(text) => evaluate_predicate(text, &scoped),
                }
            };
            let active = goal
                .decl
                .active_when
                .as_ref()
                .map(|t| evaluate_predicate(t, &scoped))
                .unwrap_or(true);
            match goal.status {
                GoalStatus::Dormant => {
                    if active {
                        goal.status = GoalStatus::Pursuing;
                    }
                }
                GoalStatus::Pursuing => {
                    if pred(&goal.decl.completes_when) {
                        goal.status = GoalStatus::Complete;
                        events.push(GoalEvent::Completed(name));
                    } else if pred(&goal.decl.fails_when) || !active {
                        goal.status = GoalStatus::Failed;
                        events.push(GoalEvent::Failed(name));
                    }
                }
                _ => {}
            }
        }
        events
    }

    /// Apply a `<set: Wren.knows.field := value>` mutation. Returns
    /// [`SetOutcome::Consumed`] when the path matched a knowledge /
    /// disposition slot (and was therefore consumed),
    /// [`SetOutcome::Rejected`] when the slot's declared schema
    /// refuses the value, or [`SetOutcome::Passthrough`] to let the
    /// generic world set handler take over.
    pub fn apply_set(&mut self, path: &[String], value: &Value, op: SetOp) -> SetOutcome {
        // `Wren.knows.field`
        if path.len() == 3 && path[1] == "knows" {
            let key = &path[2];
            let new_value = match op {
                SetOp::Assign => value.clone(),
                SetOp::Add => add_values(
                    self.knowledge.get(key).cloned().unwrap_or(Value::Null),
                    value.clone(),
                ),
                SetOp::Sub => sub_values(
                    self.knowledge.get(key).cloned().unwrap_or(Value::Null),
                    value.clone(),
                ),
            };
            // Schema gate (spec §10.2). Sum and bool slots reject
            // anything outside their declared shape; everything else
            // stays opaque.
            if let Some(spec) = self.knowledge_schema.get(key) {
                if let Some(message) = validate_knowledge_value(spec, op, &new_value) {
                    return SetOutcome::Rejected(message);
                }
            }
            self.knowledge.insert(key.clone(), new_value);
            return SetOutcome::Consumed;
        }
        // `Wren.trusts.Player` / `.respects.X` / `.fears.X`
        if path.len() == 3 && matches!(path[1].as_str(), "trusts" | "respects" | "fears") {
            let verb = path[1].clone();
            let target = path[2].clone();
            let axis = self
                .disposition
                .entry((verb.clone(), target.clone()))
                .or_insert(AxisValue {
                    current: 0.0,
                    max: 100.0,
                });
            let rhs = value.as_number().unwrap_or(0.0);
            axis.current = match op {
                SetOp::Assign => rhs,
                SetOp::Add => axis.current + rhs,
                SetOp::Sub => axis.current - rhs,
            };
            // Clamp to declared range.
            if axis.current < 0.0 {
                axis.current = 0.0;
            }
            if axis.current > axis.max {
                axis.current = axis.max;
            }
            return SetOutcome::Consumed;
        }
        SetOutcome::Passthrough
    }

    /// Match the character's hooks against `event` text. Returns the
    /// indices of subscriptions that should fire. Edge-triggered
    /// `passes N` hooks are filtered against `threshold_memory`.
    pub fn match_hooks(&mut self, event: &HookEvent<'_>) -> Vec<usize> {
        let mut hits = Vec::new();
        for (idx, sub) in self.hooks.iter_mut().enumerate() {
            if hook_matches(&sub.event, event, &mut self.threshold_memory) {
                if sub.one_shot_fired && is_one_shot(&sub.event) {
                    continue;
                }
                if is_one_shot(&sub.event) {
                    sub.one_shot_fired = true;
                }
                hits.push(idx);
            }
        }
        hits
    }

    fn scoped_world(&self, base: &World) -> World {
        let mut w = base.clone();
        // Surface bare-name aliases for disposition + knowledge so
        // hook conditions like `trust > 60` work without spelling the
        // character name. (Spec §10.1 example uses bare verbs.)
        for ((verb, _target), axis) in &self.disposition {
            // First match wins for the bare alias.
            let bare = verb.trim_end_matches('s').to_string(); // trusts → trust
            if !matches!(w.get(&bare), Value::Number(_)) {
                w.set(bare, Value::Number(axis.current));
            }
        }
        for (k, v) in &self.knowledge {
            w.set(k.clone(), v.clone());
        }
        if let Some(stats) = &self.stats {
            for (k, v) in &stats.attributes {
                w.set(k.clone(), Value::Number(*v));
            }
        }
        w
    }
}

/// One event a goal lifecycle transition produces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GoalEvent {
    Completed(String),
    Failed(String),
}

/// Mutation op understood by [`CharacterState::apply_set`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetOp {
    Assign,
    Add,
    Sub,
}

/// Result of [`CharacterState::apply_set`] — whether the character
/// store claimed the path, refused the value, or wants the caller to
/// pass it through to the generic world set handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetOutcome {
    /// The mutation matched a knowledge or disposition slot and was
    /// applied.
    Consumed,
    /// The mutation matched a slot but the declared schema refused
    /// the value (spec §10.2). The string carries a writer-facing
    /// reason; the caller should surface it as a directive error.
    Rejected(String),
    /// The path is not a character slot. The caller should run the
    /// generic world write path.
    Passthrough,
}

/// What the playhead saw — used to match against [`HookSubscription`].
#[derive(Clone, Debug)]
pub enum HookEvent<'a> {
    /// `on meeting X` — fired by `<meet: X>` directive / first
    /// dialogue exchange.
    Meeting(&'a str),
    /// `on trust passes N` / `respects passes N` style threshold cross.
    DispositionPasses {
        verb: &'a str,
        target: &'a str,
        value: f64,
    },
    /// `on cue X` — fired by `<cue: X>`.
    Cue(&'a str),
    /// `on event X` — fired by `<fire: X>`.
    Fired(&'a str),
    /// `on Participant enters L`.
    Enters(&'a str),
    /// `on Participant exits L`.
    Exits(&'a str),
    /// `on <verb> drops below N` — downward threshold cross,
    /// symmetric to [`Self::DispositionPasses`].
    DispositionDropsBelow {
        verb: &'a str,
        target: &'a str,
        value: f64,
    },
    /// `on participant joins` — a new audience member entered the
    /// stage (spec §13.1).
    ParticipantJoins,
}

fn hook_matches(
    pattern: &str,
    event: &HookEvent<'_>,
    threshold: &mut BTreeMap<String, f64>,
) -> bool {
    let pat = pattern.trim();
    match event {
        HookEvent::Meeting(who) => {
            if let Some(rest) = pat.strip_prefix("meeting ") {
                return rest.trim() == *who;
            }
        }
        HookEvent::Cue(name) => {
            if let Some(rest) = pat.strip_prefix("cue ") {
                return rest.trim() == *name;
            }
        }
        HookEvent::Fired(name) => {
            if let Some(rest) = pat.strip_prefix("event ") {
                return rest.trim() == *name;
            }
        }
        HookEvent::Enters(loc) => {
            if let Some(rest) = pat.strip_prefix("Participant enters ") {
                return rest.trim() == *loc;
            }
        }
        HookEvent::Exits(loc) => {
            if let Some(rest) = pat.strip_prefix("Participant exits ") {
                return rest.trim() == *loc;
            }
        }
        HookEvent::ParticipantJoins => {
            return pat == "participant joins" || pat == "Participant joins";
        }
        HookEvent::DispositionPasses { verb, value, .. } => {
            // `trust passes N` (verbs stored as `trusts` get bare
            // alias `trust` here).
            let bare_verb = verb.trim_end_matches('s');
            let prefix = format!("{bare_verb} passes ");
            if let Some(rest) = pat.strip_prefix(&prefix) {
                let threshold_value: f64 = match rest.trim().parse() {
                    Ok(n) => n,
                    Err(_) => return false,
                };
                // Seed at +infinity so the first observation is treated
                // as if the value started above the threshold (i.e.
                // only an actual upward crossing fires).
                let last = threshold.get(pat).copied().unwrap_or(f64::NEG_INFINITY);
                let crossed = last < threshold_value && *value >= threshold_value;
                threshold.insert(pat.to_string(), *value);
                return crossed;
            }
        }
        HookEvent::DispositionDropsBelow { verb, value, .. } => {
            let bare_verb = verb.trim_end_matches('s');
            let prefix = format!("{bare_verb} drops below ");
            if let Some(rest) = pat.strip_prefix(&prefix) {
                let threshold_value: f64 = match rest.trim().parse() {
                    Ok(n) => n,
                    Err(_) => return false,
                };
                // Seed at +infinity so the first observation below
                // threshold fires once, then subsequent stays-below
                // do not refire until the value climbs back above.
                let last = threshold.get(pat).copied().unwrap_or(f64::INFINITY);
                let crossed = last >= threshold_value && *value < threshold_value;
                threshold.insert(pat.to_string(), *value);
                return crossed;
            }
        }
    }
    false
}

/// Schema gate for `<set: …knows.field …>` writes (spec §10.2).
/// Returns `Some(error_message)` when the declared `type_spec` refuses
/// `value`. Sum (`unknown | suspects | confirmed`) and `bool` slots
/// are validated; everything else is treated as opaque and accepted.
fn validate_knowledge_value(type_spec: &str, op: SetOp, value: &Value) -> Option<String> {
    let spec = type_spec.trim();
    if spec == "bool" {
        return match value {
            Value::Bool(_) => None,
            other => Some(format!("expected bool, got {}", describe_value(other))),
        };
    }
    if spec.contains('|') {
        let variants: Vec<&str> = spec
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        // Compound ops (+=, -=) on sum slots are nonsensical; reject
        // them with the same channel as a bad variant.
        if !matches!(op, SetOp::Assign) {
            return Some(
                "sum-typed knowledge slot does not accept compound assignment".to_string(),
            );
        }
        let candidate = match value {
            Value::String(s) => s.trim().to_string(),
            other => other.display(),
        };
        if variants.iter().any(|v| *v == candidate) {
            return None;
        }
        return Some(format!(
            "expected one of {}, got `{}`",
            variants.join(" | "),
            candidate
        ));
    }
    None
}

fn describe_value(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::List(_) => "list",
        Value::Null => "null",
    }
}

fn is_one_shot(event: &str) -> bool {
    event.starts_with("meeting ")
}

fn evaluate_predicate(text: &str, world: &World) -> bool {
    let parsed = match expr::parse(text) {
        Ok(p) => p,
        Err(_) => return false,
    };
    expr::eval(&parsed, world, &mut |n, _| {
        Err(ExprError::UnknownFunction(n.into()))
    })
    .map(|v| v.truthy())
    .unwrap_or(false)
}

fn seed_knowledge(k: &KnowledgeField) -> Value {
    match k.default.as_deref() {
        Some("true") => Value::Bool(true),
        Some("false") => Value::Bool(false),
        Some(other) => match other.parse::<f64>() {
            Ok(n) => Value::Number(n),
            Err(_) => Value::String(other.to_string()),
        },
        None => Value::Null,
    }
}

fn parse_property_value(text: &str) -> Value {
    if let Ok(n) = text.parse::<f64>() {
        return Value::Number(n);
    }
    match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        other => Value::String(other.to_string()),
    }
}

fn body_reacts_predicates(body: &CharacterBody) -> Vec<(String, String)> {
    body.reacts
        .iter()
        .map(|r| (r.condition.clone(), r.tag.clone()))
        .collect()
}

fn add_values(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::List(mut items), other) => {
            items.push(other);
            Value::List(items)
        }
        (Value::Number(x), Value::Number(y)) => Value::Number(x + y),
        (Value::Number(x), other) => Value::Number(x + other.as_number().unwrap_or(0.0)),
        (Value::Null, other) => other,
        (Value::String(s), other) => Value::String(format!("{s}{}", other.display())),
        (Value::Bool(b), other) => {
            Value::Number(if b { 1.0 } else { 0.0 } + other.as_number().unwrap_or(0.0))
        }
    }
}

fn sub_values(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::List(items), other) => {
            let target = other.display();
            Value::List(
                items
                    .into_iter()
                    .filter(|v| v.display() != target)
                    .collect(),
            )
        }
        (Value::Number(x), Value::Number(y)) => Value::Number(x - y),
        (Value::Number(x), other) => Value::Number(x - other.as_number().unwrap_or(0.0)),
        (a, b) => Value::Number(a.as_number().unwrap_or(0.0) - b.as_number().unwrap_or(0.0)),
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use loom_parser::ast::{DispositionAxis, HookDecl, KnowledgeField, ReactClause};
    use loom_parser::source::Span;

    fn span() -> Span {
        Span::default()
    }

    fn wren_body() -> CharacterBody {
        CharacterBody {
            disposition: vec![DispositionAxis {
                verb: "trusts".into(),
                target: "Player".into(),
                current: 30.0,
                max: 100.0,
                mirror: None,
                span: span(),
            }],
            knowledge: vec![KnowledgeField {
                name: "met_player".into(),
                type_spec: "bool".into(),
                default: Some("false".into()),
                span: span(),
            }],
            reacts: vec![ReactClause {
                condition: "trust > 60".into(),
                tag: "warm".into(),
                span: span(),
            }],
            ..CharacterBody::default()
        }
    }

    #[test]
    fn publishes_dotted_paths() {
        let body = wren_body();
        let mut state =
            CharacterState::compile("Wren", Vec::new(), &body, &HashMap::new(), &World::new());
        let mut world = World::new();
        state.publish(&mut world);
        assert_eq!(world.get("Wren.trusts.Player"), Value::Number(30.0));
        assert_eq!(world.get("Wren.knows.met_player"), Value::Bool(false));
        // Apply a disposition bump.
        state.apply_set(
            &["Wren".into(), "trusts".into(), "Player".into()],
            &Value::Number(20.0),
            SetOp::Add,
        );
        let mut world = World::new();
        state.publish(&mut world);
        assert_eq!(world.get("Wren.trusts.Player"), Value::Number(50.0));
    }

    #[test]
    fn reacts_tags_track_disposition() {
        let body = wren_body();
        let mut state =
            CharacterState::compile("Wren", Vec::new(), &body, &HashMap::new(), &World::new());
        assert!(!state.has_tag("warm"));
        state.apply_set(
            &["Wren".into(), "trusts".into(), "Player".into()],
            &Value::Number(40.0),
            SetOp::Add,
        );
        state.refresh_reacts(&body_reacts_predicates(&body), &World::new());
        assert!(state.has_tag("warm"));
    }

    #[test]
    fn goal_lifecycle_runs() {
        let mut body = wren_body();
        body.goals.push(GoalDecl {
            name: "find_keeper".into(),
            priority: Some(0.8),
            active_when: Some("true".into()),
            completes_when: Some("Wren.knows.saw_the_keeper".into()),
            ..GoalDecl::default()
        });
        let mut state =
            CharacterState::compile("Wren", Vec::new(), &body, &HashMap::new(), &World::new());
        // First tick: dormant -> pursuing.
        let mut world = World::new();
        state.publish(&mut world);
        let events = state.tick_goals(&world);
        assert!(events.is_empty());
        assert_eq!(state.goals[0].status, GoalStatus::Pursuing);
        // Flip the predicate true.
        state
            .knowledge
            .insert("saw_the_keeper".into(), Value::Bool(true));
        let mut world = World::new();
        state.publish(&mut world);
        let events = state.tick_goals(&world);
        assert_eq!(events, vec![GoalEvent::Completed("find_keeper".into())]);
        assert_eq!(state.goals[0].status, GoalStatus::Complete);
    }

    #[test]
    fn threshold_hook_fires_once_per_crossing() {
        let mut body = wren_body();
        body.hooks.push(HookDecl {
            event: "trust passes 80".into(),
            body: Vec::new(),
            suppressed: false,
            span: span(),
        });
        let mut state =
            CharacterState::compile("Wren", Vec::new(), &body, &HashMap::new(), &World::new());
        // Below threshold — no fire.
        let hits = state.match_hooks(&HookEvent::DispositionPasses {
            verb: "trusts",
            target: "Player",
            value: 50.0,
        });
        assert!(hits.is_empty());
        // Cross.
        let hits = state.match_hooks(&HookEvent::DispositionPasses {
            verb: "trusts",
            target: "Player",
            value: 90.0,
        });
        assert_eq!(hits.len(), 1);
        // Already crossed — same value shouldn't refire.
        let hits = state.match_hooks(&HookEvent::DispositionPasses {
            verb: "trusts",
            target: "Player",
            value: 95.0,
        });
        assert!(hits.is_empty());
    }
}
