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

use std::collections::{HashMap, HashSet};

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

/// Author-declared out-transition spec for one `(node, prop)`. Mirrors
/// the parse result of `data-animate-out-<prop>="<to> <duration>"` and
/// drives the eased interpolation kicked off when the node disappears
/// from the tree.
#[derive(Debug, Clone, Copy, PartialEq)]
struct OutSpec {
    to: f32,
    duration_ms: u64,
}

/// Last-known snapshot of an out-tagged node plus the id of the
/// container it lived under at observe time. The shell uses
/// `parent_id` to graft a phantom back into the same child list on
/// later frames; `None` means the snapshot was at the root.
#[derive(Debug, Clone)]
struct NodeSnapshot {
    node: Node,
    parent_id: Option<String>,
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
    /// Wave 14.8 — `(node-id, prop-key) → author-declared out spec`.
    /// Captured every observe a node carrying `data-animate-out-*` is
    /// present. When the same `node-id` is missing from a later
    /// observe (i.e. the host rebuilt the tree without it), every
    /// pending out-spec fires a one-shot transition from the current
    /// prop value to the declared `to`, and the node's last-known
    /// snapshot graduates to a phantom that the painter keeps
    /// rendering until the transitions complete.
    out_pending: HashMap<(String, String), OutSpec>,
    /// Last-known node snapshot per id with at least one
    /// `data-animate-out-*` attr. Carries the full container subtree
    /// so a vanishing node keeps painting with its content intact.
    /// The `parent_id` slot remembers which container the snapshot
    /// lived under so the shell can graft phantoms back into the
    /// same child list — `None` means the snapshot was a root-level
    /// sibling.
    node_snapshots: HashMap<String, NodeSnapshot>,
    /// Snapshots whose owning node has left the tree and whose
    /// out-transitions are mid-flight. Painter calls
    /// [`Animator::phantom_nodes`] each frame to graft them on top of
    /// the live tree until [`Animator::tick`] drains them.
    phantoms: HashMap<String, NodeSnapshot>,
}

impl Animator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Are any transitions still running? Hosts merge this into the
    /// per-frame "request a redraw" bit so the next frame ticks the
    /// animator forward.
    pub fn needs_redraw(&self) -> bool {
        !self.active.is_empty() || !self.phantoms.is_empty()
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
        // Wave 14.8 — phantom nodes survive only as long as at least
        // one transition keyed on their id is still active. Once
        // every out-prop has finished interpolating, the phantom is
        // dropped and the painter sees the empty slot the host
        // already produced.
        let live_ids: HashSet<String> = self.active.keys().map(|(id, _)| id.clone()).collect();
        self.phantoms.retain(|id, _| live_ids.contains(id));
        self.needs_redraw() || !self.phantoms.is_empty()
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
    ///
    /// Wave 14.8 — also accumulates the set of ids present in the
    /// tree and converts the *missing* ones (any id previously seen
    /// with an `animate:out-*` attr) into phantom subtrees with
    /// out-transitions firing on every declared prop.
    pub fn observe(&mut self, nodes: &[Node], now_ms: u64) {
        let mut present: HashSet<String> = HashSet::new();
        self.observe_into(nodes, now_ms, &mut present, None);
        self.commit_unmounts(&present, now_ms);
    }

    /// Recursive observe helper that accumulates ids into `present`
    /// so the top-level [`Animator::observe`] can detect unmounts.
    /// `parent_id` carries the closest enclosing container id (or
    /// `None` at the root). Out-tagged snapshots store this so the
    /// shell can graft phantoms back into the same child list once
    /// they've left the tree.
    fn observe_into(
        &mut self,
        nodes: &[Node],
        now_ms: u64,
        present: &mut HashSet<String>,
        parent_id: Option<&str>,
    ) {
        for node in nodes {
            if let Node::Container {
                id,
                props,
                children,
            } = node
            {
                if !id.is_empty() {
                    present.insert(id.clone());
                    let mut has_out_spec = false;
                    for (attr_key, attr_value) in &props.semantic.attrs {
                        if let Some(prop) = attr_key.strip_prefix("data-animate-out-") {
                            let Some((to, duration_ms)) = parse_animate_in_value(attr_value) else {
                                continue;
                            };
                            self.out_pending.insert(
                                (id.clone(), prop.to_string()),
                                OutSpec { to, duration_ms },
                            );
                            has_out_spec = true;
                        }
                    }
                    if has_out_spec {
                        // Snapshot is the most recent representation
                        // of the node — phantoms graft from here
                        // when the host's next observe doesn't see
                        // this id. `parent_id` records the container
                        // it lived under so the shell can graft
                        // back into the same child list.
                        self.node_snapshots.insert(
                            id.clone(),
                            NodeSnapshot {
                                node: node.clone(),
                                parent_id: parent_id.map(|s| s.to_owned()),
                            },
                        );
                    }
                    for (attr_key, attr_value) in &props.semantic.attrs {
                        // Wave-deferred follow-up — `data-animate-in-<prop>`
                        // carries `<from> <duration>` and triggers a
                        // transition on the FIRST observation of the
                        // node (the "this element is appearing" case).
                        // Skipped on every subsequent observe so the
                        // entry animation runs once per mount lifecycle.
                        // `data-transition-<prop>` keeps owning the
                        // mid-life "the author's declared value moved"
                        // case below.
                        if let Some(prop) = attr_key.strip_prefix("data-animate-in-") {
                            let Some(declared) = read_numeric_prop(props, prop) else {
                                continue;
                            };
                            let key = (id.clone(), prop.to_string());
                            if self.last_seen.contains_key(&key) {
                                continue;
                            }
                            if let Some((from, duration_ms)) = parse_animate_in_value(attr_value) {
                                self.start(id, prop, from, declared, duration_ms, now_ms);
                            }
                            // Seed last_seen so the subsequent transition
                            // delta-detection path doesn't re-fire on
                            // the same value next observe.
                            self.last_seen.insert(key, declared);
                            continue;
                        }
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
                let next_parent = if id.is_empty() {
                    parent_id
                } else {
                    Some(id.as_str())
                };
                self.observe_into(children, now_ms, present, next_parent);
            }
        }
    }

    /// Wave 14.8 — fire out-transitions for any snapshot whose id is
    /// missing from `present`. Each pending out-spec for that id
    /// kicks off a transition from the snapshot's current value to
    /// the declared `to`; the snapshot graduates to a phantom for
    /// the painter to keep rendering.
    fn commit_unmounts(&mut self, present: &HashSet<String>, now_ms: u64) {
        let unmounted_ids: Vec<String> = self
            .node_snapshots
            .keys()
            .filter(|id| !present.contains(id.as_str()))
            .cloned()
            .collect();
        for id in unmounted_ids {
            let Some(snapshot) = self.node_snapshots.remove(&id) else {
                continue;
            };
            let Node::Container { props, .. } = &snapshot.node else {
                continue;
            };
            let prop_specs: Vec<(String, OutSpec)> = self
                .out_pending
                .iter()
                .filter(|((sid, _), _)| sid == &id)
                .map(|((_, prop), spec)| (prop.clone(), *spec))
                .collect();
            for (prop, spec) in prop_specs {
                let from = read_numeric_prop(props, &prop).unwrap_or(spec.to);
                self.start(&id, &prop, from, spec.to, spec.duration_ms, now_ms);
            }
            // Drop the spec entries so a fresh mount of the same id
            // doesn't carry stale out-state.
            self.out_pending.retain(|(sid, _), _| sid != &id);
            self.phantoms.insert(id, snapshot);
        }
    }

    /// Wave 14.8 — phantom subtrees the painter should graft on top
    /// of the live tree. Each phantom has its eased-prop values
    /// already written in for `now_ms`; the host can paint the
    /// returned nodes verbatim. Returns `Vec<Node>` rather than a
    /// borrow because the caller typically extends a render list.
    ///
    /// Drops the parent-id hint — callers that need it (the shell's
    /// in-place graft path) use
    /// [`Animator::phantom_nodes_with_parent`] instead.
    pub fn phantom_nodes(&self, now_ms: u64) -> Vec<Node> {
        self.phantom_nodes_with_parent(now_ms)
            .into_iter()
            .map(|(_, node)| node)
            .collect()
    }

    /// Wave 14.8 — same as [`Animator::phantom_nodes`] but pairs
    /// each phantom with the id of the container it lived under at
    /// observe time. `None` means the snapshot was a root-level
    /// sibling. Hosts that want in-place graft walk the live tree,
    /// look up each `parent_id`, and append the phantom into that
    /// container's children list; phantoms whose parent has also
    /// left the tree fall through to the root.
    pub fn phantom_nodes_with_parent(&self, now_ms: u64) -> Vec<(Option<String>, Node)> {
        self.phantoms
            .values()
            .map(|snapshot| {
                let mut node = snapshot.node.clone();
                let single = std::slice::from_mut(&mut node);
                self.apply(single, now_ms);
                (snapshot.parent_id.clone(), node)
            })
            .collect()
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
                    // Collect every prop that has either a
                    // `data-transition-<prop>` (mid-life delta
                    // transition) or `data-animate-in-<prop>` (entry
                    // transition) author hint. Both feed the same
                    // `self.active` map so a single sample-and-write
                    // path covers both.
                    let keys: Vec<String> = props
                        .semantic
                        .attrs
                        .iter()
                        .filter_map(|(k, _)| {
                            k.strip_prefix("data-transition-")
                                .or_else(|| k.strip_prefix("data-animate-in-"))
                                .or_else(|| k.strip_prefix("data-animate-out-"))
                        })
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

/// Parse a `data-animate-in-<prop>` attribute value of the shape
/// `"<from> <duration>"`. Returns `Some((from, duration_ms))` when
/// both halves parse cleanly; `None` otherwise (silently skipped by
/// `observe`). Mirrors the DSL substrate `animate:in="<prop> <from>
/// <duration>"` after the prop has been peeled into the attr name.
pub fn parse_animate_in_value(spec: &str) -> Option<(f32, u64)> {
    let trimmed = spec.trim();
    let mut parts = trimmed.split_whitespace();
    let from = parts.next()?.parse::<f32>().ok()?;
    let duration_ms = parse_duration_ms(parts.next()?)?;
    Some((from, duration_ms))
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
        // `opacity` is the canonical `animate:in` channel. Defaults
        // to 1.0 when the author didn't set it explicitly so the
        // "fade in to natural state" pattern works without a
        // resting opacity literal on every node.
        "opacity" => Some(props.opacity.unwrap_or(1.0)),
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
        "opacity" => props.opacity = Some(value.clamp(0.0, 1.0)),
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

    /// Wave 14.6 — `parse_animate_in_value("<from> <duration>")`
    /// returns the parsed `(from, duration_ms)` pair. Malformed
    /// inputs (single token, non-numeric `from`, unparseable
    /// duration) return `None` so `observe` silently skips.
    #[test]
    fn parse_animate_in_value_recognises_from_and_duration() {
        assert_eq!(parse_animate_in_value("0 200ms"), Some((0.0, 200)));
        assert_eq!(parse_animate_in_value("0.5 1.5s"), Some((0.5, 1500)));
        assert_eq!(parse_animate_in_value("  -8  250  "), Some((-8.0, 250)));
        assert!(parse_animate_in_value("alone").is_none());
        assert!(parse_animate_in_value("0 garbage").is_none());
        assert!(parse_animate_in_value("not-a-number 200ms").is_none());
    }

    /// Wave 14.6 — `data-animate-in-<prop>` triggers a transition
    /// on the first observation of the node, interpolating from
    /// the parsed `<from>` to the prop's declared value. Subsequent
    /// observations do NOT re-fire the entry (one-shot per mount).
    #[test]
    fn animate_in_starts_transition_on_first_observe() {
        let mut animator = Animator::new();
        let mut semantic = Semantic::default();
        semantic
            .attrs
            .push(("data-animate-in-padding".into(), "0 200ms".into()));
        let node = Node::Container {
            id: "toast".into(),
            props: ContainerProps {
                padding: Padding::all(12.0),
                semantic,
                ..Default::default()
            },
            children: vec![],
        };
        animator.observe(std::slice::from_ref(&node), 0);
        // Mid-flight: sample at half-duration should land halfway.
        let sample_mid = animator.current("toast", "padding", 100);
        assert!(sample_mid.is_some());
        let v = sample_mid.unwrap();
        assert!((v - 6.0).abs() < 0.5, "got {v}");
        // Second observe should NOT re-fire the entry transition.
        animator.tick(300);
        animator.observe(std::slice::from_ref(&node), 300);
        assert_eq!(animator.active_count(), 0, "entry fires once per mount");
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

    /// Wave 14.8 — `data-animate-out-<prop>` triggers an out
    /// transition when the node disappears between observes. The
    /// snapshot graduates to a phantom whose props the painter can
    /// keep rendering until the transition completes.
    #[test]
    fn animate_out_fires_transition_when_node_disappears() {
        let mut animator = Animator::new();
        let mut semantic = Semantic::default();
        semantic
            .attrs
            .push(("data-animate-out-opacity".into(), "0 200ms".into()));
        let node = Node::Container {
            id: "toast".into(),
            props: ContainerProps {
                opacity: Some(1.0),
                semantic,
                ..Default::default()
            },
            children: vec![],
        };
        // First observe — snapshot only, no transition yet.
        animator.observe(std::slice::from_ref(&node), 0);
        assert_eq!(animator.active_count(), 0);
        assert!(animator.phantom_nodes(0).is_empty());
        // Second observe — the node has vanished, so an out
        // transition kicks off and a phantom appears.
        animator.observe(&[], 100);
        assert_eq!(animator.active_count(), 1);
        let phantoms = animator.phantom_nodes(150);
        assert_eq!(phantoms.len(), 1);
        let Node::Container { id, props, .. } = &phantoms[0] else {
            panic!("phantom must keep its container shape")
        };
        assert_eq!(id, "toast");
        // Mid-flight: opacity has been eased downward.
        let mid = props.opacity.unwrap_or(1.0);
        assert!(mid < 1.0 && mid > 0.0, "mid-fade opacity: {mid}");
    }

    /// Wave 14.8 — once every out-transition for a phantom has
    /// finished, `tick` drains the phantom and `needs_redraw`
    /// returns false again. The painter sees the empty slot the
    /// host already produced.
    #[test]
    fn animate_out_phantom_drains_after_transition_finishes() {
        let mut animator = Animator::new();
        let mut semantic = Semantic::default();
        semantic
            .attrs
            .push(("data-animate-out-opacity".into(), "0 200ms".into()));
        let node = Node::Container {
            id: "toast".into(),
            props: ContainerProps {
                opacity: Some(1.0),
                semantic,
                ..Default::default()
            },
            children: vec![],
        };
        animator.observe(std::slice::from_ref(&node), 0);
        animator.observe(&[], 100);
        assert!(animator.needs_redraw());
        // Halfway: phantom still alive.
        animator.tick(200);
        assert_eq!(animator.phantom_nodes(200).len(), 1);
        // Past the end of the 200ms duration: tick drains.
        let still_dirty = animator.tick(500);
        assert!(!still_dirty);
        assert!(animator.phantom_nodes(500).is_empty());
        assert!(!animator.needs_redraw());
    }

    /// Wave 14.8 — a phantom node carries its full child subtree so
    /// the painter doesn't need to reconstruct content from a
    /// vanished parent. Nested containers ride through the snapshot.
    #[test]
    fn animate_out_phantom_preserves_child_subtree() {
        let mut animator = Animator::new();
        let mut semantic = Semantic::default();
        semantic
            .attrs
            .push(("data-animate-out-opacity".into(), "0 200ms".into()));
        let child = Node::Container {
            id: "toast-body".into(),
            props: ContainerProps::default(),
            children: vec![],
        };
        let node = Node::Container {
            id: "toast".into(),
            props: ContainerProps {
                opacity: Some(1.0),
                semantic,
                ..Default::default()
            },
            children: vec![child.clone()],
        };
        animator.observe(std::slice::from_ref(&node), 0);
        animator.observe(&[], 100);
        let phantoms = animator.phantom_nodes(150);
        let Node::Container { children, .. } = &phantoms[0] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        if let Node::Container { id, .. } = &children[0] {
            assert_eq!(id, "toast-body");
        } else {
            panic!("child must remain a container");
        }
    }

    /// Wave 14.8 — phantom snapshots remember their parent container
    /// id so the shell can graft phantoms back into the same child
    /// list (instead of dumping every phantom at the root). Nested
    /// out-tagged nodes carry the enclosing container id; root-level
    /// ones carry `None`.
    #[test]
    fn animate_out_phantom_carries_parent_id_through_unmount() {
        let mut animator = Animator::new();
        let mut out_attrs = Semantic::default();
        out_attrs
            .attrs
            .push(("data-animate-out-opacity".into(), "0 200ms".into()));
        let inner = Node::Container {
            id: "toast".into(),
            props: ContainerProps {
                opacity: Some(1.0),
                semantic: out_attrs,
                ..Default::default()
            },
            children: vec![],
        };
        let parent = Node::Container {
            id: "toast-stack".into(),
            props: ContainerProps::default(),
            children: vec![inner],
        };
        animator.observe(std::slice::from_ref(&parent), 0);
        // Unmount the toast — parent stays in the tree.
        let bare_parent = Node::Container {
            id: "toast-stack".into(),
            props: ContainerProps::default(),
            children: vec![],
        };
        animator.observe(std::slice::from_ref(&bare_parent), 100);
        let pairs = animator.phantom_nodes_with_parent(150);
        assert_eq!(pairs.len(), 1);
        let (parent_id, _phantom) = &pairs[0];
        assert_eq!(parent_id.as_deref(), Some("toast-stack"));
    }

    /// Wave 14.8 — a phantom whose parent vanished alongside it
    /// (e.g. an entire workspace section collapsed) carries the
    /// disappearing parent's id verbatim. The shell graft falls
    /// back to the root tree when the parent isn't in the live
    /// tree any more — but the data itself round-trips so any
    /// future "graft into nearest surviving ancestor" path has the
    /// trail to walk.
    #[test]
    fn animate_out_phantom_at_root_carries_no_parent() {
        let mut animator = Animator::new();
        let mut out_attrs = Semantic::default();
        out_attrs
            .attrs
            .push(("data-animate-out-opacity".into(), "0 200ms".into()));
        let root_level = Node::Container {
            id: "toast".into(),
            props: ContainerProps {
                opacity: Some(1.0),
                semantic: out_attrs,
                ..Default::default()
            },
            children: vec![],
        };
        animator.observe(std::slice::from_ref(&root_level), 0);
        animator.observe(&[], 100);
        let pairs = animator.phantom_nodes_with_parent(150);
        assert_eq!(pairs.len(), 1);
        let (parent_id, _phantom) = &pairs[0];
        assert_eq!(parent_id.as_deref(), None);
    }

    /// Wave 14.8 — re-mounting a previously phantom id wipes the
    /// snapshot and pending spec so a future unmount measures
    /// against the *new* mount, not the old one.
    #[test]
    fn animate_out_re_mount_replaces_snapshot_and_spec() {
        let mut animator = Animator::new();
        let mut semantic = Semantic::default();
        semantic
            .attrs
            .push(("data-animate-out-opacity".into(), "0 200ms".into()));
        let node = Node::Container {
            id: "toast".into(),
            props: ContainerProps {
                opacity: Some(1.0),
                semantic,
                ..Default::default()
            },
            children: vec![],
        };
        animator.observe(std::slice::from_ref(&node), 0);
        // Re-observe the same id with a different opacity — snapshot
        // refreshes, no out-transition fires.
        let refreshed = if let Node::Container { id, props, .. } = &node {
            let mut p = props.clone();
            p.opacity = Some(0.5);
            Node::Container {
                id: id.clone(),
                props: p,
                children: vec![],
            }
        } else {
            unreachable!()
        };
        animator.observe(std::slice::from_ref(&refreshed), 50);
        assert_eq!(animator.active_count(), 0);
        // Now disappear — the from value reflects the refreshed
        // opacity, not the original.
        animator.observe(&[], 100);
        let phantoms = animator.phantom_nodes(100);
        assert_eq!(phantoms.len(), 1);
        let Node::Container { props, .. } = &phantoms[0] else {
            panic!()
        };
        // At t=0 of the new transition, opacity is the snapshot's
        // refreshed value (0.5), not the original 1.0.
        assert!((props.opacity.unwrap_or(0.0) - 0.5).abs() < 1e-3);
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
