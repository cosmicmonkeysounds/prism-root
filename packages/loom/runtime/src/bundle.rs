//! Compiled program — a parsed project indexed for runtime lookup.
//!
//! A [`Bundle`] holds every `.loom` file in a project plus the
//! cross-file name indices the [`crate::resolver`] reads against.
//! Construction (parsing + indexing) lives in [`crate::project`];
//! resolution lives in [`crate::resolver`].

use std::collections::HashMap;
use std::path::PathBuf;

use loom_parser::ast::{Beat, Item, LoomFile};
use loom_parser::Diagnostic;

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
}
