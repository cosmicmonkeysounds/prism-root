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
//! - `{"type":"event","event":{...}}` — a ledger `Event` envelope
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
//! - `{"cmd":"skip"}` — booth skip beat
//! - `{"cmd":"force","raw":"sfx: ..."}` — booth force directive
//! - `{"cmd":"reload"}` — re-load the project from disk and hot-reload
//! - `{"cmd":"quit"}` — exit

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use loom_parser::ast::{BodyItem, Divert, Item};
use loom_runtime::{Bundle, Playhead, Step};
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

    let mut playhead = match Playhead::new(Arc::clone(&bundle)) {
        Ok(p) => p,
        Err(e) => {
            emit_err(&format!("playhead init: {e}"));
            std::process::exit(1);
        }
    };

    emit(json!({
        "type": "ready",
        "entry": entry,
        "diagnostics": diagnostics,
    }));

    // First batch — drive forward until we need user input.
    pump(&mut playhead);

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
            "step" => pump(&mut playhead),
            "choose" => {
                let idx = cmd.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if let Err(e) = playhead.choose(idx) {
                    emit_err(&format!("choose: {e}"));
                } else {
                    pump(&mut playhead);
                }
            }
            "world" => emit_world(&playhead),
            "entities" => emit_entities(&bundle),
            "set" => {
                let key = cmd.get("key").and_then(|v| v.as_str()).unwrap_or("");
                match cmd.get("value") {
                    None => emit_err("set: missing key/value"),
                    Some(_) if key.is_empty() => emit_err("set: missing key/value"),
                    Some(value) => {
                        let lit = json_value_to_literal(value);
                        let raw = format!("set: {} = {}", key, lit);
                        playhead.booth_force_directive(raw);
                        pump(&mut playhead);
                    }
                }
            }
            "skip" => {
                playhead.booth_skip_beat();
                pump(&mut playhead);
            }
            "force" => {
                let raw = cmd
                    .get("raw")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                playhead.booth_force_directive(raw);
                pump(&mut playhead);
            }
            "reload" => match Bundle::load(&root) {
                Ok(b) => {
                    if let Err(e) = playhead.booth_hot_reload(Arc::new(b)) {
                        emit_err(&format!("hot reload: {e}"));
                    } else {
                        pump(&mut playhead);
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

fn pump(playhead: &mut Playhead) {
    // Drain Step::Event lines until we hit Choice / Awaiting / Ended
    // or exceed a safety budget for runaway loops.
    let mut budget = 10_000usize;
    loop {
        if playhead.halted() {
            emit(json!({ "type": "ended" }));
            return;
        }
        if budget == 0 {
            emit_err("step budget exhausted (10000)");
            return;
        }
        budget -= 1;
        match playhead.step() {
            Ok(Step::Event(ev)) => emit(json!({
                "type": "event",
                "event": ev,
            })),
            Ok(Step::Choice(options)) => {
                emit(json!({
                    "type": "choice",
                    "options": options,
                }));
                return;
            }
            Ok(Step::Awaiting { coroutine }) => {
                emit(json!({
                    "type": "awaiting",
                    "coroutine": coroutine,
                }));
                return;
            }
            Ok(Step::Ended) => {
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

fn emit_world(playhead: &Playhead) {
    let entries: Vec<(String, String)> = playhead
        .world()
        .entries()
        .map(|(k, v)| (k.clone(), v.display()))
        .collect();
    emit(json!({
        "type": "world",
        "entries": entries,
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
