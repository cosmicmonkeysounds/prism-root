//! Compiled program — a parsed project indexed for runtime lookup.
//!
//! A [`Bundle`] holds every `.loom` file in a project plus the
//! cross-file name indices the [`crate::resolver`] reads against.
//! Construction (parsing + indexing) lives in [`crate::project`];
//! resolution lives in [`crate::resolver`].

use std::collections::HashMap;
use std::path::PathBuf;

use loom_parser::ast::{
    Beat, CohortBody, GeneratorBody, Item, LocationBody, LoomFile, SceneBody,
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
        // Pass 2: CHARACTER + TRAIT.
        let empty_world = World::new();
        for entry in &self.files {
            for item in &entry.file.items {
                if let Item::Declaration(decl) = item {
                    if matches!(decl.kind, DeclarationKind::Character | DeclarationKind::Trait) {
                        if let Some(body) = &decl.character {
                            let state = CharacterState::compile(
                                decl.name.clone(),
                                decl.mixin.clone(),
                                body,
                                &self.stats_profiles,
                                &empty_world,
                            );
                            self.characters.insert(decl.name.clone(), state);
                        }
                    }
                }
            }
        }
        // Character-bound generators (spec §10.5): walk the parser
        // ASTs since `CharacterState` may not surface raw bodies.
        self.bound_generators.clear();
        for entry in &self.files {
            for item in &entry.file.items {
                if let Item::Declaration(decl) = item {
                    if matches!(decl.kind, DeclarationKind::Character | DeclarationKind::Trait) {
                        if let Some(body) = &decl.character {
                            let mut bound = Vec::new();
                            for gen in &body.generators {
                                let synth = GeneratorBody {
                                    tier: gen.tier.clone(),
                                    priority: gen.priority,
                                    start_when: None,
                                    body: gen.body.clone(),
                                };
                                let qualified = format!("{}.{}", decl.name, gen.name);
                                bound.push(crate::coroutine::lower_generator(
                                    &qualified, &synth,
                                ));
                            }
                            if !bound.is_empty() {
                                self.bound_generators.insert(decl.name.clone(), bound);
                            }
                        }
                    }
                }
            }
        }
    }
}
