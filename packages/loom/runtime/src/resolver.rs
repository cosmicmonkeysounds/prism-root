//! Divert resolver — turn a parsed [`DivertTarget`] into a concrete
//! [`BeatRef`] against the project (spec §3, §7).
//!
//! Resolution rules (spec §18 open-q: errors loudly on ambiguity,
//! never guesses):
//!
//! * **`-> name`** — bare name. Look up `beats_by_name[name]`. One
//!   match → done. Zero → fall back to `files_by_stem[name]` and
//!   pick its entry beat. Multiple → `ResolveError::Ambiguous`.
//! * **`-> Folder/name`** — folder-qualified. Filter
//!   `beats_by_name[name]` to beats whose file's qualifier ends with
//!   `Folder`. Zero → unresolved. Multiple → ambiguous.
//! * **`-> file#knot`** — explicit knot inside a file. Find the file
//!   (by stem, optionally folder-qualified), then find a beat named
//!   `knot` inside it.
//! * **`-> END`** — handled by [`crate::playhead`], not here.
//! * **`<-`** — handled by [`crate::playhead`], not here.

use loom_parser::ast::DivertTarget;
use thiserror::Error;

use crate::bundle::{BeatRef, Bundle, FileIdx};

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ResolveError {
    #[error("no beat or file named `{name}`")]
    Unresolved { name: String },
    #[error("`{name}` is ambiguous; qualify with a folder (e.g. `Folder/{name}`)")]
    Ambiguous {
        name: String,
        candidates: Vec<BeatRef>,
    },
    #[error("file `{file}` does not declare a beat named `{knot}`")]
    UnknownKnot { file: String, knot: String },
}

impl Bundle {
    /// Resolve a divert reference. `from_file` is the file the
    /// divert originated in — used to disambiguate a bare name that
    /// matches multiple beats by preferring the one declared in the
    /// caller's file (spec §18 resolution heuristic).
    pub fn resolve_divert(
        &self,
        from_file: FileIdx,
        target: &DivertTarget,
    ) -> Result<BeatRef, ResolveError> {
        // Case 3: explicit knot — `file#knot`.
        if let Some(knot) = &target.knot {
            let file_idx = self.resolve_file(target)?;
            let entry = &self.files[file_idx as usize];
            for (item_idx, item) in entry.file.items.iter().enumerate() {
                if let loom_parser::ast::Item::Beat(beat) = item {
                    if &beat.name == knot {
                        return Ok(BeatRef {
                            file: file_idx,
                            beat: item_idx as u32,
                        });
                    }
                }
            }
            return Err(ResolveError::UnknownKnot {
                file: entry.stem.clone(),
                knot: knot.clone(),
            });
        }

        // Case 2: folder-qualified beat — `Folder/name`.
        if let Some(qualifier) = &target.qualifier {
            let candidates: Vec<BeatRef> = self
                .beats_by_name
                .get(&target.name)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|r| qualifier_matches(&self.files[r.file as usize].qualifier, qualifier))
                .collect();
            return match candidates.len() {
                0 => Err(ResolveError::Unresolved {
                    name: format!("{}/{}", qualifier, target.name),
                }),
                1 => Ok(candidates[0]),
                _ => Err(ResolveError::Ambiguous {
                    name: format!("{}/{}", qualifier, target.name),
                    candidates,
                }),
            };
        }

        // Case 1: bare name → beat lookup → fall back to file lookup.
        if let Some(beats) = self.beats_by_name.get(&target.name) {
            return match beats.len() {
                1 => Ok(beats[0]),
                _ => {
                    // Spec §18 heuristic — prefer a candidate
                    // declared in the calling file before reporting
                    // the ambiguity. Cross-file ambiguity still
                    // errors so the author hears about it.
                    let same_file: Vec<BeatRef> = beats
                        .iter()
                        .copied()
                        .filter(|r| r.file == from_file)
                        .collect();
                    if same_file.len() == 1 {
                        Ok(same_file[0])
                    } else {
                        Err(ResolveError::Ambiguous {
                            name: target.name.clone(),
                            candidates: beats.clone(),
                        })
                    }
                }
            };
        }
        if let Some(files) = self.files_by_stem.get(&target.name) {
            if files.len() == 1 {
                let file_idx = files[0];
                if let Some(beat_idx) = self.files[file_idx as usize].entry_beat_index() {
                    return Ok(BeatRef {
                        file: file_idx,
                        beat: beat_idx,
                    });
                }
            } else if files.len() > 1 {
                return Err(ResolveError::Ambiguous {
                    name: target.name.clone(),
                    candidates: vec![],
                });
            }
        }
        Err(ResolveError::Unresolved {
            name: target.name.clone(),
        })
    }

    /// Resolve a file reference (used by the `#knot` form). When
    /// the target carries no qualifier, the file is found by stem;
    /// when qualified, the qualifier must match the file's folder
    /// path.
    fn resolve_file(&self, target: &DivertTarget) -> Result<FileIdx, ResolveError> {
        let pool = self
            .files_by_stem
            .get(&target.name)
            .cloned()
            .unwrap_or_default();
        let filtered: Vec<FileIdx> = match &target.qualifier {
            Some(q) => pool
                .into_iter()
                .filter(|f| qualifier_matches(&self.files[*f as usize].qualifier, q))
                .collect(),
            None => pool,
        };
        match filtered.len() {
            0 => Err(ResolveError::Unresolved {
                name: target.name.clone(),
            }),
            1 => Ok(filtered[0]),
            _ => Err(ResolveError::Ambiguous {
                name: target.name.clone(),
                candidates: vec![],
            }),
        }
    }
}

/// Spec §7 wording is loose — "disambiguated by folder path". We
/// treat the qualifier as a *suffix* of the file's folder path so
/// `-> Lighthouse/ringing` works whether the beat lives at
/// `beats/Lighthouse/ringing.loom` or just `Lighthouse/ringing.loom`.
fn qualifier_matches(file_qualifier: &str, divert_qualifier: &str) -> bool {
    if file_qualifier == divert_qualifier {
        return true;
    }
    if file_qualifier.ends_with(divert_qualifier) {
        let cut = file_qualifier.len() - divert_qualifier.len();
        // Ensure we're matching on a path boundary, not mid-segment.
        if cut == 0 || file_qualifier.as_bytes()[cut - 1] == b'/' {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(name: &str) -> DivertTarget {
        DivertTarget {
            qualifier: None,
            name: name.into(),
            knot: None,
        }
    }

    fn qualified(qualifier: &str, name: &str) -> DivertTarget {
        DivertTarget {
            qualifier: Some(qualifier.into()),
            name: name.into(),
            knot: None,
        }
    }

    #[test]
    fn bare_name_resolves_to_single_beat() {
        let bundle = Bundle::from_sources([
            ("main.loom", "entry: opening\n\n== opening\n\n.\n"),
            ("beats/ringing.loom", "== ringing\n\n.\n"),
        ]);
        let r = bundle.resolve_divert(0, &target("ringing")).unwrap();
        assert_eq!(bundle.beat(r).name, "ringing");
    }

    #[test]
    fn bare_name_falls_back_to_file_entry() {
        let bundle = Bundle::from_sources([
            ("main.loom", "entry: opening\n\n== opening\n\n.\n"),
            ("cast/Wren.loom", "== greet\n\n.\n"),
        ]);
        let r = bundle.resolve_divert(0, &target("Wren")).unwrap();
        assert_eq!(bundle.beat(r).name, "greet");
    }

    #[test]
    fn ambiguous_bare_name_errors() {
        let bundle = Bundle::from_sources([
            ("main.loom", "entry: opening\n\n== opening\n\n.\n"),
            ("beats/a/ringing.loom", "== ringing\n\n.\n"),
            ("beats/b/ringing.loom", "== ringing\n\n.\n"),
        ]);
        match bundle.resolve_divert(0, &target("ringing")) {
            Err(ResolveError::Ambiguous { name, candidates }) => {
                assert_eq!(name, "ringing");
                assert_eq!(candidates.len(), 2);
            }
            other => panic!("expected ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn same_file_wins_over_cross_file_ambiguity() {
        // `ringing` exists in two beats; resolve from `a`'s file
        // should pick `a/ringing` rather than error (spec §18).
        let bundle = Bundle::from_sources([
            ("main.loom", "entry: opening\n\n== opening\n\n.\n"),
            ("beats/a/ringing.loom", "== ringing\n\n.\n"),
            ("beats/b/ringing.loom", "== ringing\n\n.\n"),
        ]);
        let from_a = bundle
            .beats_by_name
            .get("ringing")
            .and_then(|v| {
                v.iter()
                    .find(|r| bundle.files[r.file as usize].qualifier.ends_with("/a"))
            })
            .copied()
            .expect("a/ringing exists");
        let r = bundle
            .resolve_divert(from_a.file, &target("ringing"))
            .unwrap();
        assert_eq!(r.file, from_a.file);
    }

    #[test]
    fn qualifier_disambiguates() {
        let bundle = Bundle::from_sources([
            ("main.loom", "entry: opening\n\n== opening\n\n.\n"),
            ("beats/a/ringing.loom", "== ringing\n\n.\n"),
            ("beats/b/ringing.loom", "== ringing\n\n.\n"),
        ]);
        let r = bundle
            .resolve_divert(0, &qualified("a", "ringing"))
            .unwrap();
        assert!(bundle.files[r.file as usize].qualifier.ends_with("/a"));
    }

    #[test]
    fn knot_form_finds_beat_in_file() {
        let bundle = Bundle::from_sources([
            ("main.loom", "entry: opening\n\n== opening\n\n.\n"),
            ("cast/Wren.loom", "== greet\n\n.\n\n== backstory\n\n.\n"),
        ]);
        let t = DivertTarget {
            qualifier: Some("cast".into()),
            name: "Wren".into(),
            knot: Some("backstory".into()),
        };
        let r = bundle.resolve_divert(0, &t).unwrap();
        assert_eq!(bundle.beat(r).name, "backstory");
    }

    #[test]
    fn unresolved_errors_loudly() {
        let bundle = Bundle::from_sources([("main.loom", "entry: opening\n\n== opening\n\n.\n")]);
        match bundle.resolve_divert(0, &target("nowhere")) {
            Err(ResolveError::Unresolved { name }) => assert_eq!(name, "nowhere"),
            other => panic!("{other:?}"),
        }
    }
}
