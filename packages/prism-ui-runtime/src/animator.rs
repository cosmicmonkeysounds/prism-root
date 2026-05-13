//! **Wave 14.3** — `transition:<prop>="<duration>"` animator
//! substrate.
//!
//! Each `<container transition:radius="200ms"/>` (or any prop carrying
//! a numeric value) lowers to a `data-transition-<prop>` semantic
//! attr the runtime cache picks up. The host (typically the shell)
//! drives this [`Animator`] through three calls:
//!
//! 1. [`Animator::observe`] — pre-render: walk the tree and compare
//!    the live prop values against the animator's last-seen
//!    snapshot. Any change on a transition-tagged node starts a new
//!    interpolation seeded with `(from = previous, to = current,
//!    duration = parsed)`. Idempotent — re-observing the same tree
//!    is a no-op.
//! 2. [`Animator::apply`] — mid-render: walk the same tree (or its
//!    lowered children) and rewrite any prop that has an in-flight
//!    transition to the interpolated value at `now_ms`. The tree's
//!    declared end value is preserved on the animator's snapshot so
//!    the *next* observe call doesn't immediately restart the same
//!    transition.
//! 3. [`Animator::needs_redraw`] — post-render: returns `true` while
//!    any transition is still running. The shell maps this to the
//!    same dirty bit the dispatch chain feeds, so the femtovg /
//!    web event loops schedule a follow-up frame without a separate
//!    timer.
//!
//! Easing is selectable per-transition through [`Easing`]; the
//! parser today only recognises a plain duration spec
//! (`"200ms"` / `"1.5s"`), so callers that want a non-linear curve
//! call [`Animator::start_with_easing`] directly.

use std::collections::HashMap;

use crate::layout::{Node, Sizing};

/// In-flight transition state. Stored per `(node-id, prop-key)`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Transition {
    from: f32,
    to: f32,
    started_ms: u64,
    duration_ms: u64,
    easing: Easing,
}

/// Easing curve sampled per frame. Linear is the default; cubic
/// `EaseInOut` matches CSS's classic `ease` curve closely enough for
/// most UI transitions without hand-tuning bezier coefficients.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Easing {
    /// Map a 0..=1 normalised time to the eased 0..=1 progress.
    pub fn ease(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            // Cubic curves — same family CSS uses, no allocations.
            Easing::EaseIn => t * t * t,
            Easing::EaseOut => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u / 2.0
                }
            }
        }
    }
}

/// The animator substrate. Single-threaded — hosts share via
/// `Rc<RefCell<Animator>>`. Keys are `(node-id, prop-key)` strings,
/// where `prop-key` matches the DSL spelling (`opacity` for
/// `transition:opacity`, `radius` for `transition:radius`, etc.).
#[derive(Debug, Default)]
pub struct Animator {
    /// `(node-id, prop-key) → in-flight transition`.
    active: HashMap<(String, String), Transition>,
    /// `(node-id, prop-key) → last-observed declared value`. Used to
    /// detect "the author's declared value changed" deltas at
    /// observe-time. Survives across frames so a transition restarts
    /// only when the declared value actually moves again.
    last_seen: HashMap<(String, String), f32>,
}

impl Animator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Are any transitions still running? Hosts merge this into the
    /// per-frame "request a redraw" bit so the next frame ticks the
    /// animator forward.
    pub fn needs_redraw(&self) -> bool {
        !self.active.is_empty()
    }

    /// Read the current interpolated value for `(node, prop)`.
    /// Returns `None` when no transition is active for the pair.
    /// Inside the duration, the easing curve is applied; after the
    /// duration elapses, the transition self-clears on the next
    /// [`Animator::tick`] and this returns `None`.
    pub fn current(&self, node_id: &str, prop: &str, now_ms: u64) -> Option<f32> {
        let t = self.active.get(&(node_id.to_string(), prop.to_string()))?;
        Some(sample(t, now_ms))
    }

    /// Drop every transition that has run past its duration. Returns
    /// `true` when the set was non-empty *after* the prune (i.e. the
    /// animator still wants a redraw). Hosts call this once per
    /// frame after `apply` so the next frame skips finished
    /// transitions cleanly.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        self.active
            .retain(|_, t| now_ms < t.started_ms + t.duration_ms);
        self.needs_redraw()
    }

    /// Start (or replace) a transition. When one is already in flight
    /// for `(node, prop)`, its current interpolated value becomes the
    /// new transition's `from` so the visual handoff stays smooth —
    /// the classic "interrupt mid-fade" case.
    pub fn start(
        &mut self,
        node_id: &str,
        prop: &str,
        from: f32,
        to: f32,
        duration_ms: u64,
        now_ms: u64,
    ) {
        self.start_with_easing(node_id, prop, from, to, duration_ms, now_ms, Easing::Linear);
    }

    /// Same as [`Animator::start`] with an explicit easing curve.
    #[allow(clippy::too_many_arguments)]
    pub fn start_with_easing(
        &mut self,
        node_id: &str,
        prop: &str,
        from: f32,
        to: f32,
        duration_ms: u64,
        now_ms: u64,
        easing: Easing,
    ) {
        let key = (node_id.to_string(), prop.to_string());
        // If a transition is already running, sample it for a smooth
        // handoff. Otherwise honour the caller's `from`.
        let smooth_from = self
            .active
            .get(&key)
            .map(|t| sample(t, now_ms))
            .unwrap_or(from);
        let duration_ms = duration_ms.max(1);
        self.active.insert(
            key,
            Transition {
                from: smooth_from,
                to,
                started_ms: now_ms,
                duration_ms,
                easing,
            },
        );
    }

    /// Pre-render: walk a node tree, find every transition-tagged
    /// container, and kick off a transition when its declared value
    /// has moved since the last observe. Idempotent on repeated calls
    /// against the same tree.
    ///
    /// Supported props today (all numeric on `ContainerProps`):
    /// `radius`, `gap`, `padding`. Other names round-trip without
    /// starting a transition — the data carries author intent and
    /// the animator picks them up as the prop surface grows.
    pub fn observe(&mut self, nodes: &[Node], now_ms: u64) {
        for node in nodes {
            if let Node::Container {
                id,
                props,
                children,
            } = node
            {
                if !id.is_empty() {
                    for (attr_key, attr_value) in &props.semantic.attrs {
                        let Some(prop) = attr_key.strip_prefix("data-transition-") else {
                            continue;
                        };
                        let Some(declared) = read_numeric_prop(props, prop) else {
                            continue;
                        };
                        let key = (id.clone(), prop.to_string());
                        match self.last_seen.get(&key).copied() {
                            None => {
                                // First time we've seen this pair —
                                // record the value but don't animate
                                // (no "from" exists yet).
                                self.last_seen.insert(key, declared);
                            }
                            Some(prev) if (prev - declared).abs() > f32::EPSILON => {
                                if let Some(duration_ms) = parse_duration_ms(attr_value) {
                                    self.start(id, prop, prev, declared, duration_ms, now_ms);
                                }
                                self.last_seen.insert(key, declared);
                            }
                            Some(_) => {
                                // Unchanged — no-op.
                            }
                        }
                    }
                }
                self.observe(children, now_ms);
            }
        }
    }

    /// Mid-render: walk the tree and rewrite any prop that has a
    /// live transition. Mutates the nodes in place. Call after
    /// `observe` and before painting / hit-test cache build.
    pub fn apply(&self, nodes: &mut [Node], now_ms: u64) {
        for node in nodes.iter_mut() {
            if let Node::Container {
                id,
                props,
                children,
            } = node
            {
                if !id.is_empty() {
                    let keys: Vec<String> = props
                        .semantic
                        .attrs
                        .iter()
                        .filter_map(|(k, _)| k.strip_prefix("data-transition-"))
                        .map(|s| s.to_string())
                        .collect();
                    for prop in keys {
                        let Some(t) = self.active.get(&(id.clone(), prop.clone())) else {
                            continue;
                        };
                        let value = sample(t, now_ms);
                        write_numeric_prop(props, &prop, value);
                    }
                }
                self.apply(children, now_ms);
            }
        }
    }

    /// Number of in-flight transitions. Exposed for tests + the
    /// shell's debug HUD; production callers should prefer
    /// [`Animator::needs_redraw`].
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

/// Sample a transition's value at `now_ms` against its easing curve.
fn sample(t: &Transition, now_ms: u64) -> f32 {
    if now_ms <= t.started_ms {
        return t.from;
    }
    let elapsed = now_ms.saturating_sub(t.started_ms);
    if elapsed >= t.duration_ms {
        return t.to;
    }
    let progress = elapsed as f32 / t.duration_ms as f32;
    let eased = t.easing.ease(progress);
    t.from + (t.to - t.from) * eased
}

/// Parse a CSS-shaped duration: `"200ms"` / `"1.5s"` / `"0.25s"`.
/// Bare numbers parse as milliseconds (`"200"` = 200ms) so the
/// authoring vocabulary stays terse. Returns `None` on malformed
/// input.
pub fn parse_duration_ms(spec: &str) -> Option<u64> {
    let s = spec.trim();
    if let Some(rest) = s.strip_suffix("ms") {
        return rest.trim().parse::<f64>().ok().map(|v| v.max(0.0) as u64);
    }
    if let Some(rest) = s.strip_suffix('s') {
        return rest
            .trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v.max(0.0) * 1000.0) as u64);
    }
    s.parse::<f64>().ok().map(|v| v.max(0.0) as u64)
}

/// Read a numeric prop from `ContainerProps` by name. Only props that
/// support smooth interpolation are listed here. Unknown names return
/// `None` — the animator silently skips.
fn read_numeric_prop(props: &crate::layout::ContainerProps, prop: &str) -> Option<f32> {
    match prop {
        "gap" => Some(props.gap),
        "padding" => Some(props.padding.left),
        "padding-left" => Some(props.padding.left),
        "padding-right" => Some(props.padding.right),
        "padding-top" => Some(props.padding.top),
        "padding-bottom" => Some(props.padding.bottom),
        // `radius` uses the uniform corner — `CornerRadius` carries
        // four corners but the DSL's `style:radius="N"` writes them
        // identically, so sampling top-left tracks author intent.
        // Mixed-corner radii fall outside this animator path; a
        // per-corner transition is a follow-up.
        "radius" => Some(props.radius.tl),
        // Numeric sizing modes (Fixed(n)) animate cleanly; Grow / Fit
        // / Percent skip the animator.
        "width" => match props.width {
            Sizing::Fixed(n) => Some(n),
            _ => None,
        },
        "height" => match props.height {
            Sizing::Fixed(n) => Some(n),
            _ => None,
        },
        _ => None,
    }
}

/// Write an interpolated value back into `ContainerProps`. Sister to
/// [`read_numeric_prop`]; covers exactly the same key set.
fn write_numeric_prop(props: &mut crate::layout::ContainerProps, prop: &str, value: f32) {
    match prop {
        "gap" => props.gap = value,
        "padding" => {
            props.padding.left = value;
            props.padding.right = value;
            props.padding.top = value;
            props.padding.bottom = value;
        }
        "padding-left" => props.padding.left = value,
        "padding-right" => props.padding.right = value,
        "padding-top" => props.padding.top = value,
        "padding-bottom" => props.padding.bottom = value,
        "radius" => {
            props.radius = crate::command::CornerRadius {
                tl: value,
                tr: value,
                br: value,
                bl: value,
            };
        }
        "width" => {
            if matches!(props.width, Sizing::Fixed(_)) {
                props.width = Sizing::Fixed(value);
            }
        }
        "height" => {
            if matches!(props.height, Sizing::Fixed(_)) {
                props.height = Sizing::Fixed(value);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{ContainerProps, Node, Padding, Semantic};

    fn make_container(id: &str, transition: &[(&str, &str)], padding: f32) -> Node {
        let mut semantic = Semantic::default();
        for (k, v) in transition {
            semantic
                .attrs
                .push((format!("data-transition-{}", k), (*v).to_string()));
        }
        Node::Container {
            id: id.into(),
            props: ContainerProps {
                padding: Padding::all(padding),
                semantic,
                ..Default::default()
            },
            children: vec![],
        }
    }

    #[test]
    fn parse_duration_ms_recognises_common_shapes() {
        assert_eq!(parse_duration_ms("200ms"), Some(200));
        assert_eq!(parse_duration_ms("1.5s"), Some(1500));
        assert_eq!(parse_duration_ms("0.25s"), Some(250));
        assert_eq!(parse_duration_ms("100"), Some(100));
        assert_eq!(parse_duration_ms("garbage"), None);
    }

    #[test]
    fn easing_curves_are_monotonic_and_bounded() {
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            let a = easing.ease(0.0);
            let m = easing.ease(0.5);
            let b = easing.ease(1.0);
            assert!((a - 0.0).abs() < 1e-3, "{:?} at 0 == 0", easing);
            assert!((b - 1.0).abs() < 1e-3, "{:?} at 1 == 1", easing);
            assert!(a <= m && m <= b, "{:?} monotonic", easing);
        }
    }

    /// First `observe` call seeds the last-seen snapshot but doesn't
    /// start a transition — there's no "from" yet.
    #[test]
    fn first_observe_records_baseline_without_animating() {
        let mut animator = Animator::new();
        let nodes = vec![make_container("a", &[("padding", "200ms")], 10.0)];
        animator.observe(&nodes, 0);
        assert_eq!(animator.active_count(), 0);
        assert!(!animator.needs_redraw());
    }

    /// Second `observe` against a moved prop kicks off a transition.
    #[test]
    fn observe_after_change_starts_transition() {
        let mut animator = Animator::new();
        animator.observe(&[make_container("a", &[("padding", "200ms")], 10.0)], 0);
        animator.observe(&[make_container("a", &[("padding", "200ms")], 30.0)], 100);
        assert_eq!(animator.active_count(), 1);
        // Mid-flight value lives between from + to.
        let mid = animator.current("a", "padding", 200).unwrap();
        assert!(mid > 10.0 && mid < 30.0, "interpolated mid value: {}", mid);
        // At the end of the duration, the transition is at `to`.
        let done = animator.current("a", "padding", 100 + 200).unwrap();
        assert!((done - 30.0).abs() < 1e-3);
    }

    /// Apply mutates the nodes in place so downstream paint / hit
    /// passes see the eased value.
    #[test]
    fn apply_rewrites_prop_to_interpolated_value() {
        let mut animator = Animator::new();
        animator.start("a", "gap", 0.0, 100.0, 200, 0);
        let mut nodes = vec![Node::Container {
            id: "a".into(),
            props: ContainerProps {
                gap: 0.0,
                semantic: Semantic {
                    attrs: vec![("data-transition-gap".into(), "200ms".into())],
                    ..Default::default()
                },
                ..Default::default()
            },
            children: vec![],
        }];
        animator.apply(&mut nodes, 100);
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(
            (props.gap - 50.0).abs() < 1.0,
            "linear-eased gap at 50% should be ~50, got {}",
            props.gap
        );
    }

    /// `tick` clears transitions whose duration has elapsed and
    /// returns `false` once the set is empty.
    #[test]
    fn tick_clears_finished_transitions() {
        let mut animator = Animator::new();
        animator.start("a", "gap", 0.0, 100.0, 200, 0);
        assert!(animator.tick(100));
        assert!(animator.tick(199));
        assert!(!animator.tick(300));
        assert_eq!(animator.active_count(), 0);
    }

    /// Restarting an in-flight transition snaps the new `from` to
    /// the current eased value so the visual handoff stays smooth
    /// (no "snap to declared from" on every fresh `observe`).
    #[test]
    fn restarting_mid_flight_preserves_smooth_handoff() {
        let mut animator = Animator::new();
        animator.start("a", "gap", 0.0, 100.0, 200, 0);
        let mid = animator.current("a", "gap", 100).unwrap();
        animator.start("a", "gap", 999.0, 50.0, 200, 100);
        // The new transition's effective `from` should equal the
        // sampled mid value (not 999.0). At t=100ms after restart,
        // the new transition is at its declared mid-eased value.
        let immediately_after = animator.current("a", "gap", 100).unwrap();
        assert!(
            (immediately_after - mid).abs() < 1e-3,
            "handoff: expected ~{}, got {}",
            mid,
            immediately_after
        );
    }
}
