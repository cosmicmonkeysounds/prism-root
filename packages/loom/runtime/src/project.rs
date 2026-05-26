//! Project loader — walks a folder of `.loom` files, parses each
//! through `loom_parser`, and produces a [`Bundle`] with the
//! cross-file indices populated (spec §3).
//!
//! Path discipline:
//! * Project root is the directory containing `main.loom`.
//! * Names are flat across the project (Obsidian / Twine convention).
//! * The folder tree is for humans, not the resolver — it only
//!   participates when a divert qualifies with `Folder/name`.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use loom_parser::ast::Item;
use loom_parser::parse;

use crate::bundle::{BeatIdx, BeatRef, Bundle, FileIdx, LoomFileEntry, ProjectDiagnostic};

impl Bundle {
    /// Build a bundle from an explicit map of `(relative_path,
    /// source_text)` pairs. Useful for tests + in-memory projects.
    pub fn from_sources<I, P, S>(sources: I) -> Self
    where
        I: IntoIterator<Item = (P, S)>,
        P: Into<PathBuf>,
        S: Into<String>,
    {
        let mut bundle = Bundle::default();
        for (path, source) in sources {
            let path = path.into();
            let source = source.into();
            let (file, diagnostics) = parse(&source);
            let (stem, qualifier) = split_stem_and_qualifier(&path);
            bundle.files.push(LoomFileEntry {
                path,
                stem,
                qualifier,
                source,
                file,
                diagnostics,
            });
        }
        bundle.rebuild_indices();
        bundle
    }

    /// Walk a directory recursively for `*.loom` files and load them
    /// all. Files unreadable due to I/O errors are skipped silently;
    /// the caller can call this followed by their own `read_dir` for
    /// stricter behaviour.
    pub fn load(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref();
        let mut sources: Vec<(PathBuf, String)> = Vec::new();
        walk(root, root, &mut sources)?;
        Ok(Self::from_sources(sources))
    }

    /// (Re)compute the cross-file indices + entry pointer from
    /// `self.files`. Called automatically by [`Self::from_sources`];
    /// exposed for callers that mutate files in place.
    pub fn rebuild_indices(&mut self) {
        let mut beats_by_name: HashMap<String, Vec<BeatRef>> = HashMap::new();
        let mut files_by_stem: HashMap<String, Vec<FileIdx>> = HashMap::new();
        let mut main_idx: Option<FileIdx> = None;

        for (file_idx, entry) in self.files.iter().enumerate() {
            let file_idx = file_idx as FileIdx;
            files_by_stem
                .entry(entry.stem.clone())
                .or_default()
                .push(file_idx);
            for (item_idx, item) in entry.file.items.iter().enumerate() {
                if let Item::Beat(beat) = item {
                    beats_by_name
                        .entry(beat.name.clone())
                        .or_default()
                        .push(BeatRef {
                            file: file_idx,
                            beat: item_idx as BeatIdx,
                        });
                }
            }
            if entry
                .path
                .file_name()
                .map(|n| n == "main.loom")
                .unwrap_or(false)
                && entry.qualifier.is_empty()
            {
                main_idx = Some(file_idx);
            }
        }

        let mut project_diagnostics: Vec<ProjectDiagnostic> = Vec::new();
        let entry = match main_idx {
            None => {
                project_diagnostics.push(ProjectDiagnostic::MissingMainFile);
                None
            }
            Some(idx) => {
                let main = &self.files[idx as usize];
                if let Some(entry_prop) = main.file.header.properties.get("entry") {
                    let candidates = beats_by_name
                        .get(&entry_prop.value)
                        .cloned()
                        .unwrap_or_default();
                    if candidates.len() == 1 {
                        Some(candidates[0])
                    } else if candidates.is_empty() {
                        project_diagnostics.push(ProjectDiagnostic::EntryBeatUnresolved {
                            name: entry_prop.value.clone(),
                        });
                        None
                    } else {
                        // Multiple candidates — prefer one inside main.loom
                        // itself, else first declared.
                        candidates
                            .iter()
                            .find(|r| r.file == idx)
                            .or_else(|| candidates.first())
                            .copied()
                    }
                } else {
                    // No explicit entry: use main.loom's first beat.
                    main.entry_beat_index()
                        .map(|b| BeatRef { file: idx, beat: b })
                        .or_else(|| {
                            project_diagnostics.push(ProjectDiagnostic::NoEntryBeat);
                            None
                        })
                }
            }
        };

        self.beats_by_name = beats_by_name;
        self.files_by_stem = files_by_stem;
        self.entry = entry;
        self.project_diagnostics = project_diagnostics;
        self.rebuild_simulacra();
    }
}

/// Split `cast/Wren.loom` into `(stem = "Wren", qualifier = "cast")`.
/// Bare-root files have an empty qualifier. Always uses forward
/// slashes regardless of host OS — divert qualifiers in source are
/// `/`-separated (spec §7).
fn split_stem_and_qualifier(path: &Path) -> (String, String) {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let parent_segments: Vec<&str> = path
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| c.as_os_str().to_str())
                .filter(|s| !s.is_empty() && *s != ".")
                .collect()
        })
        .unwrap_or_default();
    (stem, parent_segments.join("/"))
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, String)>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ftype = entry.file_type()?;
        if ftype.is_dir() {
            walk(root, &path, out)?;
            continue;
        }
        if path.extension().map(|e| e == "loom").unwrap_or(false) {
            let source = std::fs::read_to_string(&path)?;
            let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            out.push((rel, source));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_sources_indexes_beats_and_files() {
        let bundle = Bundle::from_sources([
            (
                "main.loom",
                "# Show\nentry: opening\n\n== opening\n  cast: Wren\n\n* Go.\n  -> ringing\n",
            ),
            (
                "beats/ringing.loom",
                "== ringing\n  cast: Wren\n\nIt rings.\n",
            ),
        ]);
        assert!(
            bundle.project_diagnostics.is_empty(),
            "{:?}",
            bundle.project_diagnostics
        );
        let entry = bundle.entry.unwrap();
        assert_eq!(bundle.beat(entry).name, "opening");
        assert!(bundle.beats_by_name.contains_key("ringing"));
    }

    #[test]
    fn missing_main_reports_diagnostic() {
        let bundle = Bundle::from_sources([("solo.loom", "== solo\n\nHello.\n")]);
        assert!(bundle.entry.is_none());
        assert!(bundle
            .project_diagnostics
            .contains(&ProjectDiagnostic::MissingMainFile));
    }

    #[test]
    fn entry_property_with_no_match_reports() {
        let bundle =
            Bundle::from_sources([("main.loom", "# Show\nentry: nope\n\n== opening\n\n.\n")]);
        assert!(matches!(
            bundle.project_diagnostics[0],
            ProjectDiagnostic::EntryBeatUnresolved { .. }
        ));
    }

    #[test]
    fn default_entry_falls_back_to_first_beat() {
        let bundle = Bundle::from_sources([("main.loom", "== first\n\n.\n\n== second\n\n.\n")]);
        assert!(bundle.project_diagnostics.is_empty());
        let entry = bundle.entry.unwrap();
        assert_eq!(bundle.beat(entry).name, "first");
    }

    #[test]
    fn required_slot_emits_diagnostic_and_skips_instantiation() {
        let bundle = Bundle::from_sources([(
            "main.loom",
            "# Show\nentry: opening\n\n== opening\n  cast: Keeper\n\nDone.\n\nCHARACTER Keeper\n  voice: any\n",
        )]);
        assert!(bundle.project_diagnostics.iter().any(|d| matches!(
            d,
            ProjectDiagnostic::RequiredSlotUnfilled { character, slot }
                if character == "Keeper" && slot == "voice"
        )));
        assert!(!bundle.characters.contains_key("Keeper"));
    }

    #[test]
    fn child_character_fills_required_slot_from_parent() {
        let bundle = Bundle::from_sources([(
            "main.loom",
            "# Show\nentry: opening\n\n== opening\n\nDone.\n\nCHARACTER Keeper\n  voice: any\n\nCHARACTER Wren is Keeper\n  voice: female_alto\n",
        )]);
        // Wren fills voice; Keeper does not.
        assert!(bundle.characters.contains_key("Wren"));
        assert!(bundle.project_diagnostics.iter().any(|d| matches!(
            d,
            ProjectDiagnostic::RequiredSlotUnfilled { character, .. } if character == "Keeper"
        )));
    }

    #[test]
    fn item_inherits_parent_properties() {
        let bundle = Bundle::from_sources([(
            "main.loom",
            "# Show\nentry: opening\n\n== opening\n\nDone.\n\nITEM LootBag\n  contents: list of ITEM = []\n  gold:     int          = 0\n\nITEM goblin_pouch is LootBag\n  gold:     3\n",
        )]);
        assert!(
            bundle.project_diagnostics.is_empty(),
            "{:?}",
            bundle.project_diagnostics
        );
        let pouch = bundle.items.get("goblin_pouch").expect("pouch present");
        // contents inherited from LootBag, gold overridden to 3.
        let names: Vec<_> = pouch.properties.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"contents"));
        assert!(names.contains(&"gold"));
        let gold = pouch.properties.iter().find(|p| p.name == "gold").unwrap();
        // Override carries the literal `3` as either the raw type
        // spelling (when the line was `gold: 3`) or the default.
        let value = gold
            .default
            .clone()
            .or_else(|| gold.raw_type.clone())
            .unwrap_or_default();
        assert_eq!(value, "3");
    }

    #[test]
    fn faction_parses_into_bundle() {
        let bundle = Bundle::from_sources([(
            "main.loom",
            "# Show\nentry: opening\n\n== opening\n\nDone.\n\nFACTION Guild\n  members: list of CHARACTER = []\n",
        )]);
        assert!(bundle.factions.contains_key("Guild"));
    }

    #[test]
    fn split_stem_and_qualifier_handles_nested_paths() {
        let (stem, qual) = split_stem_and_qualifier(Path::new("beats/act_two/revelation.loom"));
        assert_eq!(stem, "revelation");
        assert_eq!(qual, "beats/act_two");
        let (stem, qual) = split_stem_and_qualifier(Path::new("main.loom"));
        assert_eq!(stem, "main");
        assert_eq!(qual, "");
    }
}
