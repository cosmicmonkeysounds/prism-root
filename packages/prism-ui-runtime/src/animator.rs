//! **§7.15** — unified [`Animator`] substrate for the three call
//! surfaces the roadmap merges into a single trait-shape:
//!
//! 1. **Mid-life value change** — `transition:<prop>="<duration>"`
//!    (or the canonical Phase-15 pipeline form,
//!    `style.<prop>={ rest | :hover → 0.6 over 200ms }`). The
//!    prop's *declared* value moves between frames and the animator
//!    eases the visual value across the gap.
//! 2. **Entry / exit transitions** — `animate:in-<prop>` /
//!    `animate:out-<prop>` (canonical Phase-15 pipeline form,
//!    `style.<prop>={ rest | :entry → from 0 over 200ms |
//!    :exit → to 0 over 150ms }`). Fires on the first observe a
//!    node carrying the entry hint becomes part of the tree, and
//!    fires again the frame the node leaves the tree.
//! 3. **Keyframe stops** — `animator:keyframes="<spec>"` (canonical
//!    Phase-15 nested record form `animator.keyframes={ duration =
//!    800ms, 0% = {…}, 50% = {…}, 100% = {…} }`). Multi-stop
//!    timelines that interpolate along the declared curve through a
//!    sequence of waypoints.
//!
//! All three are the same `(from, to, started_ms, duration_ms,
//! easing)` shape underneath; the three surfaces only differ in how
//! the `(from, to)` pair is discovered. The unified `Animator` is
//! what Q3 (§9 of the roadmap) resolves to: ship now, one substrate,
//! three idiomatic shapes — no more Tier-3 "later wave" namespace
//! deferral.
//!
//! Hosts (typically the shell) drive the animator through three
//! calls:
//!
//! 1. [`Animator::observe`] — pre-render: walk the tree, kick off
//!    transitions seeded from the matching surface (declared-value
//!    delta, entry attr, keyframe spec). Re-observing the same tree
//!    is idempotent.
//! 2. [`Animator::apply`] — mid-render: walk the tree (or its
//!    lowered children) and rewrite any prop that has an in-flight
//!    transition to the interpolated value at `now_ms`.
//! 3. [`Animator::needs_redraw`] — post-render: returns `true` while
//!    any transition is still running. The shell maps this to the
//!    same dirty bit the dispatch chain feeds, so the femtovg /
//!    web event loops schedule a follow-up frame without a separate
//!    timer.
//!
//! Easing is selectable per-transition through [`Easing`]; the
//! parser today only recognises a plain duration spec
//! (`"200ms"` / `"1.5s"`), so callers that want a non-linear curve
//! call [`Animator::start_with_easing`] directly. Colour-typed
//! interpolations route through OKLab (via
//! [`crate::interpret::color::lerp_command_color`]) so a fade
//! between vivid hues passes through the perceptual gamut instead
//! of the muddy sRGB midpoint.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::command::Color;
use crate::interpret::color::lerp_command_color;
use crate::layout::{Node, Sizing};

/// In-flight numeric transition state. Stored per `(node-id, prop-key)`.
#[derive(Debug, Clone, PartialEq)]
struct Transition {
    from: f32,
    to: f32,
    started_ms: u64,
    duration_ms: u64,
    easing: Easing,
}

/// **§7.15** — in-flight colour transition. Sibling of [`Transition`]
/// for props whose value is a [`Color`] (background, foreground,
/// tint). Lerps in OKLab via [`lerp_command_color`] so a fade between
/// saturated hues passes through the perceptual gamut.
#[derive(Debug, Clone, PartialEq)]
struct ColorTransition {
    from: Color,
    to: Color,
    started_ms: u64,
    duration_ms: u64,
    easing: Easing,
}

/// **§7.15** — multi-stop keyframe timeline. `stops` is normalised at
/// parse time to `t ∈ [0, 1]` and sorted ascending. The animator
/// interpolates the prop's value piecewise between adjacent stops,
/// applying the per-segment easing curve in the same shape the
/// per-transition path uses.
///
/// Keyframes share the eased-progress concept with the value /
/// entry / exit paths but discover `(from, to)` from the declared
/// stops rather than from a tree delta. A `Keyframes` entry is keyed
/// the same way as a `Transition` (`(node-id, prop-key)`) so the
/// animator's apply pass can see both without divergent dispatch.
#[derive(Debug, Clone, PartialEq)]
struct Keyframes {
    /// Numeric stops, sorted by `t` ascending. `t ∈ [0, 1]`.
    numeric_stops: Vec<(f32, f32)>,
    /// Colour stops in the same shape — keyed off `t ∈ [0, 1]`.
    /// Empty for non-colour props.
    color_stops: Vec<(f32, Color)>,
    started_ms: u64,
    duration_ms: u64,
    easing: Easing,
}

/// Easing curve sampled per frame. Linear is the default; cubic
/// `EaseInOut` matches CSS's classic `ease` curve closely enough for
/// most UI transitions without hand-tuning bezier coefficients.
///
/// **§4.1 (`prism-cross-cutting-systems.md`)** — `Lut` carries a
/// Luau easing closure that was *sampled at lowering time* (where the
/// per-document Lua frame lives) into an equally-spaced normalised
/// lookup table over `t ∈ [0,1]`. The animator interpolates the LUT
/// linearly per frame, so a custom `transition:easing={\fn(t) … end}`
/// curve costs zero Lua calls per tick and the animator never holds a
/// Lua handle across frames (the closure could outlive the VM).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// `N ≥ 2` equally-spaced samples of a custom easing curve over
    /// `t ∈ [0,1]` (`samples[0]` = curve at 0, `samples[N-1]` = curve
    /// at 1). Built by [`Easing::from_samples`]; fewer than two
    /// samples degrade to [`Easing::Linear`] so a malformed closure
    /// never breaks motion.
    Lut(Rc<[f32]>),
}

impl Easing {
    /// Build a sampled-LUT easing from a Luau closure's outputs. Fewer
    /// than two samples is not a usable curve — fall back to linear
    /// (the animator's contract: a bad easing degrades, never panics).
    pub fn from_samples(samples: impl Into<Rc<[f32]>>) -> Self {
        let samples = samples.into();
        if samples.len() < 2 {
            Easing::Linear
        } else {
            Easing::Lut(samples)
        }
    }

    /// Map a 0..=1 normalised time to the eased 0..=1 progress.
    pub fn ease(&self, t: f32) -> f32 {
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
            // Linearly interpolate between the two bracketing LUT
            // samples. The closure already ran once at lowering time;
            // this is pure arithmetic on the cached table.
            Easing::Lut(samples) => {
                let n = samples.len();
                let scaled = t * (n - 1) as f32;
                let i = scaled.floor() as usize;
                if i >= n - 1 {
                    return samples[n - 1];
                }
                let frac = scaled - i as f32;
                samples[i] + (samples[i + 1] - samples[i]) * frac
            }
        }
    }
}

/// Parse a `data-transition-easing` attribute value into an
/// [`Easing`]. Two encodings round-trip from the DSL lowering
/// (`interpret.rs`):
///
/// - a **named keyword** (`linear` / `ease-in` / `ease-out` /
///   `ease-in-out` / `ease`) → the matching builtin curve;
/// - a **comma-separated float list** (`0,0.02,0.09,…,1`) → a sampled
///   Luau easing closure, decoded into [`Easing::Lut`].
///
/// Anything unrecognised degrades to [`Easing::Linear`] — the
/// animator never fails closed on a malformed easing hint.
pub fn parse_easing(spec: &str) -> Easing {
    let s = spec.trim();
    if s.is_empty() {
        return Easing::Linear;
    }
    if s.contains(',') {
        let samples: Vec<f32> = s
            .split(',')
            .filter_map(|p| p.trim().parse::<f32>().ok())
            .collect();
        return Easing::from_samples(samples);
    }
    match s.to_ascii_lowercase().replace('_', "-").as_str() {
        "linear" => Easing::Linear,
        "ease-in" | "easein" => Easing::EaseIn,
        "ease-out" | "easeout" => Easing::EaseOut,
        "ease-in-out" | "easeinout" | "ease" => Easing::EaseInOut,
        _ => Easing::Linear,
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

/// **§7.15** — unified animator substrate. Single-threaded — hosts
/// share via `Rc<RefCell<Animator>>`. Keys are `(node-id, prop-key)`
/// strings, where `prop-key` matches the DSL spelling (`opacity` for
/// `transition:opacity`, `radius` for `transition:radius`,
/// `background` for a colour-keyed `transition:background`, etc.).
///
/// Three call surfaces share the same active-transition map:
///
/// - numeric / colour deltas (`data-transition-*`),
/// - entry / exit transitions (`data-animate-in-*` /
///   `data-animate-out-*`, also dispatched from
///   `style:<prop>:entry` / `style:<prop>:exit` via the §7.7
///   pseudo-state runtime),
/// - keyframe stops (`data-animator-keyframes`).
#[derive(Debug, Default)]
pub struct Animator {
    /// `(node-id, prop-key) → in-flight numeric transition`.
    active: HashMap<(String, String), Transition>,
    /// `(node-id, prop-key) → in-flight colour transition`. Sibling
    /// of [`Self::active`] for colour-typed props; same lifecycle.
    active_color: HashMap<(String, String), ColorTransition>,
    /// `(node-id, prop-key) → keyframe timeline`. Each entry runs
    /// the full multi-stop interpolation between waypoints; the
    /// `apply` pass samples whichever segment `now_ms` lands in.
    active_keyframes: HashMap<(String, String), Keyframes>,
    /// `(node-id, prop-key) → last-observed declared value`. Used to
    /// detect "the author's declared value changed" deltas at
    /// observe-time. Survives across frames so a transition restarts
    /// only when the declared value actually moves again.
    last_seen: HashMap<(String, String), f32>,
    /// **§7.15** — sibling of [`Self::last_seen`] for colour props.
    last_seen_color: HashMap<(String, String), Color>,
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
    /// **§7.15** — node ids the animator has already observed at
    /// least once. Used to gate keyframe install (fires once per
    /// mount lifecycle) symmetrically with `data-animate-in-*`'s
    /// entry-once rule.
    seen_ids: HashSet<String>,
}

impl Animator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Are any transitions still running? Hosts merge this into the
    /// per-frame "request a redraw" bit so the next frame ticks the
    /// animator forward. **§7.15** — covers the three surfaces:
    /// numeric, colour, and keyframe.
    pub fn needs_redraw(&self) -> bool {
        !self.active.is_empty()
            || !self.active_color.is_empty()
            || !self.active_keyframes.is_empty()
            || !self.phantoms.is_empty()
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
    /// transitions cleanly. **§7.15** — prunes the three surfaces
    /// uniformly.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        self.active
            .retain(|_, t| now_ms < t.started_ms + t.duration_ms);
        self.active_color
            .retain(|_, t| now_ms < t.started_ms + t.duration_ms);
        self.active_keyframes
            .retain(|_, k| now_ms < k.started_ms + k.duration_ms);
        // Wave 14.8 — phantom nodes survive only as long as at least
        // one transition keyed on their id is still active. Once
        // every out-prop has finished interpolating, the phantom is
        // dropped and the painter sees the empty slot the host
        // already produced. Numeric and colour out-transitions both
        // count toward "still draining" — same lifecycle.
        let live_ids: HashSet<String> = self
            .active
            .keys()
            .map(|(id, _)| id.clone())
            .chain(self.active_color.keys().map(|(id, _)| id.clone()))
            .collect();
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

    /// **§7.15** — start (or replace) a colour transition for
    /// `(node, prop)`. Same smooth-handoff semantics as
    /// [`Animator::start_with_easing`]: if a colour transition is
    /// already running for the same key, its currently-sampled
    /// colour becomes the new `from`, so an "interrupt mid-fade"
    /// doesn't snap.
    #[allow(clippy::too_many_arguments)]
    pub fn start_color(
        &mut self,
        node_id: &str,
        prop: &str,
        from: Color,
        to: Color,
        duration_ms: u64,
        now_ms: u64,
        easing: Easing,
    ) {
        let key = (node_id.to_string(), prop.to_string());
        let smooth_from = self
            .active_color
            .get(&key)
            .map(|t| sample_color(t, now_ms))
            .unwrap_or(from);
        let duration_ms = duration_ms.max(1);
        self.active_color.insert(
            key,
            ColorTransition {
                from: smooth_from,
                to,
                started_ms: now_ms,
                duration_ms,
                easing,
            },
        );
    }

    /// **§7.15** — read the currently-interpolated colour for
    /// `(node, prop)`. Returns `None` when no colour transition is
    /// active for the pair.
    pub fn current_color(&self, node_id: &str, prop: &str, now_ms: u64) -> Option<Color> {
        let t = self
            .active_color
            .get(&(node_id.to_string(), prop.to_string()))?;
        Some(sample_color(t, now_ms))
    }

    /// **§7.15** — install a keyframe timeline for `(node, prop)`.
    /// Stops are normalised to `t ∈ [0, 1]` and sorted ascending; an
    /// empty stop list is a no-op (the animator silently skips,
    /// mirroring the rest of the substrate's "degrade, never panic"
    /// rule). Replaces any existing timeline for the same key.
    pub fn start_keyframes(&mut self, node_id: &str, prop: &str, spec: KeyframesSpec, now_ms: u64) {
        let duration_ms = spec.duration_ms.max(1);
        if spec.numeric_stops.is_empty() && spec.color_stops.is_empty() {
            return;
        }
        let mut numeric_stops = spec.numeric_stops;
        let mut color_stops = spec.color_stops;
        numeric_stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        color_stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let key = (node_id.to_string(), prop.to_string());
        self.active_keyframes.insert(
            key,
            Keyframes {
                numeric_stops,
                color_stops,
                started_ms: now_ms,
                duration_ms,
                easing: spec.easing,
            },
        );
    }

    /// **§7.15** — read the keyframe-interpolated numeric value for
    /// `(node, prop)` at `now_ms`. `None` when no timeline is active
    /// or the prop's stop set is empty.
    pub fn keyframe_numeric(&self, node_id: &str, prop: &str, now_ms: u64) -> Option<f32> {
        let k = self
            .active_keyframes
            .get(&(node_id.to_string(), prop.to_string()))?;
        sample_keyframes_numeric(k, now_ms)
    }

    /// **§7.15** — read the keyframe-interpolated colour for
    /// `(node, prop)` at `now_ms`. `None` when no timeline is active
    /// or the prop's colour-stop set is empty.
    pub fn keyframe_color(&self, node_id: &str, prop: &str, now_ms: u64) -> Option<Color> {
        let k = self
            .active_keyframes
            .get(&(node_id.to_string(), prop.to_string()))?;
        sample_keyframes_color(k, now_ms)
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
                    // §4.1 — a single `data-transition-easing` hint
                    // governs every animated prop on this container
                    // (entry, mid-life delta, and out transitions),
                    // mirroring CSS's per-element `transition-timing-function`.
                    let easing = node_easing(props);
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
                    // Track whether this is the first observe of
                    // this id so the keyframe install fires once
                    // per mount lifecycle (same rule as
                    // `data-animate-in-*`).
                    let first_observe = !self.seen_ids.contains(id);
                    for (attr_key, attr_value) in &props.semantic.attrs {
                        // **§7.15** — `data-animator-keyframes`
                        // installs a multi-stop timeline on first
                        // observation of the node. Each referenced
                        // prop gets its own `(node-id, prop)` entry
                        // in `active_keyframes` so the apply pass
                        // can sample each independently.
                        if attr_key == "data-animator-keyframes" {
                            if !first_observe {
                                continue;
                            }
                            for (prop, mut spec) in parse_keyframes_attr(attr_value) {
                                spec.easing = match (&spec.easing, &easing) {
                                    // Per-keyframe easing wins; fall
                                    // through to the node-level
                                    // `data-transition-easing` only
                                    // when the keyframe spec didn't
                                    // declare one of its own.
                                    (Easing::Linear, e) => e.clone(),
                                    (other, _) => other.clone(),
                                };
                                self.start_keyframes(id, &prop, spec, now_ms);
                            }
                            continue;
                        }
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
                            if is_color_prop(prop) {
                                let Some(declared) = read_color_prop(props, prop) else {
                                    continue;
                                };
                                let key = (id.clone(), prop.to_string());
                                if self.last_seen_color.contains_key(&key) {
                                    continue;
                                }
                                if let Some((from, duration_ms)) =
                                    parse_animate_in_color_value(attr_value)
                                {
                                    self.start_color(
                                        id,
                                        prop,
                                        from,
                                        declared,
                                        duration_ms,
                                        now_ms,
                                        easing.clone(),
                                    );
                                }
                                self.last_seen_color.insert(key, declared);
                                continue;
                            }
                            let Some(declared) = read_numeric_prop(props, prop) else {
                                continue;
                            };
                            let key = (id.clone(), prop.to_string());
                            if self.last_seen.contains_key(&key) {
                                continue;
                            }
                            if let Some((from, duration_ms)) = parse_animate_in_value(attr_value) {
                                self.start_with_easing(
                                    id,
                                    prop,
                                    from,
                                    declared,
                                    duration_ms,
                                    now_ms,
                                    easing.clone(),
                                );
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
                        // Colour-typed transition: read against the
                        // colour table, lerp in OKLab on `start_color`.
                        if is_color_prop(prop) {
                            let Some(declared) = read_color_prop(props, prop) else {
                                continue;
                            };
                            let key = (id.clone(), prop.to_string());
                            match self.last_seen_color.get(&key).copied() {
                                None => {
                                    self.last_seen_color.insert(key, declared);
                                }
                                Some(prev) if prev != declared => {
                                    if let Some(duration_ms) = parse_duration_ms(attr_value) {
                                        self.start_color(
                                            id,
                                            prop,
                                            prev,
                                            declared,
                                            duration_ms,
                                            now_ms,
                                            easing.clone(),
                                        );
                                    }
                                    self.last_seen_color.insert(key, declared);
                                }
                                Some(_) => {}
                            }
                            continue;
                        }
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
                                    self.start_with_easing(
                                        id,
                                        prop,
                                        prev,
                                        declared,
                                        duration_ms,
                                        now_ms,
                                        easing.clone(),
                                    );
                                }
                                self.last_seen.insert(key, declared);
                            }
                            Some(_) => {
                                // Unchanged — no-op.
                            }
                        }
                    }
                    self.seen_ids.insert(id.clone());
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
    /// the painter to keep rendering. **§7.15** — colour out-specs
    /// route through the OKLab colour-transition table instead of
    /// the numeric one.
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
            let easing = node_easing(props);
            for (prop, spec) in prop_specs {
                if is_color_prop(&prop) {
                    // Colour exit reads the declared exit colour
                    // from the spec's textual `to` slot via the
                    // out-spec's original attr — for the Phase 1
                    // landing we exit to fully transparent (alpha
                    // 0) on the current colour, mirroring the
                    // numeric "fade to declared `to`" idiom
                    // without requiring a hex `to` literal in the
                    // out-spec grammar.
                    let from = read_color_prop(props, &prop).unwrap_or(Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0,
                    });
                    let to = Color { a: 0, ..from };
                    self.start_color(
                        &id,
                        &prop,
                        from,
                        to,
                        spec.duration_ms,
                        now_ms,
                        easing.clone(),
                    );
                    continue;
                }
                let from = read_numeric_prop(props, &prop).unwrap_or(spec.to);
                self.start_with_easing(
                    &id,
                    &prop,
                    from,
                    spec.to,
                    spec.duration_ms,
                    now_ms,
                    easing.clone(),
                );
            }
            // Drop the spec entries so a fresh mount of the same id
            // doesn't carry stale out-state.
            self.out_pending.retain(|(sid, _), _| sid != &id);
            // Forget the `seen_ids` mark so a fresh mount of the
            // same id re-fires its entry / keyframe install.
            self.seen_ids.remove(&id);
            self.last_seen.retain(|(sid, _), _| sid != &id);
            self.last_seen_color.retain(|(sid, _), _| sid != &id);
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
    ///
    /// **§7.15** — three live-transition tables are sampled in
    /// turn: numeric (`self.active`), colour (`self.active_color`),
    /// and keyframe (`self.active_keyframes`). Keyframe samples
    /// take precedence over both delta / entry transitions for the
    /// same `(id, prop)` pair — keyframes are an explicit multi-stop
    /// timeline; the smoothing paths are implicit.
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
                    // `self.active` / `self.active_color` maps so a
                    // single sample-and-write path covers both.
                    let mut keys: Vec<String> = props
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
                    // Pull in any keyframe-only props that don't
                    // also have a `data-transition-*` /
                    // `data-animate-in-*` author hint on this node.
                    for (kid, prop) in self.active_keyframes.keys() {
                        if kid == id && !keys.iter().any(|k| k == prop) {
                            keys.push(prop.clone());
                        }
                    }
                    for prop in keys {
                        // Keyframes win over deltas / entries.
                        if let Some(k) = self.active_keyframes.get(&(id.clone(), prop.clone())) {
                            if let Some(n) = sample_keyframes_numeric(k, now_ms) {
                                write_numeric_prop(props, &prop, n);
                            }
                            if let Some(c) = sample_keyframes_color(k, now_ms) {
                                write_color_prop(props, &prop, c);
                            }
                            continue;
                        }
                        if let Some(t) = self.active.get(&(id.clone(), prop.clone())) {
                            let value = sample(t, now_ms);
                            write_numeric_prop(props, &prop, value);
                            continue;
                        }
                        if let Some(t) = self.active_color.get(&(id.clone(), prop.clone())) {
                            let value = sample_color(t, now_ms);
                            write_color_prop(props, &prop, value);
                        }
                    }
                }
                self.apply(children, now_ms);
            }
        }
    }

    /// Number of in-flight transitions. Exposed for tests + the
    /// shell's debug HUD; production callers should prefer
    /// [`Animator::needs_redraw`]. **§7.15** — sums the three
    /// surfaces; the numeric / colour / keyframe split is an
    /// internal detail callers don't need.
    pub fn active_count(&self) -> usize {
        self.active.len() + self.active_color.len() + self.active_keyframes.len()
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

/// §4.1 — resolve a container's `data-transition-easing` hint into an
/// [`Easing`]. Absent → [`Easing::Linear`] (the prior behaviour, so
/// every existing transition is byte-identical without the attr).
fn node_easing(props: &crate::layout::ContainerProps) -> Easing {
    props
        .semantic
        .attrs
        .iter()
        .find(|(k, _)| k == "data-transition-easing")
        .map(|(_, v)| parse_easing(v))
        .unwrap_or_default()
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

/// **§7.15** — read a colour-valued prop from `ContainerProps` by
/// name. Sibling of [`read_numeric_prop`] for the OKLab-lerp path.
/// `background` and the foreground `color` (resolved at paint time
/// on text children) are first-class; `tint` rounds out the existing
/// [`crate::layout::StateOverrides`] colour fields. Unknown names
/// return `None`.
fn read_color_prop(props: &crate::layout::ContainerProps, prop: &str) -> Option<Color> {
    match prop {
        "background" => props.background,
        // No top-level `color` on `ContainerProps` today — text
        // colour lives on `TextProps`. When a container declares a
        // `color` transition, the animator records the declared
        // colour into the active set and the apply pass writes it
        // back into the `data-style-color` attr, letting the
        // resolver pipeline (style.rs) pick it up on the next
        // walk. This keeps the runtime substrate intact while the
        // typed `color` ContainerProps field lands as a follow-up.
        "color" => read_color_data_attr(props, "data-style-color"),
        "tint" => read_color_data_attr(props, "data-style-tint"),
        _ => None,
    }
}

/// Decode a `Color` from a `data-style-<key>` semantic attr. Used by
/// [`read_color_prop`] for the `color` / `tint` props that don't
/// have a typed `ContainerProps` field yet.
fn read_color_data_attr(props: &crate::layout::ContainerProps, attr: &str) -> Option<Color> {
    let raw = props
        .semantic
        .attrs
        .iter()
        .find(|(k, _)| k == attr)
        .map(|(_, v)| v.as_str())?;
    parse_hex_color(raw)
}

/// Parse a `#RGB` / `#RRGGBB` / `#RRGGBBAA` hex literal into a
/// [`Color`]. Matches the `interpret::style::parse_color` shape so
/// the animator and the style resolver decode identically; lifted
/// here so the animator doesn't reach into the style module's
/// `pub(super)` API.
fn parse_hex_color(raw: &str) -> Option<Color> {
    let s = raw.trim();
    let hex = s.strip_prefix('#')?;
    let (r, g, b, a) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            255_u8,
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            u8::from_str_radix(&hex[6..8], 16).ok()?,
        ),
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            (r * 17, g * 17, b * 17, 255_u8)
        }
        _ => return None,
    };
    Some(Color { r, g, b, a })
}

/// **§7.15** — `data-animate-in-<color-prop>="#<from-hex> <duration>"`
/// parser. Sibling of [`parse_animate_in_value`] for the colour
/// path: the from value is a hex literal instead of a float.
fn parse_animate_in_color_value(spec: &str) -> Option<(Color, u64)> {
    let trimmed = spec.trim();
    let mut parts = trimmed.split_whitespace();
    let from = parse_hex_color(parts.next()?)?;
    let duration_ms = parse_duration_ms(parts.next()?)?;
    Some((from, duration_ms))
}

/// Write a colour-valued prop back into `ContainerProps`. Sister to
/// [`write_numeric_prop`].
fn write_color_prop(props: &mut crate::layout::ContainerProps, prop: &str, value: Color) {
    match prop {
        "background" => props.background = Some(value),
        "color" => write_color_data_attr(props, "data-style-color", value),
        "tint" => write_color_data_attr(props, "data-style-tint", value),
        _ => {}
    }
}

/// Write a `Color` back into the matching `data-style-<key>` attr.
/// Used by [`write_color_prop`] for props without a typed
/// `ContainerProps` field yet.
fn write_color_data_attr(props: &mut crate::layout::ContainerProps, attr: &str, value: Color) {
    let encoded = format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        value.r, value.g, value.b, value.a
    );
    let mut found = false;
    for (k, v) in props.semantic.attrs.iter_mut() {
        if k == attr {
            *v = encoded.clone();
            found = true;
            break;
        }
    }
    if !found {
        props.semantic.attrs.push((attr.to_string(), encoded));
    }
}

/// **§7.15** — sample a colour transition's value at `now_ms`
/// against its easing curve. Sibling of [`sample`] for the OKLab
/// lerp path.
fn sample_color(t: &ColorTransition, now_ms: u64) -> Color {
    if now_ms <= t.started_ms {
        return t.from;
    }
    let elapsed = now_ms.saturating_sub(t.started_ms);
    if elapsed >= t.duration_ms {
        return t.to;
    }
    let progress = elapsed as f32 / t.duration_ms as f32;
    let eased = t.easing.ease(progress);
    lerp_command_color(t.from, t.to, eased as f64)
}

/// **§7.15** — author-declared keyframe spec, the parsed form of an
/// `animator:keyframes="<spec>"` attribute. Carries the duration,
/// the optional easing override, and the per-prop stop tables. The
/// [`Animator::start_keyframes`] entry point consumes one of these
/// and turns it into an in-flight [`Keyframes`] entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyframesSpec {
    /// Total timeline duration. Stops are placed at relative `t ∈
    /// [0, 1]`; `t * duration_ms` gives the absolute timestamp.
    pub duration_ms: u64,
    /// `(t, value)` pairs for the numeric-typed half of the
    /// timeline. `t` is the relative time within the duration,
    /// `[0, 1]`. Stops at the same `t` keep insertion order; the
    /// install path sorts ascending.
    pub numeric_stops: Vec<(f32, f32)>,
    /// `(t, colour)` pairs for the colour-typed half. Same shape
    /// and ordering rules.
    pub color_stops: Vec<(f32, Color)>,
    /// Easing applied to the per-segment progress (the same curve
    /// shape used by [`Transition`] and [`ColorTransition`]).
    pub easing: Easing,
}

/// **§7.15** — parse the canonical keyframes attribute spec into a
/// structured [`KeyframesSpec`]. Spec grammar:
///
/// ```text
/// keyframes := stop (';' stop)*
/// stop := stop-key '=' stop-value
/// stop-key := 'duration' | percent | 'easing'
/// stop-value := duration-ms | easing-keyword | prop-list
/// prop-list := prop ('&' prop)*
/// prop := <prop-name> ':' <value>
/// percent := <float> '%'
/// ```
///
/// Example:
///
/// ```text
/// duration=800ms; easing=ease-in-out;
/// 0%=opacity:0&background:#ffffff;
/// 50%=opacity:1&background:#3b82f6;
/// 100%=opacity:0.5
/// ```
///
/// Returns `None` when the spec is unparseable (no duration, no
/// stops). Bare numeric or colour values dispatch into the matching
/// stop table.
pub fn parse_keyframes_spec(spec: &str) -> Option<KeyframesSpec> {
    let mut duration_ms: Option<u64> = None;
    let mut easing = Easing::default();
    let mut numeric_stops: Vec<(f32, f32)> = Vec::new();
    let mut color_stops: Vec<(f32, Color)> = Vec::new();
    for chunk in spec.split(';') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let Some((key, value)) = chunk.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key.eq_ignore_ascii_case("duration") {
            duration_ms = parse_duration_ms(value);
            continue;
        }
        if key.eq_ignore_ascii_case("easing") {
            easing = parse_easing(value);
            continue;
        }
        if let Some(percent) = key
            .strip_suffix('%')
            .and_then(|n| n.trim().parse::<f32>().ok())
        {
            let t = (percent / 100.0).clamp(0.0, 1.0);
            for prop_chunk in value.split('&') {
                let prop_chunk = prop_chunk.trim();
                let Some((prop, raw_val)) = prop_chunk.split_once(':') else {
                    continue;
                };
                let prop = prop.trim();
                let raw_val = raw_val.trim();
                if let Some(colour) = parse_hex_color(raw_val) {
                    color_stops.push((t, colour));
                    // Tag the stop with the prop name. The
                    // animator key is `(node-id, prop)`; rather
                    // than carrying a separate prop per stop, we
                    // emit one `KeyframesSpec` per prop at
                    // install time. The caller's installer (see
                    // observe-side keyframes pickup) splits stops
                    // by prop before calling `start_keyframes`.
                    // Store the prop in a sidecar through a
                    // `(t, prop, color)` tuple? — for the Phase 1
                    // landing we keep one KeyframesSpec per
                    // (node, prop), so the prop is implied by the
                    // installer. The vec here is the colour
                    // stops *for this prop*; the parser layer
                    // upstream invokes us once per prop.
                    let _ = prop;
                    continue;
                }
                if let Ok(n) = raw_val.parse::<f32>() {
                    numeric_stops.push((t, n));
                    let _ = prop;
                    continue;
                }
            }
        }
    }
    let duration_ms = duration_ms?;
    if numeric_stops.is_empty() && color_stops.is_empty() {
        return None;
    }
    Some(KeyframesSpec {
        duration_ms,
        numeric_stops,
        color_stops,
        easing,
    })
}

/// **§7.15** — top-level parser for an `animator:keyframes` attr
/// value. Splits the spec into one [`KeyframesSpec`] per
/// referenced prop, so each `(node-id, prop)` key in the animator
/// gets its own timeline. Returns a vec of `(prop, spec)`; an
/// empty vec means the attr was malformed (silently skipped).
///
/// The single-prop [`parse_keyframes_spec`] above is the
/// implementation kernel — this wrapper handles the "spec
/// references many props, animator needs them keyed separately"
/// concern.
pub fn parse_keyframes_attr(spec: &str) -> Vec<(String, KeyframesSpec)> {
    let mut duration_ms: Option<u64> = None;
    let mut easing = Easing::default();
    // (prop, t, value-or-color)
    let mut numeric: Vec<(String, f32, f32)> = Vec::new();
    let mut colour: Vec<(String, f32, Color)> = Vec::new();
    for chunk in spec.split(';') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let Some((key, value)) = chunk.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key.eq_ignore_ascii_case("duration") {
            duration_ms = parse_duration_ms(value);
            continue;
        }
        if key.eq_ignore_ascii_case("easing") {
            easing = parse_easing(value);
            continue;
        }
        let Some(percent) = key
            .strip_suffix('%')
            .and_then(|n| n.trim().parse::<f32>().ok())
        else {
            continue;
        };
        let t = (percent / 100.0).clamp(0.0, 1.0);
        for prop_chunk in value.split('&') {
            let prop_chunk = prop_chunk.trim();
            let Some((prop, raw_val)) = prop_chunk.split_once(':') else {
                continue;
            };
            let prop = prop.trim().to_string();
            let raw_val = raw_val.trim();
            if let Some(c) = parse_hex_color(raw_val) {
                colour.push((prop, t, c));
                continue;
            }
            if let Ok(n) = raw_val.parse::<f32>() {
                numeric.push((prop, t, n));
                continue;
            }
        }
    }
    let Some(duration_ms) = duration_ms else {
        return Vec::new();
    };
    let mut by_prop: std::collections::BTreeMap<String, KeyframesSpec> =
        std::collections::BTreeMap::new();
    for (prop, t, value) in numeric {
        let entry = by_prop.entry(prop).or_insert_with(|| KeyframesSpec {
            duration_ms,
            numeric_stops: Vec::new(),
            color_stops: Vec::new(),
            easing: easing.clone(),
        });
        entry.numeric_stops.push((t, value));
    }
    for (prop, t, value) in colour {
        let entry = by_prop.entry(prop).or_insert_with(|| KeyframesSpec {
            duration_ms,
            numeric_stops: Vec::new(),
            color_stops: Vec::new(),
            easing: easing.clone(),
        });
        entry.color_stops.push((t, value));
    }
    by_prop.into_iter().collect()
}

/// **§7.15** — sample the numeric value of a keyframe timeline at
/// `now_ms`. Walks the (sorted, ascending-`t`) stop list and lerps
/// between the bracketing pair, applying the timeline's easing
/// curve to the per-segment progress. Out-of-bounds `now_ms` clamps
/// to the first / last stop. `None` when the timeline has no
/// numeric stops.
fn sample_keyframes_numeric(k: &Keyframes, now_ms: u64) -> Option<f32> {
    if k.numeric_stops.is_empty() {
        return None;
    }
    let progress = if now_ms <= k.started_ms {
        0.0
    } else {
        let elapsed = now_ms.saturating_sub(k.started_ms);
        if elapsed >= k.duration_ms {
            1.0
        } else {
            elapsed as f32 / k.duration_ms as f32
        }
    };
    let stops = &k.numeric_stops;
    if progress <= stops[0].0 {
        return Some(stops[0].1);
    }
    if progress >= stops[stops.len() - 1].0 {
        return Some(stops[stops.len() - 1].1);
    }
    for pair in stops.windows(2) {
        let (t0, v0) = pair[0];
        let (t1, v1) = pair[1];
        if progress >= t0 && progress <= t1 {
            let span = (t1 - t0).max(f32::EPSILON);
            let local = ((progress - t0) / span).clamp(0.0, 1.0);
            let eased = k.easing.ease(local);
            return Some(v0 + (v1 - v0) * eased);
        }
    }
    Some(stops[stops.len() - 1].1)
}

/// **§7.15** — sample the colour value of a keyframe timeline at
/// `now_ms`. Sister of [`sample_keyframes_numeric`] for the OKLab
/// lerp path. `None` when the timeline has no colour stops.
fn sample_keyframes_color(k: &Keyframes, now_ms: u64) -> Option<Color> {
    if k.color_stops.is_empty() {
        return None;
    }
    let progress = if now_ms <= k.started_ms {
        0.0
    } else {
        let elapsed = now_ms.saturating_sub(k.started_ms);
        if elapsed >= k.duration_ms {
            1.0
        } else {
            elapsed as f32 / k.duration_ms as f32
        }
    };
    let stops = &k.color_stops;
    if progress <= stops[0].0 {
        return Some(stops[0].1);
    }
    if progress >= stops[stops.len() - 1].0 {
        return Some(stops[stops.len() - 1].1);
    }
    for pair in stops.windows(2) {
        let (t0, c0) = pair[0];
        let (t1, c1) = pair[1];
        if progress >= t0 && progress <= t1 {
            let span = (t1 - t0).max(f32::EPSILON);
            let local = ((progress - t0) / span).clamp(0.0, 1.0);
            let eased = k.easing.ease(local);
            return Some(lerp_command_color(c0, c1, eased as f64));
        }
    }
    Some(stops[stops.len() - 1].1)
}

/// **§7.15** — the set of prop names the animator recognises as
/// colour-valued. Used by the observe path to dispatch between the
/// numeric and colour transition tables when reading a
/// `data-transition-<prop>` or `data-animate-in-<prop>` attr.
fn is_color_prop(prop: &str) -> bool {
    matches!(prop, "background" | "color" | "tint")
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

    /// §4.1 — `parse_easing` maps named keywords to builtin curves
    /// and a comma list to a sampled LUT; junk degrades to linear.
    #[test]
    fn parse_easing_recognises_keywords_and_lut() {
        assert_eq!(parse_easing("linear"), Easing::Linear);
        assert_eq!(parse_easing("ease-in"), Easing::EaseIn);
        assert_eq!(parse_easing("EASE_OUT"), Easing::EaseOut);
        assert_eq!(parse_easing("ease"), Easing::EaseInOut);
        assert_eq!(parse_easing("garbage"), Easing::Linear);
        assert_eq!(parse_easing(""), Easing::Linear);
        match parse_easing("0,0.25,1") {
            Easing::Lut(s) => assert_eq!(&*s, &[0.0, 0.25, 1.0]),
            other => panic!("expected Lut, got {other:?}"),
        }
        // A single sample is not a curve — degrade to linear.
        assert_eq!(parse_easing("0.5"), Easing::Linear);
    }

    /// §4.1 — `Easing::Lut` linearly interpolates between bracketing
    /// samples, is bounded, and stays monotone for a monotone table.
    #[test]
    fn lut_easing_interpolates_between_samples() {
        let e = Easing::from_samples(vec![0.0, 0.5, 1.0]);
        assert!((e.ease(0.0) - 0.0).abs() < 1e-6);
        assert!((e.ease(1.0) - 1.0).abs() < 1e-6);
        // Midpoint of the table is sample[1] exactly.
        assert!((e.ease(0.5) - 0.5).abs() < 1e-6);
        // Quarter point sits halfway between sample[0] and sample[1].
        assert!((e.ease(0.25) - 0.25).abs() < 1e-6);
        // Clamps out-of-range t to the table ends.
        assert!((e.ease(-1.0) - 0.0).abs() < 1e-6);
        assert!((e.ease(2.0) - 1.0).abs() < 1e-6);
    }

    /// §4.1 — a `data-transition-easing` LUT attr on the container
    /// reshapes the interpolation curve for that node's transitions.
    /// A back-loaded curve keeps the value near `from` at mid-flight
    /// where a linear curve would already be halfway.
    #[test]
    fn observe_applies_custom_easing_from_attr() {
        fn node(padding: f32) -> Node {
            Node::Container {
                id: "card".into(),
                props: ContainerProps {
                    padding: Padding::all(padding),
                    semantic: Semantic {
                        attrs: vec![
                            ("data-transition-padding".into(), "200ms".into()),
                            // Strongly back-loaded: flat at 0 until the
                            // last segment. Linear at progress 0.5
                            // would be 0.5; this LUT is still ~0.
                            ("data-transition-easing".into(), "0,0,0,1".into()),
                        ],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                children: vec![],
            }
        }
        let mut animator = Animator::new();
        animator.observe(std::slice::from_ref(&node(0.0)), 0);
        animator.observe(std::slice::from_ref(&node(100.0)), 0);
        assert_eq!(animator.active_count(), 1);
        // Mid-flight (progress 0.5): custom curve keeps it near `from`.
        let mid = animator.current("card", "padding", 100).unwrap();
        assert!(
            mid < 5.0,
            "back-loaded easing mid value: {mid} (linear would be ~50)"
        );
        // End of duration still lands exactly on `to`.
        let done = animator.current("card", "padding", 200).unwrap();
        assert!((done - 100.0).abs() < 1e-3, "end value: {done}");
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

    // -----------------------------------------------------------
    // §7.15 — unified Animator trait tests
    // -----------------------------------------------------------

    /// §7.15 — `start_color` + `current_color` round-trips through
    /// the OKLab lerp path. Endpoints are exact; the midpoint sits
    /// at the perceptual middle, not the sRGB linear blend.
    #[test]
    fn color_transition_interpolates_in_oklab() {
        let mut animator = Animator::new();
        let from = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        let to = Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        animator.start_color("a", "background", from, to, 200, 0, Easing::Linear);
        let at_start = animator.current_color("a", "background", 0).unwrap();
        assert_eq!(at_start, from);
        let at_end = animator.current_color("a", "background", 200).unwrap();
        assert_eq!(at_end, to);
        let mid = animator.current_color("a", "background", 100).unwrap();
        // OKLab midpoint of black/white is darker than sRGB 0x80.
        assert!(mid.r == mid.g && mid.g == mid.b);
        assert!(mid.r < 0x80, "OKLab mid grey, got 0x{:02x}", mid.r);
    }

    /// §7.15 — `data-transition-background="200ms"` on a node whose
    /// background colour changes between observes fires a colour
    /// transition routed through OKLab.
    #[test]
    fn observe_fires_color_transition_on_background_change() {
        fn node(bg: Color) -> Node {
            let mut semantic = Semantic::default();
            semantic
                .attrs
                .push(("data-transition-background".into(), "200ms".into()));
            Node::Container {
                id: "card".into(),
                props: ContainerProps {
                    background: Some(bg),
                    semantic,
                    ..Default::default()
                },
                children: vec![],
            }
        }
        let mut animator = Animator::new();
        let blue = Color {
            r: 0x3b,
            g: 0x82,
            b: 0xf6,
            a: 0xff,
        };
        let red = Color {
            r: 0xef,
            g: 0x44,
            b: 0x44,
            a: 0xff,
        };
        animator.observe(std::slice::from_ref(&node(blue)), 0);
        // First observe seeds the snapshot — no transition yet.
        assert_eq!(animator.active_count(), 0);
        animator.observe(std::slice::from_ref(&node(red)), 100);
        // A colour transition is live.
        assert_eq!(animator.active_count(), 1);
        let mid = animator.current_color("card", "background", 200).unwrap();
        // Mid-flight: the blue and red have mixed perceptually
        // (channels don't all sit at the starting colour any more).
        assert!(
            mid != blue && mid != red,
            "expected mid-flight colour, got #{:02x}{:02x}{:02x}",
            mid.r,
            mid.g,
            mid.b
        );
    }

    /// §7.15 — `parse_keyframes_attr` splits a multi-prop spec into
    /// one `KeyframesSpec` per referenced prop. Stops are sorted
    /// ascending and carry the shared duration + easing.
    #[test]
    fn parse_keyframes_attr_splits_props() {
        let spec = "duration=800ms;0%=opacity:0&background:#000000;\
                    50%=opacity:1&background:#3b82f6;\
                    100%=opacity:0.5&background:#ffffff";
        let parsed = parse_keyframes_attr(spec);
        assert_eq!(parsed.len(), 2, "two props: opacity + background");
        let by_prop: std::collections::HashMap<_, _> = parsed.into_iter().collect();
        let opacity = by_prop.get("opacity").expect("opacity present");
        assert_eq!(opacity.duration_ms, 800);
        assert_eq!(opacity.numeric_stops.len(), 3);
        assert!((opacity.numeric_stops[0].0 - 0.0).abs() < 1e-3);
        assert!((opacity.numeric_stops[1].0 - 0.5).abs() < 1e-3);
        assert!((opacity.numeric_stops[2].0 - 1.0).abs() < 1e-3);
        assert!((opacity.numeric_stops[1].1 - 1.0).abs() < 1e-3);
        let bg = by_prop.get("background").expect("background present");
        assert_eq!(bg.color_stops.len(), 3);
        assert_eq!(bg.color_stops[1].1.r, 0x3b);
        assert_eq!(bg.color_stops[1].1.g, 0x82);
        assert_eq!(bg.color_stops[1].1.b, 0xf6);
    }

    /// §7.15 — `start_keyframes` installs a numeric timeline; the
    /// keyframe sampler interpolates between bracketing stops, with
    /// out-of-bounds `now_ms` clamping to the first / last stop.
    #[test]
    fn keyframes_numeric_interpolates_between_stops() {
        let mut animator = Animator::new();
        animator.start_keyframes(
            "a",
            "opacity",
            KeyframesSpec {
                duration_ms: 1000,
                numeric_stops: vec![(0.0, 0.0), (0.5, 1.0), (1.0, 0.5)],
                color_stops: vec![],
                easing: Easing::Linear,
            },
            0,
        );
        // At t=0, value is the first stop.
        assert!((animator.keyframe_numeric("a", "opacity", 0).unwrap() - 0.0).abs() < 1e-3);
        // At t=500 (mid of first segment endpoint), value is 1.0.
        assert!((animator.keyframe_numeric("a", "opacity", 500).unwrap() - 1.0).abs() < 1e-3);
        // At t=250 (quarter into the first segment of width 0.5),
        // we're at half-progress within that segment → linear lerp
        // from 0.0 to 1.0 at 0.5 → 0.5.
        let q = animator.keyframe_numeric("a", "opacity", 250).unwrap();
        assert!((q - 0.5).abs() < 1e-3, "quarter-progress: {q}");
        // At t=1000 (end), we're at the last stop.
        assert!((animator.keyframe_numeric("a", "opacity", 1000).unwrap() - 0.5).abs() < 1e-3);
        // Beyond end clamps to last stop.
        assert!((animator.keyframe_numeric("a", "opacity", 2000).unwrap() - 0.5).abs() < 1e-3);
    }

    /// §7.15 — keyframe colour timeline interpolates colour stops
    /// in OKLab. Endpoints exact; mid-flight sits between the two
    /// bracketing colours.
    #[test]
    fn keyframes_color_interpolates_between_stops() {
        let mut animator = Animator::new();
        let red = Color {
            r: 0xff,
            g: 0,
            b: 0,
            a: 0xff,
        };
        let blue = Color {
            r: 0,
            g: 0,
            b: 0xff,
            a: 0xff,
        };
        animator.start_keyframes(
            "a",
            "background",
            KeyframesSpec {
                duration_ms: 1000,
                numeric_stops: vec![],
                color_stops: vec![(0.0, red), (1.0, blue)],
                easing: Easing::Linear,
            },
            0,
        );
        let start = animator.keyframe_color("a", "background", 0).unwrap();
        assert!((start.r as i32 - 0xff).abs() <= 1);
        let end = animator.keyframe_color("a", "background", 1000).unwrap();
        assert!((end.b as i32 - 0xff).abs() <= 1);
        let mid = animator.keyframe_color("a", "background", 500).unwrap();
        assert!(
            mid.r > 0 && mid.b > 0,
            "OKLab mid of red→blue mixes both, got #{:02x}{:02x}{:02x}",
            mid.r,
            mid.g,
            mid.b
        );
    }

    /// §7.15 — `data-animator-keyframes="<spec>"` on a node fires
    /// once on first observe; `apply` writes the keyframe-sampled
    /// values back into the node's props.
    #[test]
    fn observe_installs_and_apply_writes_keyframes() {
        let mut semantic = Semantic::default();
        semantic.attrs.push((
            "data-animator-keyframes".into(),
            "duration=1000ms;0%=opacity:0;100%=opacity:1".into(),
        ));
        let node = Node::Container {
            id: "fade".into(),
            props: ContainerProps {
                opacity: Some(0.5),
                semantic,
                ..Default::default()
            },
            children: vec![],
        };
        let mut animator = Animator::new();
        animator.observe(std::slice::from_ref(&node), 0);
        assert_eq!(animator.active_count(), 1, "keyframe timeline live");
        let mut tree = vec![node.clone()];
        animator.apply(&mut tree, 500);
        // Mid-flight: opacity sampled at 0.5 of a 0→1 lerp.
        let Node::Container { props, .. } = &tree[0] else {
            panic!()
        };
        let op = props.opacity.unwrap_or(0.0);
        assert!((op - 0.5).abs() < 1e-3, "mid-flight opacity: {op}");
    }

    /// §7.15 — re-observing a node with `data-animator-keyframes`
    /// is a no-op for keyframe install (entry-once rule). Tick
    /// drains the timeline once the duration elapses.
    #[test]
    fn keyframes_fire_once_per_mount_and_drain_on_tick() {
        let mut semantic = Semantic::default();
        semantic.attrs.push((
            "data-animator-keyframes".into(),
            "duration=200ms;0%=opacity:0;100%=opacity:1".into(),
        ));
        let node = Node::Container {
            id: "fade".into(),
            props: ContainerProps {
                opacity: Some(0.5),
                semantic,
                ..Default::default()
            },
            children: vec![],
        };
        let mut animator = Animator::new();
        animator.observe(std::slice::from_ref(&node), 0);
        animator.observe(std::slice::from_ref(&node), 50);
        // Second observe must NOT install a second timeline.
        assert_eq!(animator.active_count(), 1);
        // Past the duration, tick drains the timeline and the
        // animator stops asking for redraws.
        let still = animator.tick(1000);
        assert!(!still, "timeline drained");
        assert_eq!(animator.active_count(), 0);
    }

    /// §7.15 — keyframes take precedence over delta / entry
    /// transitions for the same `(node, prop)`. When both are
    /// installed, `apply` writes the keyframe-sampled value.
    #[test]
    fn keyframes_win_over_delta_transition_on_same_prop() {
        let mut animator = Animator::new();
        // Install a numeric delta transition that would interpolate
        // opacity 0 → 1 over 1000ms.
        animator.start("fade", "opacity", 0.0, 1.0, 1000, 0);
        // Install a keyframe timeline for the same (node, prop)
        // that holds opacity at 0.25 mid-flight.
        animator.start_keyframes(
            "fade",
            "opacity",
            KeyframesSpec {
                duration_ms: 1000,
                numeric_stops: vec![(0.0, 0.25), (1.0, 0.25)],
                color_stops: vec![],
                easing: Easing::Linear,
            },
            0,
        );
        let mut semantic = Semantic::default();
        semantic
            .attrs
            .push(("data-transition-opacity".into(), "1000ms".into()));
        let node = Node::Container {
            id: "fade".into(),
            props: ContainerProps {
                opacity: Some(0.5),
                semantic,
                ..Default::default()
            },
            children: vec![],
        };
        let mut tree = vec![node];
        animator.apply(&mut tree, 500);
        let Node::Container { props, .. } = &tree[0] else {
            panic!()
        };
        let op = props.opacity.unwrap_or(0.0);
        // The keyframe path wrote 0.25, not the delta-transition's
        // mid-flight 0.5.
        assert!((op - 0.25).abs() < 1e-3, "keyframes should win, got {op}");
    }

    /// §7.15 — re-mounting an id whose previous lifecycle finished
    /// re-fires the entry / keyframe install. `seen_ids` clears on
    /// unmount so a fresh mount measures against a clean slate.
    #[test]
    fn remount_reinstalls_keyframes() {
        let mut semantic = Semantic::default();
        semantic.attrs.push((
            "data-animator-keyframes".into(),
            "duration=200ms;0%=opacity:0;100%=opacity:1".into(),
        ));
        semantic
            .attrs
            .push(("data-animate-out-opacity".into(), "0 100ms".into()));
        let node = Node::Container {
            id: "fade".into(),
            props: ContainerProps {
                opacity: Some(0.5),
                semantic,
                ..Default::default()
            },
            children: vec![],
        };
        let mut animator = Animator::new();
        animator.observe(std::slice::from_ref(&node), 0);
        assert_eq!(animator.active_count(), 1);
        // Unmount: the node leaves; out transition + drain.
        animator.observe(&[], 300);
        animator.tick(500);
        // Re-mount: keyframes should install again.
        animator.observe(std::slice::from_ref(&node), 600);
        // One keyframe timeline is live again.
        assert!(animator.active_count() >= 1);
    }
}
