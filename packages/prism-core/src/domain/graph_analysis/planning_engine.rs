//! Planning Engine — Generic Critical Path Method (CPM).
//!
//! Port of `packages/prism-core/src/domain/graph-analysis/planning-engine.ts`
//! at commit 8426588. Works on any `GraphObject` that declares
//! dependencies via `data.dependsOn` and/or `data.blockedBy`. Useful
//! for tasks, goals, project phases, learning paths — any domain with
//! ordering constraints.
//!
//! Duration priority:
//!   1. `data.durationDays` — explicit integer days
//!   2. `data.estimateMs`   — millisecond estimate → days
//!   3. `date` + `end_date` — span from scheduled dates
//!   4. default 1 day

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use crate::foundation::object_model::GraphObject;

use super::dependency_graph::topological_sort;

// ── Types ──────────────────────────────────────────────────────────

/// One node in a [`PlanResult`]. Mirrors the TS `PlanNode` interface
/// exactly — the early/late window and float determine whether the
/// node is on the critical path.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanNode {
    pub id: String,
    pub name: String,
    pub type_name: String,
    pub duration_days: i64,
    pub early_start: i64,
    pub early_finish: i64,
    pub late_start: i64,
    pub late_finish: i64,
    pub total_float: i64,
    pub is_critical: bool,
    pub predecessors: Vec<String>,
}

/// Output of [`compute_plan`] — total project duration, the computed
/// critical path (as an ordered list of IDs), and the full node map.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanResult {
    pub total_duration_days: i64,
    pub critical_path: Vec<String>,
    pub nodes: BTreeMap<String, PlanNode>,
}

// ── Helpers ────────────────────────────────────────────────────────

const MS_PER_DAY: i64 = 1_000 * 60 * 60 * 24;

fn get_duration(obj: &GraphObject) -> i64 {
    // 1. explicit durationDays
    if let Some(v) = obj.data.get("durationDays") {
        if let Some(d) = v.as_i64() {
            if d > 0 {
                return d;
            }
        }
        if let Some(d) = v.as_f64() {
            if d > 0.0 {
                return d as i64;
            }
        }
    }

    // 2. estimateMs → days (ceil, min 1)
    if let Some(v) = obj.data.get("estimateMs") {
        let ms = v.as_i64().or_else(|| v.as_f64().map(|f| f as i64));
        if let Some(ms) = ms {
            if ms > 0 {
                let days = (ms + MS_PER_DAY - 1) / MS_PER_DAY;
                return days.max(1);
            }
        }
    }

    // 3. date + end_date span
    if let (Some(start), Some(end)) = (obj.date.as_deref(), obj.end_date.as_deref()) {
        if let (Some(start_ms), Some(end_ms)) = (parse_iso_ms(start), parse_iso_ms(end)) {
            let span = end_ms - start_ms;
            if span > 0 {
                let days = (span + MS_PER_DAY - 1) / MS_PER_DAY;
                return days.max(1);
            }
        }
    }

    1
}

/// Parse an ISO-8601 date or date-time string to milliseconds since
/// the Unix epoch. Matches the behavior of TS `new Date(s).getTime()`
/// for the test-covered formats: `YYYY-MM-DD` and full RFC-3339.
fn parse_iso_ms(s: &str) -> Option<i64> {
    // Full RFC-3339 / ISO 8601 with time + offset.
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis());
    }
    // Bare YYYY-MM-DD → interpret at UTC midnight, matching JS Date.
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(ndt) = d.and_hms_opt(0, 0, 0) {
            return Some(ndt.and_utc().timestamp_millis());
        }
    }
    None
}

fn get_predecessors(obj: &GraphObject) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for key in ["dependsOn", "blockedBy"] {
        if let Some(Value::Array(arr)) = obj.data.get(key) {
            for v in arr {
                if let Value::String(s) = v {
                    if seen.insert(s.clone()) {
                        out.push(s.clone());
                    }
                }
            }
        }
    }

    out
}

// ── CPM ────────────────────────────────────────────────────────────

/// Run a Critical Path Method pass over the given objects and return a
/// [`PlanResult`] with early/late windows, total float, and the critical
/// path in topological order.
pub fn compute_plan(objects: &[GraphObject]) -> PlanResult {
    if objects.is_empty() {
        return PlanResult {
            total_duration_days: 0,
            critical_path: Vec::new(),
            nodes: BTreeMap::new(),
        };
    }

    let order = topological_sort(objects);
    let mut nodes: BTreeMap<String, PlanNode> = BTreeMap::new();

    for obj in objects {
        nodes.insert(
            obj.id.0.clone(),
            PlanNode {
                id: obj.id.0.clone(),
                name: obj.name.clone(),
                type_name: obj.type_name.clone(),
                duration_days: get_duration(obj),
                early_start: 0,
                early_finish: 0,
                late_start: 0,
                late_finish: 0,
                total_float: 0,
                is_critical: false,
                predecessors: get_predecessors(obj),
            },
        );
    }

    // Forward pass: max of predecessors' earlyFinish → this.earlyStart.
    for id in &order {
        // Gather predecessor EFs first so we don't clash with the
        // subsequent mutable borrow.
        let preds = nodes
            .get(id)
            .map(|n| n.predecessors.clone())
            .unwrap_or_default();
        let max_pred_ef = preds
            .iter()
            .filter_map(|pid| nodes.get(pid).map(|n| n.early_finish))
            .max()
            .unwrap_or(0);

        if let Some(node) = nodes.get_mut(id) {
            node.early_start = max_pred_ef;
            node.early_finish = max_pred_ef + node.duration_days;
        }
    }

    let total_duration = nodes.values().map(|n| n.early_finish).max().unwrap_or(0);

    // Build successor index.
    let mut succs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for obj in objects {
        succs.insert(obj.id.0.clone(), Vec::new());
    }
    for node in nodes.values() {
        for pred_id in &node.predecessors {
            succs
                .entry(pred_id.clone())
                .or_default()
                .push(node.id.clone());
        }
    }

    // Backward pass: initialize lateFinish = totalDuration.
    for node in nodes.values_mut() {
        node.late_finish = total_duration;
        node.late_start = total_duration - node.duration_days;
    }

    for id in order.iter().rev() {
        let node_succs = succs.get(id).cloned().unwrap_or_default();
        if !node_succs.is_empty() {
            let min_ls = node_succs
                .iter()
                .filter_map(|sid| nodes.get(sid).map(|n| n.late_start))
                .min()
                .unwrap_or(i64::MAX);

            if let Some(node) = nodes.get_mut(id) {
                node.late_finish = min_ls;
                node.late_start = min_ls - node.duration_days;
            }
        }
        if let Some(node) = nodes.get_mut(id) {
            node.total_float = node.late_start - node.early_start;
            node.is_critical = node.total_float <= 0;
        }
    }

    // Extract critical path: start from critical nodes whose
    // predecessors are all non-critical, then walk along critical
    // successors (preferring the one with the latest earlyFinish).
    let crit_set: HashSet<String> = nodes
        .values()
        .filter(|n| n.is_critical)
        .map(|n| n.id.clone())
        .collect();

    let mut starts: Vec<&PlanNode> = nodes
        .values()
        .filter(|n| n.is_critical && n.predecessors.iter().all(|p| !crit_set.contains(p)))
        .collect();
    starts.sort_by_key(|n| n.early_start);

    let mut critical_path: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();

    for start in &starts {
        walk_critical(
            &start.id,
            &nodes,
            &succs,
            &crit_set,
            &mut critical_path,
            &mut visited,
        );
    }

    PlanResult {
        total_duration_days: total_duration,
        critical_path,
        nodes,
    }
}

fn walk_critical(
    id: &str,
    nodes: &BTreeMap<String, PlanNode>,
    succs: &BTreeMap<String, Vec<String>>,
    crit_set: &HashSet<String>,
    critical_path: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    if visited.contains(id) {
        return;
    }
    visited.insert(id.to_string());
    critical_path.push(id.to_string());

    let mut crit_succs: Vec<&String> = succs
        .get(id)
        .map(|v| v.iter().filter(|s| crit_set.contains(*s)).collect())
        .unwrap_or_default();

    // Descending earlyFinish, so `[0]` picks the latest-finishing
    // successor just like the TS sort.
    crit_succs.sort_by(|a, b| {
        let ea = nodes.get(*a).map(|n| n.early_finish).unwrap_or(0);
        let eb = nodes.get(*b).map(|n| n.early_finish).unwrap_or(0);
        eb.cmp(&ea)
    });

    if let Some(next) = crit_succs.first() {
        let next = (*next).clone();
        walk_critical(&next, nodes, succs, crit_set, critical_path, visited);
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_obj(id: &str, data: Value) -> GraphObject {
        let mut obj = GraphObject::new(id, "task", format!("Task {id}"));
        if let Value::Object(map) = data {
            for (k, v) in map {
                obj.data.insert(k, v);
            }
        }
        obj
    }

    fn make_obj_dates(id: &str, date: &str, end_date: &str) -> GraphObject {
        let mut obj = make_obj(id, json!({}));
        obj.date = Some(date.to_string());
        obj.end_date = Some(end_date.to_string());
        obj
    }

    #[test]
    fn empty_input_returns_empty() {
        let result = compute_plan(&[]);
        assert_eq!(result.total_duration_days, 0);
        assert!(result.critical_path.is_empty());
        assert_eq!(result.nodes.len(), 0);
    }

    #[test]
    fn single_task_default_duration() {
        let result = compute_plan(&[make_obj("A", json!({}))]);
        let node = result.nodes.get("A").unwrap();
        assert_eq!(node.duration_days, 1);
        assert_eq!(node.early_start, 0);
        assert_eq!(node.early_finish, 1);
        assert!(node.is_critical);
        assert_eq!(result.total_duration_days, 1);
        assert_eq!(result.critical_path, vec!["A".to_string()]);
    }

    #[test]
    fn explicit_duration_days() {
        let result = compute_plan(&[make_obj("A", json!({ "durationDays": 5 }))]);
        assert_eq!(result.nodes.get("A").unwrap().duration_days, 5);
        assert_eq!(result.total_duration_days, 5);
    }

    #[test]
    fn estimate_ms_to_days() {
        let ms = 3 * 24 * 60 * 60 * 1000_i64; // 3 days
        let result = compute_plan(&[make_obj("A", json!({ "estimateMs": ms }))]);
        assert_eq!(result.nodes.get("A").unwrap().duration_days, 3);
    }

    #[test]
    fn date_span_duration() {
        let result = compute_plan(&[make_obj_dates("A", "2024-01-01", "2024-01-04")]);
        assert_eq!(result.nodes.get("A").unwrap().duration_days, 3);
    }

    #[test]
    fn linear_chain() {
        // A(2d) → B(3d) → C(1d) = 6 days total
        let objects = vec![
            make_obj("A", json!({ "durationDays": 2 })),
            make_obj("B", json!({ "durationDays": 3, "dependsOn": ["A"] })),
            make_obj("C", json!({ "durationDays": 1, "dependsOn": ["B"] })),
        ];
        let result = compute_plan(&objects);

        assert_eq!(result.total_duration_days, 6);

        let a = result.nodes.get("A").unwrap();
        assert_eq!(a.early_start, 0);
        assert_eq!(a.early_finish, 2);

        let b = result.nodes.get("B").unwrap();
        assert_eq!(b.early_start, 2);
        assert_eq!(b.early_finish, 5);

        let c = result.nodes.get("C").unwrap();
        assert_eq!(c.early_start, 5);
        assert_eq!(c.early_finish, 6);

        assert_eq!(
            result.critical_path,
            vec!["A".to_string(), "B".to_string(), "C".to_string()]
        );
    }

    #[test]
    fn diamond_with_float() {
        // A(1d) → B(3d) → D(1d)  = critical: A→B→D = 5d
        // A(1d) → C(1d) → D(1d)  = non-critical: A→C→D = 3d, float=2
        let objects = vec![
            make_obj("A", json!({ "durationDays": 1 })),
            make_obj("B", json!({ "durationDays": 3, "dependsOn": ["A"] })),
            make_obj("C", json!({ "durationDays": 1, "dependsOn": ["A"] })),
            make_obj("D", json!({ "durationDays": 1, "dependsOn": ["B", "C"] })),
        ];
        let result = compute_plan(&objects);

        assert_eq!(result.total_duration_days, 5);

        let c = result.nodes.get("C").unwrap();
        assert_eq!(c.total_float, 2);
        assert!(!c.is_critical);

        let b = result.nodes.get("B").unwrap();
        assert_eq!(b.total_float, 0);
        assert!(b.is_critical);

        assert!(result.critical_path.contains(&"A".to_string()));
        assert!(result.critical_path.contains(&"B".to_string()));
        assert!(result.critical_path.contains(&"D".to_string()));
        assert!(!result.critical_path.contains(&"C".to_string()));
    }

    #[test]
    fn parallel_independent_have_float() {
        let objects = vec![
            make_obj("X", json!({ "durationDays": 5 })),
            make_obj("Y", json!({ "durationDays": 2 })),
        ];
        let result = compute_plan(&objects);

        assert_eq!(result.total_duration_days, 5);

        let x = result.nodes.get("X").unwrap();
        assert!(x.is_critical);

        let y = result.nodes.get("Y").unwrap();
        assert_eq!(y.total_float, 3);
        assert!(!y.is_critical);
    }

    #[test]
    fn supports_blocked_by_field() {
        let objects = vec![
            make_obj("A", json!({ "durationDays": 2 })),
            make_obj("B", json!({ "durationDays": 1, "blockedBy": ["A"] })),
        ];
        let result = compute_plan(&objects);
        assert_eq!(result.nodes.get("B").unwrap().early_start, 2);
    }

    #[test]
    fn records_predecessors() {
        let objects = vec![
            make_obj("A", json!({})),
            make_obj("B", json!({ "dependsOn": ["A"] })),
        ];
        let result = compute_plan(&objects);
        assert_eq!(
            result.nodes.get("B").unwrap().predecessors,
            vec!["A".to_string()]
        );
        assert!(result.nodes.get("A").unwrap().predecessors.is_empty());
    }
}
