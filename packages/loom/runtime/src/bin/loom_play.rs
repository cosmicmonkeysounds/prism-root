//! `loom-play` — multi-head interactive driver for a Loom project.
//!
//! Loads a project from a directory, constructs one default `Mesh`
//! (head `"h0"`), and drives any number of additional heads over a
//! JSON-line stdio protocol. Every head is an independent `Playhead`
//! / `Mesh` over the same shared `Arc<Bundle>`; snapshots are cheap
//! clones of mutable state only.
//!
//! ## Protocol
//!
//! Each line on stdout is one JSON object with a `"type"` tag. Every
//! payload that pertains to a specific head carries `"head": "hN"`
//! (default `"h0"`):
//! - `{"type":"ready","head":"h0","entry":"opening","diagnostics":[...]}`
//! - `{"type":"tracks","head":"h0","tracks":[...]}`
//!   — re-emitted on reload and after fork
//! - `{"type":"event","head":"h0","idx":N,"track":N,"cause":N|null,"event":{...}}`
//! - `{"type":"cells","head":"h0","track":N,"cells":[...]}`
//! - `{"type":"choice","head":"h0","options":[...]}`
//! - `{"type":"awaiting","head":"h0","coroutine":N}`
//! - `{"type":"ended","head":"h0"}`
//! - `{"type":"world","head":"h0","entries":[...]}`
//! - `{"type":"snapshot","head":"h0","id":"s7","at":N}`
//! - `{"type":"restored","head":"h0","id":"s7"}`
//! - `{"type":"forked","head":"h1","from":"s7"|null,"parent":"h0"}`
//! - `{"type":"dropped","head":"h1"}`
//! - `{"type":"heads","heads":[{"id":"h0","primary":true,"halted":false,"ledger_len":N}, ...]}`
//! - `{"type":"error","message":"..."}` (head-agnostic)
//!
//! Each line on stdin is one JSON command. Per-head commands accept
//! an optional `"head"` (default `"h0"`):
//! - `{"cmd":"step","head":"h0"}` — advance until the next Choice / Awaiting / Ended
//! - `{"cmd":"choose","index":N,"head":"h0"}` — pick a choice
//! - `{"cmd":"world","head":"h0"}` — emit a world snapshot
//! - `{"cmd":"tracks","head":"h0"}` — re-emit the current track list
//! - `{"cmd":"cells","track":N,"head":"h0"}` — emit cells_for_track for one track
//! - `{"cmd":"skip","head":"h0"}` — booth skip beat
//! - `{"cmd":"force","raw":"sfx: ...","head":"h0"}` — booth force directive
//! - `{"cmd":"set","key":"...","value":...,"head":"h0"}` — route a `<set:>` through the booth
//! - `{"cmd":"reload","head":"h0"}` — re-load the project from disk and hot-reload (all heads if head omitted? — current: per head)
//! - `{"cmd":"snapshot","head":"h0"}` — capture; reply carries the new id
//! - `{"cmd":"restore","head":"h0","id":"s7"}` — overwrite head with snapshot
//! - `{"cmd":"fork","head":"h0"}` or `{"cmd":"fork","from":"s7"}` — spawn a new head
//! - `{"cmd":"drop","head":"h1"}` — discard a non-primary head
//! - `{"cmd":"heads"}` — re-emit the head listing
//! - `{"cmd":"entities"}` — emit the bundle-wide entity schema (head-agnostic)
//! - `{"cmd":"quit"}` — exit

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use loom_parser::ast::{BodyItem, Divert, Item};
use loom_runtime::{Bundle, Mesh, MeshSnapshot, Step, TrackId, TrackIdentity};
use serde_json::{json, Value as JsonValue};

/// Identifier handed back to clients for any per-head wire address.
type HeadId = String;
/// Identifier handed back to clients for any captured snapshot.
type SnapshotId = String;

/// Mutable driver state shared across the dispatch loop.
struct Driver {
    bundle: Arc<Bundle>,
    root: PathBuf,
    heads: HashMap<HeadId, Mesh>,
    primary: HeadId,
    snapshots: HashMap<SnapshotId, (HeadId, MeshSnapshot, usize)>,
    next_head_id: u64,
    next_snapshot_id: u64,
}

impl Driver {
    fn new(bundle: Arc<Bundle>, root: PathBuf, mesh: Mesh) -> Self {
        let primary: HeadId = "h0".into();
        let mut heads = HashMap::new();
        heads.insert(primary.clone(), mesh);
        Self {
            bundle,
            root,
            heads,
            primary,
            snapshots: HashMap::new(),
            next_head_id: 1,
            next_snapshot_id: 0,
        }
    }

    fn fresh_head_id(&mut self) -> HeadId {
        let id = format!("h{}", self.next_head_id);
        self.next_head_id += 1;
        id
    }

    fn fresh_snapshot_id(&mut self) -> SnapshotId {
        let id = format!("s{}", self.next_snapshot_id);
        self.next_snapshot_id += 1;
        id
    }

    fn head_of(cmd: &JsonValue, default: &str) -> HeadId {
        cmd.get("head")
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    }
}

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

    let mesh = match Mesh::new(Arc::clone(&bundle)) {
        Ok(m) => m,
        Err(e) => {
            emit_err(&format!("mesh init: {e}"));
            std::process::exit(1);
        }
    };

    let mut driver = Driver::new(Arc::clone(&bundle), root, mesh);
    let primary = driver.primary.clone();

    emit(json!({
        "type": "ready",
        "head": primary,
        "entry": entry,
        "diagnostics": diagnostics,
    }));
    emit_tracks(&driver, &primary);

    // First batch — drive forward until the primary head needs user input.
    pump(&mut driver, &primary);

    let stdin = io::stdin();
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
        let kind = cmd.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "step" => with_head(&mut driver, &cmd, pump),
            "choose" => {
                let idx = cmd.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                with_head(&mut driver, &cmd, |d, h| {
                    let mesh = d.heads.get_mut(h).expect("head exists (checked)");
                    match mesh.choose(idx) {
                        Err(e) => emit_err(&format!("choose: {e}")),
                        Ok(()) => pump(d, h),
                    }
                })
            }
            "world" => with_head(&driver, &cmd, emit_world),
            "tracks" => with_head(&driver, &cmd, emit_tracks),
            "entities" => emit_entities(&driver.bundle),
            "cells" => with_head(&driver, &cmd, |d, h| {
                let track = cmd
                    .get("track")
                    .and_then(|v| v.as_u64())
                    .map(|n| TrackId(n as u32))
                    .unwrap_or(TrackId::MAIN);
                emit_cells(d, h, track);
            }),
            "set" => {
                let key = cmd.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                match cmd.get("value") {
                    None => emit_err("set: missing key/value"),
                    Some(_) if key.is_empty() => emit_err("set: missing key/value"),
                    Some(value) => {
                        let lit = json_value_to_literal(value);
                        let raw = format!("set: {} = {}", key, lit);
                        with_head(&mut driver, &cmd, |d, h| {
                            let mesh = d.heads.get_mut(h).expect("head exists (checked)");
                            mesh.playhead_mut().booth_force_directive(raw.clone());
                            pump(d, h);
                        });
                    }
                }
            }
            "skip" => with_head(&mut driver, &cmd, |d, h| {
                let mesh = d.heads.get_mut(h).expect("head exists (checked)");
                mesh.playhead_mut().booth_skip_beat();
                pump(d, h);
            }),
            "force" => {
                let raw = cmd
                    .get("raw")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                with_head(&mut driver, &cmd, |d, h| {
                    let mesh = d.heads.get_mut(h).expect("head exists (checked)");
                    mesh.playhead_mut().booth_force_directive(raw.clone());
                    pump(d, h);
                })
            }
            "reload" => match Bundle::load(&driver.root) {
                Ok(b) => {
                    let new_bundle = Arc::new(b);
                    with_head(&mut driver, &cmd, |d, h| {
                        let mesh = d.heads.get_mut(h).expect("head exists (checked)");
                        if let Err(e) =
                            mesh.playhead_mut().booth_hot_reload(Arc::clone(&new_bundle))
                        {
                            emit_err(&format!("hot reload: {e}"));
                        } else {
                            mesh.seed_from_bundle(&new_bundle);
                            emit_tracks(d, h);
                            pump(d, h);
                        }
                    });
                    driver.bundle = new_bundle;
                }
                Err(e) => emit_err(&format!("reload load: {e}")),
            },
            "set_root" => {
                if let Some(p) = cmd.get("path").and_then(|v| v.as_str()) {
                    driver.root = PathBuf::from(p);
                }
            }
            "snapshot" => with_head(&mut driver, &cmd, do_snapshot),
            "restore" => {
                let id = cmd.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                with_head(&mut driver, &cmd, |d, h| do_restore(d, h, &id))
            }
            "fork" => {
                let from = cmd.get("from").and_then(|v| v.as_str()).map(String::from);
                let parent = Driver::head_of(&cmd, &driver.primary);
                do_fork(&mut driver, &parent, from.as_deref());
            }
            "drop" => {
                let target = Driver::head_of(&cmd, "");
                do_drop(&mut driver, &target);
            }
            "heads" => emit_heads(&driver),
            "quit" => break,
            other => emit_err(&format!("unknown cmd: {other}")),
        }
    }
}

/// Resolve the head referenced by a command and dispatch `f` against
/// it. Emits `error` if the head is unknown, never panics.
fn with_head<R, F>(driver: R, cmd: &JsonValue, f: F)
where
    R: HeadCtx,
    F: FnOnce(R::Target, &str),
{
    let head = Driver::head_of(cmd, driver.primary_id());
    if !driver.has_head(&head) {
        emit_err(&format!("unknown head: {head}"));
        return;
    }
    let (ctx, _) = driver.into_ctx();
    f(ctx, &head);
}

/// Tiny trait so `with_head` accepts either `&Driver` (for read-only
/// emit helpers) or `&mut Driver` (for mutating commands) without
/// duplicating the head-lookup boilerplate.
trait HeadCtx {
    type Target;
    fn primary_id(&self) -> &str;
    fn has_head(&self, id: &str) -> bool;
    fn into_ctx(self) -> (Self::Target, ());
}

impl<'a> HeadCtx for &'a Driver {
    type Target = &'a Driver;
    fn primary_id(&self) -> &str { &self.primary }
    fn has_head(&self, id: &str) -> bool { self.heads.contains_key(id) }
    fn into_ctx(self) -> (Self::Target, ()) { (self, ()) }
}

impl<'a> HeadCtx for &'a mut Driver {
    type Target = &'a mut Driver;
    fn primary_id(&self) -> &str { &self.primary }
    fn has_head(&self, id: &str) -> bool { self.heads.contains_key(id) }
    fn into_ctx(self) -> (Self::Target, ()) { (self, ()) }
}

fn do_snapshot(driver: &mut Driver, head: &str) {
    let mesh = driver.heads.get(head).expect("head exists (checked)");
    let at = mesh.ledger().len();
    let snap = mesh.snapshot();
    let id = driver.fresh_snapshot_id();
    driver.snapshots.insert(id.clone(), (head.to_string(), snap, at));
    emit(json!({
        "type": "snapshot",
        "head": head,
        "id": id,
        "at": at,
    }));
}

fn do_restore(driver: &mut Driver, head: &str, id: &str) {
    let snap = match driver.snapshots.get(id) {
        Some((_origin, snap, _at)) => snap.clone(),
        None => {
            emit_err(&format!("unknown snapshot: {id}"));
            return;
        }
    };
    let mesh = driver.heads.get_mut(head).expect("head exists (checked)");
    mesh.restore(&snap);
    emit(json!({
        "type": "restored",
        "head": head,
        "id": id,
    }));
    emit_tracks(driver, head);
}

fn do_fork(driver: &mut Driver, parent: &str, from: Option<&str>) {
    let mesh = match from {
        Some(snap_id) => match driver.snapshots.get(snap_id) {
            Some((_origin, snap, _at)) => {
                let mut fresh = Mesh::new(Arc::clone(&driver.bundle))
                    .expect("bundle valid (already loaded)");
                fresh.restore(snap);
                fresh
            }
            None => {
                emit_err(&format!("unknown snapshot: {snap_id}"));
                return;
            }
        },
        None => match driver.heads.get(parent) {
            Some(m) => m.fork(),
            None => {
                emit_err(&format!("unknown head: {parent}"));
                return;
            }
        },
    };
    let new_id = driver.fresh_head_id();
    driver.heads.insert(new_id.clone(), mesh);
    emit(json!({
        "type": "forked",
        "head": new_id,
        "from": from,
        "parent": parent,
    }));
    emit_tracks(driver, &new_id);
}

fn do_drop(driver: &mut Driver, head: &str) {
    if head == driver.primary {
        emit_err("cannot drop the primary head");
        return;
    }
    if driver.heads.remove(head).is_some() {
        emit(json!({"type": "dropped", "head": head}));
    } else {
        emit_err(&format!("unknown head: {head}"));
    }
}

fn emit_heads(driver: &Driver) {
    let mut heads: Vec<JsonValue> = driver
        .heads
        .iter()
        .map(|(id, mesh)| {
            json!({
                "id": id,
                "primary": *id == driver.primary,
                "halted": mesh.playhead().halted(),
                "ledger_len": mesh.ledger().len(),
            })
        })
        .collect();
    heads.sort_by(|a, b| {
        a.get("id").and_then(|v| v.as_str())
            .cmp(&b.get("id").and_then(|v| v.as_str()))
    });
    emit(json!({"type": "heads", "heads": heads}));
}

fn pump(driver: &mut Driver, head: &str) {
    // Drain Step::Event lines until we hit Choice / Awaiting / Ended
    // or exceed a safety budget for runaway loops. Each event is
    // tagged with its head + track id + ledger index + cause so the
    // simulator can build the multitrack canvas without a second pass.
    let mut budget = 10_000usize;
    loop {
        let mesh = match driver.heads.get_mut(head) {
            Some(m) => m,
            None => {
                emit_err(&format!("unknown head: {head}"));
                return;
            }
        };
        if mesh.playhead().halted() {
            emit(json!({"type": "ended", "head": head}));
            return;
        }
        if budget == 0 {
            emit_err("step budget exhausted (10000)");
            return;
        }
        budget -= 1;
        let start = mesh.ledger().len();
        match mesh.step() {
            Ok((_, Step::Event(_))) => {
                emit_new_envelopes(mesh, head, start);
            }
            Ok((_, Step::Choice(options))) => {
                emit_new_envelopes(mesh, head, start);
                emit(json!({
                    "type": "choice",
                    "head": head,
                    "options": options,
                }));
                return;
            }
            Ok((_, Step::Awaiting { coroutine })) => {
                emit_new_envelopes(mesh, head, start);
                emit(json!({
                    "type": "awaiting",
                    "head": head,
                    "coroutine": coroutine,
                }));
                return;
            }
            Ok((_, Step::Ended)) => {
                emit_new_envelopes(mesh, head, start);
                emit(json!({"type": "ended", "head": head}));
                return;
            }
            Err(e) => {
                emit_err(&format!("step: {e}"));
                return;
            }
        }
    }
}

fn emit_new_envelopes(mesh: &Mesh, head: &str, start: usize) {
    let events = mesh.ledger().events();
    let meta = mesh.ledger().meta();
    for idx in start..events.len() {
        emit(json!({
            "type": "event",
            "head": head,
            "idx": idx,
            "track": meta[idx].track.0,
            "cause": meta[idx].cause,
            "event": events[idx],
        }));
    }
}

fn emit_world(driver: &Driver, head: &str) {
    let mesh = driver.heads.get(head).expect("head exists (checked)");
    let entries: Vec<(String, String)> = mesh
        .world()
        .entries()
        .map(|(k, v)| (k.clone(), v.display()))
        .collect();
    emit(json!({
        "type": "world",
        "head": head,
        "entries": entries,
    }));
}

/// Emit the track list. Each track has an id, a human-readable label,
/// a kind tag the simulator uses for colour coding, and the driver.
fn emit_tracks(driver: &Driver, head: &str) {
    let mesh = driver.heads.get(head).expect("head exists (checked)");
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
        "head": head,
        "tracks": tracks,
    }));
}

/// Emit `cells_for_track` for one track. The simulator pulls cells
/// lazily as it scrolls the multitrack canvas.
fn emit_cells(driver: &Driver, head: &str, track: TrackId) {
    let mesh = driver.heads.get(head).expect("head exists (checked)");
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
        "head": head,
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
