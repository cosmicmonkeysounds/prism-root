//! Meridian — stats, axes, pools, trees (spec §11).
//!
//! Five primitives:
//! * `attribute` — static per-character integer with a range.
//! * `axis` — runtime-advancing dimension (one of six modes).
//! * `pool` — spendable supply with `max` / `regen` / `cost`.
//! * `stat` — computed value (expression evaluated against the
//!   character's namespace and the world).
//! * `TREE` — DAG of unlockable nodes (`cost` + `requires` + `effect`).
//!
//! A [`StatsProfile`] is compiled from a `STATS` declaration and
//! instantiated as a [`StatsInstance`] per character that opts in
//! (`stats: Combat`). Profile names mirror the source identifiers;
//! everything else lowers into Rust values the playhead can read.

use std::collections::BTreeMap;

use loom_parser::ast::{
    AttributeDecl, AxisDecl, PoolDecl, StatExprDecl, StatsBody, TreeBody, TreeNodeDecl,
};

use crate::expr::{self, ExprError, Value, World};

/// One compiled STATS declaration.
#[derive(Clone, Debug, Default)]
pub struct StatsProfile {
    pub name: String,
    pub attributes: Vec<AttributeDecl>,
    pub axes: Vec<AxisDecl>,
    pub pools: Vec<PoolDecl>,
    pub stats: Vec<StatExprDecl>,
}

impl StatsProfile {
    pub fn from_body(name: impl Into<String>, body: &StatsBody) -> Self {
        Self {
            name: name.into(),
            attributes: body.attributes.clone(),
            axes: body.axes.clone(),
            pools: body.pools.clone(),
            stats: body.stats.clone(),
        }
    }
}

/// Per-character state for a single [`StatsProfile`].
#[derive(Clone, Debug, Default)]
pub struct StatsInstance {
    pub profile_name: String,
    pub attributes: BTreeMap<String, f64>,
    pub axes: BTreeMap<String, AxisState>,
    pub pools: BTreeMap<String, PoolState>,
    /// Raw stat expressions kept around for lazy evaluation against a
    /// world snapshot that includes the character's attributes.
    pub stat_expressions: BTreeMap<String, String>,
    /// Cached attribute ranges so mutations can clamp.
    pub attribute_ranges: BTreeMap<String, (f64, f64)>,
}

/// Live state of a single [`AxisDecl`].
#[derive(Clone, Debug, Default)]
pub struct AxisState {
    pub mode: AxisMode,
    /// `xp` for `xp_curve`; usage counter for `use_tracking`;
    /// milestone count for `milestone`; trigger count for
    /// `narrative_trigger`; unused for `sdk_controlled`.
    pub progress: f64,
    /// Logical level the axis has reached. Bumped by `advance` once a
    /// curve threshold is crossed.
    pub level: u32,
    /// `level * level * 50` style curve expression. Only consulted for
    /// `xp_curve`.
    pub curve_expression: Option<String>,
    /// `<fire: level_up>` style directive text emitted when `level`
    /// increments. Surfaced to the playhead as a deferred event.
    pub on_advance: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AxisMode {
    #[default]
    NarrativeTrigger,
    XpCurve,
    UseTracking,
    PointBuy,
    Milestone,
    SdkControlled,
}

impl AxisMode {
    pub fn parse(text: &str) -> AxisMode {
        match text.trim() {
            "xp_curve" => AxisMode::XpCurve,
            "use_tracking" => AxisMode::UseTracking,
            "point_buy" => AxisMode::PointBuy,
            "milestone" => AxisMode::Milestone,
            "sdk_controlled" => AxisMode::SdkControlled,
            _ => AxisMode::NarrativeTrigger,
        }
    }
}

/// Live state of a single [`PoolDecl`].
#[derive(Clone, Debug, Default)]
pub struct PoolState {
    pub current: f64,
    pub max: f64,
    /// `regen` parsed into a per-second rate (the part before `/s`).
    /// `0.0` means no regen.
    pub regen_per_second: f64,
    /// When `Some(true)`, regen only ticks while `in_combat` is false
    /// (the spec's `regen: 2/s when not in_combat`).
    pub regen_when_out_of_combat: bool,
    /// Optional cost-per-use expression text — surfaced for callers
    /// that want to dock the pool when an ability fires.
    pub cost_expression: Option<String>,
}

impl PoolState {
    /// Tick `dt` seconds of regen. Respects the
    /// `regen_when_out_of_combat` gate.
    pub fn tick(&mut self, dt: f64, in_combat: bool) {
        if self.regen_per_second <= 0.0 {
            return;
        }
        if self.regen_when_out_of_combat && in_combat {
            return;
        }
        self.current = (self.current + self.regen_per_second * dt).min(self.max);
    }
}

impl StatsInstance {
    /// Build a fresh instance from a profile, seeding attributes to
    /// their defaults and pools to their `max` value.
    pub fn from_profile(profile: &StatsProfile, world: &World) -> Self {
        let mut inst = StatsInstance {
            profile_name: profile.name.clone(),
            ..Default::default()
        };
        for attr in &profile.attributes {
            inst.attributes.insert(attr.name.clone(), attr.default);
            inst.attribute_ranges
                .insert(attr.name.clone(), (attr.min, attr.max));
        }
        for axis in &profile.axes {
            inst.axes.insert(
                axis.name.clone(),
                AxisState {
                    mode: axis
                        .mode
                        .as_deref()
                        .map(AxisMode::parse)
                        .unwrap_or_default(),
                    curve_expression: axis.curve.clone(),
                    on_advance: axis.on_advance.clone(),
                    ..AxisState::default()
                },
            );
        }
        for stat in &profile.stats {
            inst.stat_expressions
                .insert(stat.name.clone(), stat.expression.clone());
        }
        // Pools depend on stat expressions (`max: max_health`); resolve
        // those against a world that already has attributes loaded.
        let scoped = world_with_attributes(world, &inst.attributes);
        for pool in &profile.pools {
            let max = resolve_pool_max(pool, &scoped, &inst.stat_expressions);
            let (rate, out_of_combat) = parse_regen(pool.regen.as_deref().unwrap_or(""));
            inst.pools.insert(
                pool.name.clone(),
                PoolState {
                    current: max,
                    max,
                    regen_per_second: rate,
                    regen_when_out_of_combat: out_of_combat,
                    cost_expression: pool.cost.clone(),
                },
            );
        }
        inst
    }

    /// Evaluate one named stat against `world` augmented with this
    /// instance's attributes + axes + pools. Returns [`Value::Null`]
    /// when the stat name isn't known.
    pub fn evaluate_stat(&self, name: &str, world: &World) -> Result<Value, ExprError> {
        let Some(src) = self.stat_expressions.get(name) else {
            return Ok(Value::Null);
        };
        let scoped = self.scoped_world(world);
        let parsed = expr::parse(src)?;
        expr::eval(&parsed, &scoped, &mut |fname, _args| {
            Err(ExprError::UnknownFunction(fname.into()))
        })
    }

    /// Snapshot the instance into a [`World`] derived from `base`,
    /// adding bare attribute / axis-level / pool-current entries so
    /// stat expressions can read them as plain identifiers
    /// (`strength`, `level`, `health`).
    pub fn scoped_world(&self, base: &World) -> World {
        let mut w = base.clone();
        for (k, v) in &self.attributes {
            w.set(k.clone(), Value::Number(*v));
        }
        for (k, axis) in &self.axes {
            w.set(k.clone(), Value::Number(axis.level as f64));
            w.set(format!("{k}.xp"), Value::Number(axis.progress));
        }
        for (k, pool) in &self.pools {
            w.set(k.clone(), Value::Number(pool.current));
            w.set(format!("{k}.max"), Value::Number(pool.max));
        }
        w
    }

    /// Publish this instance into the world under `prefix`
    /// (e.g. `Wren` → `Wren.strength`, `Wren.level`, `Wren.health`,
    /// `Wren.health.max`, `Wren.damage`).
    pub fn publish(&self, prefix: &str, world: &mut World) {
        for (k, v) in &self.attributes {
            world.set(format!("{prefix}.{k}"), Value::Number(*v));
        }
        for (k, axis) in &self.axes {
            world.set(format!("{prefix}.{k}"), Value::Number(axis.level as f64));
            world.set(
                format!("{prefix}.{k}.xp"),
                Value::Number(axis.progress),
            );
        }
        for (k, pool) in &self.pools {
            world.set(format!("{prefix}.{k}"), Value::Number(pool.current));
            world.set(format!("{prefix}.{k}.max"), Value::Number(pool.max));
        }
        // Stat expressions evaluated against the scoped world so they
        // see attribute values.
        for name in self.stat_expressions.keys() {
            if let Ok(v) = self.evaluate_stat(name, world) {
                world.set(format!("{prefix}.{name}"), v);
            }
        }
    }

    /// Set an attribute value, clamping to the declared range.
    pub fn set_attribute(&mut self, name: &str, value: f64) {
        let (lo, hi) = self
            .attribute_ranges
            .get(name)
            .copied()
            .unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
        let clamped = value.clamp(lo, hi);
        self.attributes.insert(name.to_string(), clamped);
    }

    /// Advance an axis. For `xp_curve` axes, `delta` is added to xp
    /// and `level` bumps each time the curve threshold is crossed.
    /// For `narrative_trigger`, every call bumps level by 1.
    /// Returns the number of levels gained.
    pub fn advance_axis(&mut self, name: &str, delta: f64) -> u32 {
        let Some(axis) = self.axes.get_mut(name) else {
            return 0;
        };
        let mut gained = 0u32;
        match axis.mode {
            AxisMode::XpCurve => {
                axis.progress += delta;
                loop {
                    let next_level = (axis.level + 1) as f64;
                    let mut w = World::new();
                    w.set("level", Value::Number(next_level));
                    let threshold = axis
                        .curve_expression
                        .as_ref()
                        .and_then(|c| {
                            let parsed = expr::parse(c).ok()?;
                            expr::eval(&parsed, &w, &mut |n, _| {
                                Err(ExprError::UnknownFunction(n.into()))
                            })
                            .ok()
                        })
                        .and_then(|v| v.as_number())
                        .unwrap_or(f64::INFINITY);
                    if axis.progress >= threshold {
                        axis.progress -= threshold;
                        axis.level += 1;
                        gained += 1;
                    } else {
                        break;
                    }
                }
            }
            AxisMode::NarrativeTrigger | AxisMode::Milestone => {
                let bump = delta.max(1.0).floor() as u32;
                axis.level += bump;
                gained = bump;
            }
            AxisMode::UseTracking => {
                axis.progress += delta;
                gained = 0;
            }
            AxisMode::PointBuy | AxisMode::SdkControlled => {
                // Mode is recognised but the actual driver isn't
                // implemented yet. TODO(spec §11): point-buy budget +
                // SDK pump callback.
            }
        }
        gained
    }
}

fn world_with_attributes(base: &World, attrs: &BTreeMap<String, f64>) -> World {
    let mut w = base.clone();
    for (k, v) in attrs {
        w.set(k.clone(), Value::Number(*v));
    }
    w
}

fn resolve_pool_max(
    pool: &PoolDecl,
    world: &World,
    stat_expressions: &BTreeMap<String, String>,
) -> f64 {
    let Some(text) = pool.max.as_deref() else {
        return 0.0;
    };
    if let Ok(n) = text.parse::<f64>() {
        return n;
    }
    // Try resolving the bare reference as a stat expression first.
    if let Some(src) = stat_expressions.get(text.trim()) {
        if let Ok(parsed) = expr::parse(src) {
            if let Ok(v) = expr::eval(&parsed, world, &mut |n, _| {
                Err(ExprError::UnknownFunction(n.into()))
            }) {
                return v.as_number().unwrap_or(0.0);
            }
        }
    }
    // Fall back to parsing the text as an expression directly.
    if let Ok(parsed) = expr::parse(text) {
        if let Ok(v) = expr::eval(&parsed, world, &mut |n, _| {
            Err(ExprError::UnknownFunction(n.into()))
        }) {
            return v.as_number().unwrap_or(0.0);
        }
    }
    0.0
}

/// Parse `"2/s when not in_combat"` style regen text. Returns
/// `(rate_per_second, only_out_of_combat)`.
fn parse_regen(text: &str) -> (f64, bool) {
    let text = text.trim();
    if text.is_empty() {
        return (0.0, false);
    }
    let (rate_part, gate) = match text.find(" when ") {
        Some(idx) => (
            text[..idx].trim(),
            Some(text[idx + 6..].trim().to_string()),
        ),
        None => (text, None),
    };
    let rate = if let Some(num) = rate_part.strip_suffix("/s") {
        num.trim().parse::<f64>().unwrap_or(0.0)
    } else {
        rate_part.parse::<f64>().unwrap_or(0.0)
    };
    let out_of_combat = matches!(gate.as_deref(), Some("not in_combat"));
    (rate, out_of_combat)
}

// ---------------------------------------------------------------------
// TREE
// ---------------------------------------------------------------------

/// A compiled `TREE` declaration (spec §11).
#[derive(Clone, Debug, Default)]
pub struct Tree {
    pub name: String,
    pub nodes: BTreeMap<String, TreeNodeDecl>,
}

impl Tree {
    pub fn from_body(name: impl Into<String>, body: &TreeBody) -> Self {
        let mut nodes = BTreeMap::new();
        for n in &body.nodes {
            nodes.insert(n.name.clone(), n.clone());
        }
        Self {
            name: name.into(),
            nodes,
        }
    }

    /// Check whether `node` can currently be unlocked for `world`.
    /// `unlocked` is the set of nodes already taken on this character.
    pub fn can_unlock(
        &self,
        node_name: &str,
        unlocked: &BTreeMap<String, bool>,
        world: &World,
    ) -> bool {
        let Some(node) = self.nodes.get(node_name) else {
            return false;
        };
        if unlocked.get(node_name).copied().unwrap_or(false) {
            return false;
        }
        let Some(req) = node.requires.as_deref() else {
            return true;
        };
        evaluate_requires(req, unlocked, world)
    }
}

fn evaluate_requires(req: &str, unlocked: &BTreeMap<String, bool>, world: &World) -> bool {
    // Bridge two tiny call-shapes into the expression evaluator: a
    // `node(name)` query asks whether a sibling node is already
    // unlocked; an `axis(name)` query reads the axis level from the
    // world (where `StatsInstance::publish` left it).
    let parsed = match expr::parse(req) {
        Ok(p) => p,
        Err(_) => return false,
    };
    expr::eval(&parsed, world, &mut |name, args| match name {
        "node" => {
            let arg = args.first().map(|a| a.as_name()).unwrap_or_default();
            Ok(Value::Bool(unlocked.get(&arg).copied().unwrap_or(false)))
        }
        "axis" => {
            // `axis(one_handed)` reads `one_handed` from the world.
            let arg = args.first().map(|a| a.as_name()).unwrap_or_default();
            Ok(world.get(&arg))
        }
        other => Err(ExprError::UnknownFunction(other.into())),
    })
    .map(|v| v.truthy())
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_parser::ast::AttributeDecl;
    use loom_parser::source::Span;

    fn span() -> Span {
        Span::default()
    }

    fn body() -> StatsBody {
        StatsBody {
            attributes: vec![AttributeDecl {
                name: "strength".into(),
                default: 10.0,
                min: 1.0,
                max: 30.0,
                span: span(),
            }],
            axes: vec![AxisDecl {
                name: "level".into(),
                mode: Some("xp_curve".into()),
                curve: Some("level * level * 50".into()),
                on_advance: None,
                span: span(),
            }],
            pools: vec![PoolDecl {
                name: "health".into(),
                max: Some("max_health".into()),
                regen: Some("2/s when not in_combat".into()),
                cost: None,
                span: span(),
            }],
            stats: vec![
                StatExprDecl {
                    name: "max_health".into(),
                    expression: "50 + strength * 5".into(),
                    span: span(),
                },
                StatExprDecl {
                    name: "damage".into(),
                    expression: "8 + strength * 0.5".into(),
                    span: span(),
                },
            ],
        }
    }

    #[test]
    fn instance_seeds_attributes_and_pools() {
        let profile = StatsProfile::from_body("Combat", &body());
        let world = World::new();
        let inst = StatsInstance::from_profile(&profile, &world);
        assert_eq!(inst.attributes.get("strength"), Some(&10.0));
        let health = inst.pools.get("health").unwrap();
        assert_eq!(health.max, 100.0);
        assert_eq!(health.current, 100.0);
        assert!(health.regen_when_out_of_combat);
    }

    #[test]
    fn stat_depends_on_attribute() {
        let profile = StatsProfile::from_body("Combat", &body());
        let mut inst = StatsInstance::from_profile(&profile, &World::new());
        let v = inst.evaluate_stat("damage", &World::new()).unwrap();
        assert_eq!(v, Value::Number(13.0)); // 8 + 10 * 0.5
        inst.set_attribute("strength", 20.0);
        let v = inst.evaluate_stat("damage", &World::new()).unwrap();
        assert_eq!(v, Value::Number(18.0));
    }

    #[test]
    fn pool_regens_only_out_of_combat() {
        let profile = StatsProfile::from_body("Combat", &body());
        let mut inst = StatsInstance::from_profile(&profile, &World::new());
        let h = inst.pools.get_mut("health").unwrap();
        h.current = 50.0;
        h.tick(3.0, true);
        assert_eq!(h.current, 50.0);
        h.tick(3.0, false);
        assert_eq!(h.current, 56.0); // +2/s * 3s
    }

    #[test]
    fn xp_curve_levels_up_when_threshold_crossed() {
        let profile = StatsProfile::from_body("Combat", &body());
        let mut inst = StatsInstance::from_profile(&profile, &World::new());
        // Curve: level * level * 50 → next-level threshold from 0 = 50.
        let gained = inst.advance_axis("level", 49.0);
        assert_eq!(gained, 0);
        let gained = inst.advance_axis("level", 1.0);
        assert_eq!(gained, 1);
        assert_eq!(inst.axes.get("level").unwrap().level, 1);
    }

    #[test]
    fn tree_unlock_respects_requires() {
        let body = TreeBody {
            nodes: vec![
                TreeNodeDecl {
                    name: "armsman_1".into(),
                    cost: None,
                    requires: None,
                    effects: vec![],
                    span: span(),
                },
                TreeNodeDecl {
                    name: "armsman_2".into(),
                    cost: None,
                    requires: Some("node(armsman_1)".into()),
                    effects: vec![],
                    span: span(),
                },
            ],
        };
        let tree = Tree::from_body("WarriorPath", &body);
        let mut unlocked: BTreeMap<String, bool> = BTreeMap::new();
        let world = World::new();
        assert!(!tree.can_unlock("armsman_2", &unlocked, &world));
        unlocked.insert("armsman_1".into(), true);
        assert!(tree.can_unlock("armsman_2", &unlocked, &world));
        // Already unlocked nodes can't unlock again.
        unlocked.insert("armsman_2".into(), true);
        assert!(!tree.can_unlock("armsman_2", &unlocked, &world));
    }
}
