//! Compiled program — a parsed project indexed for runtime lookup.
//!
//! A [`Bundle`] holds every `.loom` file in a project plus the
//! cross-file name indices the [`crate::resolver`] reads against.
//! Construction (parsing + indexing) lives in [`crate::project`];
//! resolution lives in [`crate::resolver`].

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use loom_parser::ast::{
    Beat, CharacterBody, CohortBody, GeneratorBody, Item, LocationBody, LoomFile, SceneBody,
};
use loom_parser::Diagnostic;

use crate::coroutine::Program;
use crate::expr::World;
use crate::meridian::{StatsProfile, Tree};
use crate::simulacra::CharacterState;

/// Index into [`Bundle::files`].
pub type FileIdx = u32;
/// Index of a [`Beat`] inside a file's `items` list.
pub type BeatIdx = u32;

/// Stable handle to one beat inside the bundle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BeatRef {
    pub file: FileIdx,
    pub beat: BeatIdx,
}

/// One loaded `.loom` file.
#[derive(Clone, Debug)]
pub struct LoomFileEntry {
    /// Project-relative path, normalised with forward slashes.
    pub path: PathBuf,
    /// File stem — `cast/Wren.loom` → `Wren`. Used for the
    /// `-> Wren` divert form (spec §7).
    pub stem: String,
    /// Forward-slash qualifier — `cast/Wren.loom` → `cast`. Used
    /// for the `-> Folder/beat` and `-> Folder/Wren#knot` divert
    /// forms (spec §7).
    pub qualifier: String,
    /// Raw source. Kept so diagnostic span resolution doesn't need
    /// to re-read from disk.
    pub source: String,
    pub file: LoomFile,
    pub diagnostics: Vec<Diagnostic>,
}

impl LoomFileEntry {
    /// The entry beat for this file — used when a divert names a
    /// file by stem instead of a beat. Phase-3: returns the first
    /// `Item::Beat` in source order; the file's header `entry:`
    /// property is only honoured for the project root (`main.loom`).
    pub fn entry_beat_index(&self) -> Option<BeatIdx> {
        for (idx, item) in self.file.items.iter().enumerate() {
            if let Item::Beat(_) = item {
                return Some(idx as BeatIdx);
            }
        }
        None
    }
}

/// Project-level diagnostics emitted by the loader. Distinct from
/// `loom_parser::Diagnostic` (file-scoped) because these describe
/// cross-file conditions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectDiagnostic {
    /// `main.loom` was not found in the project root.
    MissingMainFile,
    /// `main.loom`'s header `entry:` pointed at a beat that doesn't
    /// exist anywhere in the project.
    EntryBeatUnresolved { name: String },
    /// `main.loom` had no `entry:` property and the file declares
    /// no beats — the runtime has nothing to play.
    NoEntryBeat,
    /// A child CHARACTER inherits from two parents that both supply a
    /// default for the same slot, and the child doesn't override. The
    /// runtime can't pick one without losing information (spec §9.5).
    AmbiguousSlot {
        character: String,
        prop: String,
    },
}

/// One compiled Loom project.
#[derive(Debug, Default)]
pub struct Bundle {
    pub files: Vec<LoomFileEntry>,
    /// `beat_name → [BeatRef, …]`. A vector because a project can
    /// legally declare two beats with the same bare name in
    /// different files; the resolver requires the caller to
    /// disambiguate with `Folder/name` if so.
    pub beats_by_name: HashMap<String, Vec<BeatRef>>,
    /// `file_stem → [FileIdx, …]`. Used for the `-> Wren` divert
    /// form (file by stem).
    pub files_by_stem: HashMap<String, Vec<FileIdx>>,
    /// The starting beat, taken from `main.loom`'s `entry:`
    /// property. `None` if the project has no `main.loom` or no
    /// resolvable entry.
    pub entry: Option<BeatRef>,
    pub project_diagnostics: Vec<ProjectDiagnostic>,
    /// Compiled CHARACTER / TRAIT bodies (spec §10), keyed by name.
    /// TRAITs land in the same map so inheritance lookups don't need
    /// a second index; the resolver refuses to spawn a TRAIT standalone.
    pub characters: HashMap<String, CharacterState>,
    /// Compiled STATS profiles (spec §11), keyed by name.
    pub stats_profiles: HashMap<String, StatsProfile>,
    /// Compiled TREE declarations (spec §11), keyed by name.
    pub trees: HashMap<String, Tree>,
    /// Top-level SCENE declarations (spec §12.3), keyed by name.
    pub scenes: HashMap<String, SceneBody>,
    /// Top-level GENERATOR declarations (spec §12.4), keyed by name.
    pub generators: HashMap<String, GeneratorBody>,
    /// Lowered SCENE programs ready for the scheduler.
    pub scene_programs: HashMap<String, Program>,
    /// Lowered top-level GENERATOR programs.
    pub generator_programs: HashMap<String, Program>,
    /// Character-bound generators, derived from
    /// [`CharacterState::generators`].
    pub bound_generators: HashMap<String, Vec<Program>>,
    /// COHORT declarations (spec §13.1), keyed by name.
    pub cohorts: HashMap<String, CohortBody>,
    /// LOCATION declarations (spec §13.1), keyed by name.
    pub locations: HashMap<String, LocationBody>,
}

impl Bundle {
    pub fn file(&self, idx: FileIdx) -> &LoomFileEntry {
        &self.files[idx as usize]
    }

    pub fn beat(&self, r: BeatRef) -> &Beat {
        match &self.file(r.file).file.items[r.beat as usize] {
            Item::Beat(b) => b,
            _ => panic!("BeatRef does not point at a beat — index corrupted"),
        }
    }

    /// All parser diagnostics across every file, in file order.
    pub fn parser_diagnostics(&self) -> impl Iterator<Item = (&LoomFileEntry, &Diagnostic)> {
        self.files
            .iter()
            .flat_map(|f| f.diagnostics.iter().map(move |d| (f, d)))
    }

    /// Pre-populate `characters`, `stats_profiles`, and `trees` from
    /// the parsed file ASTs. Idempotent — clears existing maps first
    /// so callers can re-run after mutating `files`.
    pub fn rebuild_simulacra(&mut self) {
        use loom_parser::ast::DeclarationKind;
        self.characters.clear();
        self.stats_profiles.clear();
        self.trees.clear();
        // Pass 1: STATS + TREE — characters depend on profiles.
        for entry in &self.files {
            for item in &entry.file.items {
                if let Item::Declaration(decl) = item {
                    match decl.kind {
                        DeclarationKind::Stats => {
                            if let Some(body) = &decl.stats {
                                self.stats_profiles.insert(
                                    decl.name.clone(),
                                    StatsProfile::from_body(decl.name.clone(), body),
                                );
                            }
                        }
                        DeclarationKind::Tree => {
                            if let Some(body) = &decl.tree {
                                self.trees
                                    .insert(decl.name.clone(), Tree::from_body(decl.name.clone(), body));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        // SCENE + top-level GENERATOR — independent of stats /
        // characters; lower while we still have a borrow on the
        // file ASTs.
        self.scenes.clear();
        self.generators.clear();
        self.scene_programs.clear();
        self.generator_programs.clear();
        self.cohorts.clear();
        self.locations.clear();
        for entry in &self.files {
            for item in &entry.file.items {
                if let Item::Declaration(decl) = item {
                    match decl.kind {
                        DeclarationKind::Cohort => {
                            if let Some(body) = &decl.cohort {
                                self.cohorts.insert(decl.name.clone(), body.clone());
                            }
                        }
                        DeclarationKind::Location => {
                            if let Some(body) = &decl.location {
                                self.locations.insert(decl.name.clone(), body.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        for entry in &self.files {
            for item in &entry.file.items {
                if let Item::Declaration(decl) = item {
                    match decl.kind {
                        DeclarationKind::Scene => {
                            if let Some(body) = &decl.scene {
                                self.scenes.insert(decl.name.clone(), body.clone());
                                self.scene_programs.insert(
                                    decl.name.clone(),
                                    crate::coroutine::lower_scene(&decl.name, body),
                                );
                            }
                        }
                        DeclarationKind::Generator => {
                            if let Some(body) = &decl.generator {
                                self.generators.insert(decl.name.clone(), body.clone());
                                self.generator_programs.insert(
                                    decl.name.clone(),
                                    crate::coroutine::lower_generator(&decl.name, body),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        // Pass 2: CHARACTER + TRAIT. Collect raw declarations first so
        // we can resolve `is X, Y` mixin merges before compile (spec
        // §9). TRAITs and parent CHARACTERs both contribute defaults;
        // the child wins on every slot it declared itself. An
        // unresolved conflict between two parents on the *same* slot
        // surfaces as `ProjectDiagnostic::AmbiguousSlot`.
        let mut raw_decls: HashMap<String, (CharacterBody, Vec<String>, bool)> = HashMap::new();
        // Source order — used to compose hooks left-to-right.
        let mut decl_order: Vec<String> = Vec::new();
        for entry in &self.files {
            for item in &entry.file.items {
                if let Item::Declaration(decl) = item {
                    if matches!(decl.kind, DeclarationKind::Character | DeclarationKind::Trait) {
                        if let Some(body) = &decl.character {
                            let is_trait = matches!(decl.kind, DeclarationKind::Trait);
                            if !raw_decls.contains_key(&decl.name) {
                                decl_order.push(decl.name.clone());
                            }
                            raw_decls.insert(
                                decl.name.clone(),
                                (body.clone(), decl.mixin.clone(), is_trait),
                            );
                        }
                    }
                }
            }
        }
        let empty_world = World::new();
        let mut merged: HashMap<String, CharacterBody> = HashMap::new();
        for name in &decl_order {
            let merged_body = merge_character(
                name,
                &raw_decls,
                &mut merged,
                &mut self.project_diagnostics,
                &mut HashSet::new(),
            );
            merged.insert(name.clone(), merged_body);
        }
        for name in &decl_order {
            let (_, inherits, is_trait) = raw_decls.get(name).cloned().unwrap();
            let merged_body = merged.get(name).cloned().unwrap_or_default();
            // Skip standalone TRAITs from the runtime character index —
            // they exist only as mixins. The resolver already rejects
            // spawning a TRAIT, but leaving them out of `characters`
            // keeps publish() / hooks / goals scoped to real actors.
            if is_trait {
                continue;
            }
            let state = CharacterState::compile(
                name.clone(),
                inherits,
                &merged_body,
                &self.stats_profiles,
                &empty_world,
            );
            self.characters.insert(name.clone(), state);
        }
        // Character-bound generators (spec §10.5): use merged bodies
        // so inherited TRAIT / parent generators are spawned on the
        // child character too.
        self.bound_generators.clear();
        for name in &decl_order {
            let (_, _, is_trait) = raw_decls.get(name).cloned().unwrap();
            if is_trait {
                continue;
            }
            let Some(body) = merged.get(name) else { continue };
            let mut bound = Vec::new();
            for gen in &body.generators {
                let synth = GeneratorBody {
                    tier: gen.tier.clone(),
                    priority: gen.priority,
                    start_when: None,
                    body: gen.body.clone(),
                };
                let qualified = format!("{}.{}", name, gen.name);
                bound.push(crate::coroutine::lower_generator(&qualified, &synth));
            }
            if !bound.is_empty() {
                self.bound_generators.insert(name.clone(), bound);
            }
        }
    }
}

/// Recursively merge a character's declared body with each parent in
/// its `inherits` list (left-to-right). The merge is conservative:
/// * Properties the child declared win.
/// * Properties only the child *doesn't* declare are pulled from a
///   parent. When two parents disagree on the same default the
///   conflict is reported as `ProjectDiagnostic::AmbiguousSlot` and
///   the first parent's value is kept so playback still works.
/// * Hooks compose by source order — every parent's hooks plus the
///   child's are concatenated (spec §9 — multiple `on meeting Player`
///   bodies all run).
/// * Disposition / knowledge / goals / generators / reacts fall back
///   to source-order concatenation, deduplicated by name where each
///   collection has one.
fn merge_character(
    name: &str,
    raw: &HashMap<String, (CharacterBody, Vec<String>, bool)>,
    merged_cache: &mut HashMap<String, CharacterBody>,
    diagnostics: &mut Vec<ProjectDiagnostic>,
    visiting: &mut HashSet<String>,
) -> CharacterBody {
    if visiting.contains(name) {
        // Cycle — return whatever's already cached or the bare body.
        return merged_cache
            .get(name)
            .cloned()
            .or_else(|| raw.get(name).map(|(b, _, _)| b.clone()))
            .unwrap_or_default();
    }
    if let Some(b) = merged_cache.get(name) {
        return b.clone();
    }
    visiting.insert(name.to_string());
    let Some((own, inherits, _)) = raw.get(name).cloned() else {
        visiting.remove(name);
        return CharacterBody::default();
    };
    let mut merged_body = CharacterBody::default();

    // Walk parents left-to-right so the earlier parent wins ties.
    let mut parent_property_sources: HashMap<String, String> = HashMap::new();
    for parent_name in &inherits {
        let parent_body = merge_character(parent_name, raw, merged_cache, diagnostics, visiting);
        // Properties: pull each parent default only if the child hasn't
        // declared it. Track which parent supplied each so we can
        // diagnose ambiguous slots when a second parent disagrees.
        for (k, v) in &parent_body.properties {
            if own.properties.contains_key(k) {
                continue;
            }
            if let Some(existing_parent) = parent_property_sources.get(k) {
                let existing = merged_body.properties.get(k).map(|p| p.value.clone());
                let other = Some(v.value.clone());
                if existing != other {
                    diagnostics.push(ProjectDiagnostic::AmbiguousSlot {
                        character: name.to_string(),
                        prop: k.clone(),
                    });
                }
                let _ = existing_parent;
                continue;
            }
            merged_body.properties.insert(k.clone(), v.clone());
            parent_property_sources.insert(k.clone(), parent_name.clone());
        }
        // stats_profile from a parent only when child doesn't pick one.
        if merged_body.stats_profile.is_none() && own.stats_profile.is_none() {
            merged_body.stats_profile = parent_body.stats_profile.clone();
        }
        // Disposition: append parent's axes for `(verb, target)` pairs
        // the child + earlier parents haven't covered.
        for d in &parent_body.disposition {
            let exists = merged_body
                .disposition
                .iter()
                .any(|e| e.verb == d.verb && e.target == d.target)
                || own
                    .disposition
                    .iter()
                    .any(|e| e.verb == d.verb && e.target == d.target);
            if !exists {
                merged_body.disposition.push(d.clone());
            }
        }
        // Knowledge: dedup by name.
        for k in &parent_body.knowledge {
            let exists = merged_body.knowledge.iter().any(|e| e.name == k.name)
                || own.knowledge.iter().any(|e| e.name == k.name);
            if !exists {
                merged_body.knowledge.push(k.clone());
            }
        }
        // Goals: dedup by name.
        for g in &parent_body.goals {
            let exists = merged_body.goals.iter().any(|e| e.name == g.name)
                || own.goals.iter().any(|e| e.name == g.name);
            if !exists {
                merged_body.goals.push(g.clone());
            }
        }
        // Generators: dedup by name.
        for g in &parent_body.generators {
            let exists = merged_body.generators.iter().any(|e| e.name == g.name)
                || own.generators.iter().any(|e| e.name == g.name);
            if !exists {
                merged_body.generators.push(g.clone());
            }
        }
        // Reacts: simple concat, no dedup (different conditions can
        // both fire).
        for r in &parent_body.reacts {
            merged_body.reacts.push(r.clone());
        }
        // Hooks: append in source order so every parent's body runs.
        for h in &parent_body.hooks {
            merged_body.hooks.push(h.clone());
        }
    }

    // Now layer the child's own declarations on top — child wins.
    for (k, v) in &own.properties {
        merged_body.properties.insert(k.clone(), v.clone());
    }
    if own.stats_profile.is_some() {
        merged_body.stats_profile = own.stats_profile.clone();
    }
    for d in &own.disposition {
        merged_body.disposition.push(d.clone());
    }
    for k in &own.knowledge {
        merged_body.knowledge.push(k.clone());
    }
    for g in &own.goals {
        merged_body.goals.push(g.clone());
    }
    for g in &own.generators {
        merged_body.generators.push(g.clone());
    }
    for r in &own.reacts {
        merged_body.reacts.push(r.clone());
    }
    for h in &own.hooks {
        merged_body.hooks.push(h.clone());
    }

    visiting.remove(name);
    merged_body
}
