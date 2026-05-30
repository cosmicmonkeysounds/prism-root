//! `loom-play` — single-process interactive driver for a Loom project.
//!
//! Loads a project from a directory, constructs a Playhead, and drives
//! it over a JSON-line stdio protocol so an external GUI (e.g. the
//! Python PySide6 simulator under `packages/loom/simulator/`) can play
//! a show without re-implementing the runtime.
//!
//! ## Protocol
//!
//! Each line on stdout is one JSON object with a `"type"` tag:
//! - `{"type":"ready","entry":"opening","diagnostics":[...]}`
//! - `{"type":"tracks","tracks":[{"id":N,"kind":"role|person|cohort|generator|booth|main","label":"...","driver":"..."}, ...]}`
//!   — emitted once on `ready` and again after `reload`
//! - `{"type":"event","idx":N,"track":N,"cause":N|null,"event":{...}}` — a ledger
//!   envelope with its mesh metadata (loom-editor.html §3)
//! - `{"type":"cells","track":N,"cells":[{"kind":"BeatVisit","bundle_ref":"...","start":N,"end":N|null,"snapshot":"..."}, ...]}`
//! - `{"type":"choice","options":[{"index":N,"text":"...","sticky":false}, ...]}`
//! - `{"type":"awaiting","coroutine":N}`
//! - `{"type":"ended"}`
//! - `{"type":"world","entries":[["key","display"], ...]}`
//! - `{"type":"error","message":"..."}`
//!
//! Each line on stdin is one JSON command:
//! - `{"cmd":"step"}` — advance until the next Choice / Awaiting / Ended
//! - `{"cmd":"choose","index":N}` — pick a choice
//! - `{"cmd":"world"}` — emit a world snapshot
//! - `{"cmd":"tracks"}` — re-emit the current track list
//! - `{"cmd":"cells","track":N}` — emit cells_for_track for one track
//! - `{"cmd":"skip"}` — booth skip beat
//! - `{"cmd":"force","raw":"sfx: ..."}` — booth force directive
//! - `{"cmd":"reload"}` — re-load the project from disk and hot-reload
//! - `{"cmd":"quit"}` — exit

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use loom_parser::ast::{BodyItem, Divert, Item};
use loom_runtime::{Bundle, Mesh, Step, TrackId, TrackIdentity};
use serde_json::{json, Value as JsonValue};

fn main() {
    let mut args = std::env::args().skip(1);
    let root = match args.next() {
        Some(p) => PathBuf::from(p),
        None => {
            emit_err("usage: loom-play <project-dir>");
            std::process::exit(2);
        }
    };

    let bundle = match Bundle::load(&root) {
        Ok(b) => Arc::new(b),
        Err(e) => {
            emit_err(&format!("failed to load project: {e}"));
            std::process::exit(1);
        }
    };

    let diagnostics: Vec<JsonValue> = collect_diagnostics(&bundle);
    let entry = bundle
        .entry
        .map(|r| bundle.beat(r).name.clone())
        .unwrap_or_default();

    let mut mesh = match Mesh::new(Arc::clone(&bundle)) {
        Ok(m) => m,
        Err(e) => {
            emit_err(&format!("mesh init: {e}"));
            std::process::exit(1);
        }
    };

    emit(json!({
        "type": "ready",
        "entry": entry,
        "diagnostics": diagnostics,
    }));
    emit_tracks(&mesh);

    // First batch — drive forward until we need user input.
    pump(&mut mesh);

    let stdin = io::stdin();
    let mut root = root;
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cmd: JsonValue = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                emit_err(&format!("bad command: {e}"));
                continue;
            }
        };
        match cmd.get("cmd").and_then(|v| v.as_str()).unwrap_or("") {
            "step" => pump(&mut mesh),
            "choose" => {
                let idx = cmd.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if let Err(e) = mesh.choose(idx) {
                    emit_err(&format!("choose: {e}"));
                } else {
                    pump(&mut mesh);
                }
            }
            "world" => emit_world(&mesh),
            "entities" => emit_entities(&bundle),
            "tracks" => emit_tracks(&mesh),
            "cells" => {
                let track = cmd
                    .get("track")
                    .and_then(|v| v.as_u64())
                    .map(|n| TrackId(n as u32))
                    .unwrap_or(TrackId::MAIN);
                emit_cells(&mesh, track);
            }
            "set" => {
                let key = cmd.get("key").and_then(|v| v.as_str()).unwrap_or("");
                match cmd.get("value") {
                    None => emit_err("set: missing key/value"),
                    Some(_) if key.is_empty() => emit_err("set: missing key/value"),
                    Some(value) => {
                        let lit = json_value_to_literal(value);
                        let raw = format!("set: {} = {}", key, lit);
                        mesh.playhead_mut().booth_force_directive(raw);
                        pump(&mut mesh);
                    }
                }
            }
            "skip" => {
                mesh.playhead_mut().booth_skip_beat();
                pump(&mut mesh);
            }
            "force" => {
                let raw = cmd
                    .get("raw")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                mesh.playhead_mut().booth_force_directive(raw);
                pump(&mut mesh);
            }
            "reload" => match Bundle::load(&root) {
                Ok(b) => {
                    let new_bundle = Arc::new(b);
                    if let Err(e) = mesh.playhead_mut().booth_hot_reload(Arc::clone(&new_bundle))
                    {
                        emit_err(&format!("hot reload: {e}"));
                    } else {
                        // Re-seed in case the new bundle introduced new
                        // ROLEs / PERSONs / generators.
                        mesh.seed_from_bundle(&new_bundle);
                        emit_tracks(&mesh);
                        pump(&mut mesh);
                    }
                }
                Err(e) => emit_err(&format!("reload load: {e}")),
            },
            "set_root" => {
                if let Some(p) = cmd.get("path").and_then(|v| v.as_str()) {
                    root = PathBuf::from(p);
                }
            }
            "quit" => break,
            other => emit_err(&format!("unknown cmd: {other}")),
        }
    }
}

fn pump(mesh: &mut Mesh) {
    // Drain Step::Event lines until we hit Choice / Awaiting / Ended
    // or exceed a safety budget for runaway loops. Each event is
    // tagged with its track id + ledger index + cause so the
    // simulator can build the multitrack canvas without a second pass.
    let mut budget = 10_000usize;
    loop {
        if mesh.playhead().halted() {
            emit(json!({ "type": "ended" }));
            return;
        }
        if budget == 0 {
            emit_err("step budget exhausted (10000)");
            return;
        }
        budget -= 1;
        // Snapshot the ledger end before stepping so the new envelopes
        // can be emitted with their (track, cause) metadata.
        let start = mesh.ledger().len();
        match mesh.step() {
            Ok((_step_track, Step::Event(_ev))) => {
                emit_new_envelopes(mesh, start);
            }
            Ok((_, Step::Choice(options))) => {
                emit_new_envelopes(mesh, start);
                emit(json!({
                    "type": "choice",
                    "options": options,
                }));
                return;
            }
            Ok((_, Step::Awaiting { coroutine })) => {
                emit_new_envelopes(mesh, start);
                emit(json!({
                    "type": "awaiting",
                    "coroutine": coroutine,
                }));
                return;
            }
            Ok((_, Step::Ended)) => {
                emit_new_envelopes(mesh, start);
                emit(json!({ "type": "ended" }));
                return;
            }
            Err(e) => {
                emit_err(&format!("step: {e}"));
                return;
            }
        }
    }
}

/// Emit every envelope written since `start` as a per-track-tagged
/// `event` line. One stdin step typically produces one envelope but
/// hook drains and booth directives can synthesise several; the
/// simulator handles them all uniformly.
fn emit_new_envelopes(mesh: &Mesh, start: usize) {
    let events = mesh.ledger().events();
    let meta = mesh.ledger().meta();
    for idx in start..events.len() {
        emit(json!({
            "type": "event",
            "idx": idx,
            "track": meta[idx].track.0,
            "cause": meta[idx].cause,
            "event": events[idx],
        }));
    }
}

fn emit_world(mesh: &Mesh) {
    let entries: Vec<(String, String)> = mesh
        .world()
        .entries()
        .map(|(k, v)| (k.clone(), v.display()))
        .collect();
    emit(json!({
        "type": "world",
        "entries": entries,
    }));
}

/// Emit the track list. Each track has an id, a human-readable label,
/// a kind tag the simulator uses for colour coding, and the driver.
fn emit_tracks(mesh: &Mesh) {
    let mut tracks: Vec<JsonValue> = mesh
        .tracks()
        .map(|t| {
            let (kind, name) = match &t.identity {
                TrackIdentity::Role(n) => ("role", n.as_str()),
                TrackIdentity::Person(n) => ("person", n.as_str()),
                TrackIdentity::Cohort(n) => ("cohort", n.as_str()),
                TrackIdentity::AmbientGenerator(n) => ("generator", n.as_str()),
                TrackIdentity::Booth => ("booth", "Booth"),
                TrackIdentity::Main => ("main", "Main"),
            };
            json!({
                "id": t.id.0,
                "kind": kind,
                "label": name,
                "driver": format!("{:?}", t.driver),
            })
        })
        .collect();
    // Stable canvas order: Booth first, then Main spine, then Roles,
    // then Persons, then Cohorts, then ambient Generators.
    tracks.sort_by_key(|t| {
        let kind = t.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let rank = match kind {
            "booth" => 0,
            "main" => 1,
            "role" => 2,
            "person" => 3,
            "cohort" => 4,
            "generator" => 5,
            _ => 6,
        };
        let id = t.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        (rank, id)
    });
    emit(json!({
        "type": "tracks",
        "tracks": tracks,
    }));
}

/// Emit `cells_for_track` for one track. The simulator pulls cells
/// lazily as it scrolls the multitrack canvas.
fn emit_cells(mesh: &Mesh, track: TrackId) {
    let cells: Vec<JsonValue> = mesh
        .cells_for_track(track)
        .iter()
        .map(|c| {
            json!({
                "track": c.track.0,
                "kind": format!("{:?}", c.kind),
                "bundle_ref": c.bundle_ref,
                "start": c.start,
                "end": c.end,
                "snapshot": c.snapshot,
            })
        })
        .collect();
    emit(json!({
        "type": "cells",
        "track": track.0,
        "cells": cells,
    }));
}

fn collect_diagnostics(bundle: &Bundle) -> Vec<JsonValue> {
    let mut out: Vec<JsonValue> = Vec::new();
    for (file, diag) in bundle.parser_diagnostics() {
        out.push(json!({
            "kind": "parser",
            "file": file.path.display().to_string(),
            "message": format!("{diag:?}"),
        }));
    }
    for diag in &bundle.project_diagnostics {
        out.push(json!({
            "kind": "project",
            "message": format!("{diag:?}"),
        }));
    }
    out
}

/// Convert a JSON value (from the simulator) into a Loom expression
/// literal for splicing into `<set: key = …>`. Bools / numbers / strings
/// are quoted appropriately; anything else falls back to a JSON-escaped
/// string so the expression parser still accepts it.
fn json_value_to_literal(v: &JsonValue) -> String {
    match v {
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => {
            // Bare identifiers (sum-type variants like `confirmed`) pass through;
            // anything with whitespace / punctuation gets quoted.
            if s.chars().all(|c| c.is_alphanumeric() || c == '_') && !s.is_empty() {
                s.clone()
            } else {
                format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
            }
        }
        JsonValue::Null => "null".to_string(),
        other => format!("\"{}\"", other.to_string().replace('"', "\\\"")),
    }
}

/// Walk the bundle and emit a `{type:"entities", …}` envelope: a
/// compact graph schema describing every character / cohort / location
/// / beat / item / faction in the project. Consumed by the simulator's
/// World tree + node-canvas views. Edges fold together divert targets,
/// trust dispositions, and beat cast/setting wiring so the canvas can
/// draw them without re-parsing on the client.
fn emit_entities(bundle: &Bundle) {
    let mut characters = Vec::new();
    for ch in bundle.characters.values() {
        let dispositions: Vec<JsonValue> = ch
            .disposition
            .iter()
            .map(|((verb, target), axis)| {
                json!({
                    "verb": verb,
                    "target": target,
                    "current": axis.current,
                    "max": axis.max,
                })
            })
            .collect();
        let knowledge: Vec<JsonValue> = ch
            .knowledge
            .iter()
            .map(|(k, v)| {
                json!({
                    "field": k,
                    "value": v.display(),
                    "schema": ch.knowledge_schema.get(k).cloned().unwrap_or_default(),
                })
            })
            .collect();
        characters.push(json!({
            "name": ch.name,
            "inherits": ch.inherits,
            "properties": ch.properties,
            "disposition": dispositions,
            "knowledge": knowledge,
            "reacts_tags": ch.reacts_tags.iter().cloned().collect::<Vec<_>>(),
            "has_stats": ch.stats.is_some(),
        }));
    }

    let cohorts: Vec<JsonValue> = bundle
        .cohorts
        .iter()
        .map(|(name, body)| {
            json!({
                "name": name,
                "label": body.label,
                "capacity": body.capacity,
            })
        })
        .collect();

    let locations: Vec<JsonValue> = bundle
        .locations
        .iter()
        .map(|(name, body)| {
            json!({
                "name": name,
                "label": body.label,
                "ambient": body.ambient,
                "capacity": body.capacity,
                "contains": body.contains,
            })
        })
        .collect();

    // Beats — flatten outgoing diverts + choices for the canvas. We
    // surface bare target names (no qualifier resolution) which is
    // enough for the visual; the runtime still owns dispatch.
    let mut beats: Vec<JsonValue> = Vec::new();
    for file_entry in &bundle.files {
        let file_path = file_entry.path.display().to_string();
        for item in &file_entry.file.items {
            let Item::Beat(beat) = item else { continue };
            let cast = beat
                .contract
                .get("cast")
                .map(|p| p.value.clone())
                .unwrap_or_default();
            let setting = beat
                .contract
                .get("setting")
                .map(|p| p.value.clone())
                .unwrap_or_default();
            let mut diverts: Vec<String> = Vec::new();
            let mut choices: Vec<JsonValue> = Vec::new();
            collect_targets(&beat.body, &mut diverts, &mut choices);
            beats.push(json!({
                "name": beat.name,
                "file": file_path,
                "cast": cast,
                "setting": setting,
                "diverts": diverts,
                "choices": choices,
            }));
        }
    }

    let items: Vec<JsonValue> = bundle.items.keys().map(|n| json!({ "name": n })).collect();
    let factions: Vec<JsonValue> = bundle
        .factions
        .keys()
        .map(|n| json!({ "name": n }))
        .collect();
    let stats_profiles: Vec<JsonValue> = bundle
        .stats_profiles
        .keys()
        .map(|n| json!({ "name": n }))
        .collect();

    let entry = bundle
        .entry
        .map(|r| bundle.beat(r).name.clone())
        .unwrap_or_default();

    emit(json!({
        "type": "entities",
        "entry": entry,
        "characters": characters,
        "cohorts": cohorts,
        "locations": locations,
        "beats": beats,
        "items": items,
        "factions": factions,
        "stats_profiles": stats_profiles,
    }));
}

fn collect_targets(body: &[BodyItem], diverts: &mut Vec<String>, choices: &mut Vec<JsonValue>) {
    use loom_parser::ast::BodyItem as BI;
    for item in body {
        match item {
            BI::Divert(d) => push_divert(d, diverts),
            BI::Choice(c) => {
                let mut targets = Vec::new();
                collect_targets(&c.body, &mut targets, &mut Vec::new());
                choices.push(json!({
                    "text": c.text,
                    "sticky": c.sticky,
                    "targets": targets,
                }));
            }
            BI::Conditional(cond) => {
                for arm in &cond.arms {
                    collect_targets(&arm.body, diverts, choices);
                }
            }
            BI::Match(m) => {
                for arm in &m.arms {
                    collect_targets(&arm.body, diverts, choices);
                }
            }
            BI::EachVisit(ev) => {
                collect_targets(&ev.first, diverts, choices);
                collect_targets(&ev.then, diverts, choices);
                collect_targets(&ev.finally, diverts, choices);
            }
            _ => {}
        }
    }
}

fn push_divert(d: &Divert, out: &mut Vec<String>) {
    match d {
        Divert::To { target, .. } | Divert::Tunnel { target, .. } => out.push(target.name.clone()),
        Divert::End { .. } => out.push("END".into()),
        Divert::Return { .. } => {}
    }
}

fn emit(v: JsonValue) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{}", v);
    let _ = stdout.flush();
}

fn emit_err(msg: &str) {
    emit(json!({ "type": "error", "message": msg }));
}
