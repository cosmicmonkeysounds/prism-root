//! Dependency graph utilities — topological sort, cycle detection,
//! blocking chains, and slip-impact analysis.
//!
//! Port of `packages/prism-core/src/domain/graph-analysis/dependency-graph.ts`
//! at commit 8426588. Operates on `GraphObject` arrays where
//! `data.dependsOn` (or `data.blockedBy`) declares predecessor IDs.
//! The graph is directed: `A → B` means "A blocks B" (B cannot start
//! until A is done). All functions are pure and side-effect free.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use serde_json::Value;

use crate::foundation::object_model::GraphObject;

// ── Types ──────────────────────────────────────────────────────────

/// Directed adjacency list: key → ordered set of successor IDs it unblocks.
///
/// Uses `BTreeSet` so successor order is deterministic (alphabetical),
/// matching the TS `Set` insertion-order semantics for the small, ID-
/// keyed graphs this module handles.
pub type DependencyGraph = BTreeMap<String, BTreeSet<String>>;

/// Result row from [`compute_slip_impact`] — how many days a downstream
/// object slips if its upstream root slips, plus its BFS depth from the
/// root.
#[derive(Debug, Clone, PartialEq)]
pub struct SlipImpact {
    pub object_id: String,
    pub object_name: String,
    pub slip_days: i64,
    pub depth: usize,
}

// ── Helpers ────────────────────────────────────────────────────────

/// Pull `dependsOn` + `blockedBy` string arrays from an object's `data`
/// payload and return the de-duplicated union in first-seen order.
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

// ── Graph construction ─────────────────────────────────────────────

/// Build a "blocks" graph: `A → B` means A must finish before B can start.
pub fn build_dependency_graph(objects: &[GraphObject]) -> DependencyGraph {
    let mut graph: DependencyGraph = BTreeMap::new();

    for obj in objects {
        graph.entry(obj.id.0.clone()).or_default();
    }

    for obj in objects {
        for pred_id in get_predecessors(obj) {
            graph.entry(pred_id).or_default().insert(obj.id.0.clone());
        }
    }

    graph
}

/// Build the inverse graph: `B → {A}` means B is blocked by A.
pub fn build_predecessor_graph(objects: &[GraphObject]) -> DependencyGraph {
    let mut graph: DependencyGraph = BTreeMap::new();

    for obj in objects {
        let entry = graph.entry(obj.id.0.clone()).or_default();
        for pred_id in get_predecessors(obj) {
            entry.insert(pred_id);
        }
    }

    graph
}

// ── Topological sort (Kahn's algorithm) ────────────────────────────

/// Return object IDs in topological order (predecessors before
/// successors). Cyclic nodes are appended at the end.
pub fn topological_sort(objects: &[GraphObject]) -> Vec<String> {
    let graph = build_dependency_graph(objects);
    let mut in_degree: BTreeMap<String, usize> = BTreeMap::new();

    // Preserve the original object order for deterministic output.
    let mut order: Vec<String> = Vec::with_capacity(objects.len());
    for obj in objects {
        let deg = get_predecessors(obj).len();
        in_degree.insert(obj.id.0.clone(), deg);
        order.push(obj.id.0.clone());
    }

    let mut queue: VecDeque<String> = order
        .iter()
        .filter(|id| in_degree.get(*id).copied().unwrap_or(0) == 0)
        .cloned()
        .collect();
    let mut result: Vec<String> = Vec::new();

    while let Some(id) = queue.pop_front() {
        result.push(id.clone());
        if let Some(successors) = graph.get(&id) {
            for succ in successors {
                if let Some(deg) = in_degree.get_mut(succ) {
                    if *deg > 0 {
                        *deg -= 1;
                    }
                    if *deg == 0 {
                        queue.push_back(succ.clone());
                    }
                }
            }
        }
    }

    let result_set: HashSet<String> = result.iter().cloned().collect();
    for id in &order {
        if !result_set.contains(id) {
            result.push(id.clone());
        }
    }

    result
}

// ── Cycle detection ────────────────────────────────────────────────

/// Return dependency cycles as arrays of IDs (empty = no cycles). Each
/// returned cycle starts and ends with the same node ID.
pub fn detect_cycles(objects: &[GraphObject]) -> Vec<Vec<String>> {
    let graph = build_dependency_graph(objects);
    let mut visited: HashSet<String> = HashSet::new();
    let mut in_stack: HashSet<String> = HashSet::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    for obj in objects {
        if !visited.contains(&obj.id.0) {
            let mut path: Vec<String> = Vec::new();
            dfs(
                &obj.id.0,
                &graph,
                &mut visited,
                &mut in_stack,
                &mut path,
                &mut cycles,
            );
        }
    }

    cycles
}

fn dfs(
    id: &str,
    graph: &DependencyGraph,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
    path: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
) {
    if in_stack.contains(id) {
        if let Some(start) = path.iter().position(|p| p == id) {
            let mut cycle: Vec<String> = path[start..].to_vec();
            cycle.push(id.to_string());
            cycles.push(cycle);
        }
        return;
    }
    if visited.contains(id) {
        return;
    }

    visited.insert(id.to_string());
    in_stack.insert(id.to_string());
    path.push(id.to_string());

    if let Some(successors) = graph.get(id) {
        // Clone so we can recurse while the graph borrow drops.
        let succs: Vec<String> = successors.iter().cloned().collect();
        for succ in succs {
            dfs(&succ, graph, visited, in_stack, path, cycles);
        }
    }

    path.pop();
    in_stack.remove(id);
}

// ── Blocking chain ─────────────────────────────────────────────────

/// Return all object IDs that are *transitively blocking* `object_id`
/// (upstream). Result is in BFS order (closest blockers first).
pub fn find_blocking_chain(object_id: &str, objects: &[GraphObject]) -> Vec<String> {
    let pred = build_predecessor_graph(objects);
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(object_id.to_string());
    let mut result: Vec<String> = Vec::new();

    while let Some(current) = queue.pop_front() {
        if let Some(predecessors) = pred.get(&current) {
            for p in predecessors {
                if !visited.contains(p) && p != object_id {
                    visited.insert(p.clone());
                    result.push(p.clone());
                    queue.push_back(p.clone());
                }
            }
        }
    }

    result
}

// ── Impact analysis ────────────────────────────────────────────────

/// Return all object IDs downstream of `object_id` (BFS order).
pub fn find_impacted_objects(object_id: &str, objects: &[GraphObject]) -> Vec<String> {
    let graph = build_dependency_graph(objects);
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(object_id.to_string());
    let mut result: Vec<String> = Vec::new();

    while let Some(current) = queue.pop_front() {
        if let Some(successors) = graph.get(&current) {
            for succ in successors {
                if !visited.contains(succ) {
                    visited.insert(succ.clone());
                    result.push(succ.clone());
                    queue.push_back(succ.clone());
                }
            }
        }
    }

    result
}

/// Compute how many days each downstream object would slip if
/// `object_id` slips by `slip_days`. BFS wave propagation, conservative
/// (no float absorption). Returned rows are sorted by depth, then name.
pub fn compute_slip_impact(
    object_id: &str,
    slip_days: i64,
    objects: &[GraphObject],
) -> Vec<SlipImpact> {
    let graph = build_dependency_graph(objects);
    let obj_map: BTreeMap<String, &GraphObject> =
        objects.iter().map(|o| (o.id.0.clone(), o)).collect();

    let mut slip_map: BTreeMap<String, i64> = BTreeMap::new();
    slip_map.insert(object_id.to_string(), slip_days);
    let mut depth_map: BTreeMap<String, usize> = BTreeMap::new();
    depth_map.insert(object_id.to_string(), 0);
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(object_id.to_string());
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(object_id.to_string());

    while let Some(current) = queue.pop_front() {
        let current_slip = slip_map.get(&current).copied().unwrap_or(0);
        let current_depth = depth_map.get(&current).copied().unwrap_or(0);

        if let Some(successors) = graph.get(&current) {
            for succ in successors {
                let existing = slip_map.get(succ).copied().unwrap_or(0);
                slip_map.insert(succ.clone(), existing.max(current_slip));

                let new_depth = current_depth + 1;
                let existing_depth = depth_map.get(succ).copied().unwrap_or(usize::MAX);
                depth_map.insert(succ.clone(), existing_depth.min(new_depth));

                if !visited.contains(succ) {
                    visited.insert(succ.clone());
                    queue.push_back(succ.clone());
                }
            }
        }
    }

    let mut rows: Vec<SlipImpact> = slip_map
        .into_iter()
        .filter(|(id, _)| id != object_id)
        .map(|(id, slip)| {
            let name = obj_map
                .get(&id)
                .map(|o| o.name.clone())
                .unwrap_or_else(|| id.clone());
            let depth = depth_map.get(&id).copied().unwrap_or(0);
            SlipImpact {
                object_id: id,
                object_name: name,
                slip_days: slip,
                depth,
            }
        })
        .collect();

    rows.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.object_name.cmp(&b.object_name))
    });

    rows
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

    // A → B → C (linear chain)
    fn linear() -> Vec<GraphObject> {
        vec![
            make_obj("A", json!({})),
            make_obj("B", json!({ "dependsOn": ["A"] })),
            make_obj("C", json!({ "dependsOn": ["B"] })),
        ]
    }

    // Diamond: A → B, A → C, B → D, C → D
    fn diamond() -> Vec<GraphObject> {
        vec![
            make_obj("A", json!({})),
            make_obj("B", json!({ "dependsOn": ["A"] })),
            make_obj("C", json!({ "dependsOn": ["A"] })),
            make_obj("D", json!({ "dependsOn": ["B", "C"] })),
        ]
    }

    // Cycle: A → B → C → A
    fn cyclic() -> Vec<GraphObject> {
        vec![
            make_obj("A", json!({ "dependsOn": ["C"] })),
            make_obj("B", json!({ "dependsOn": ["A"] })),
            make_obj("C", json!({ "dependsOn": ["B"] })),
        ]
    }

    // ── buildDependencyGraph ───────────────────────────────────────

    #[test]
    fn build_dependency_graph_forward() {
        let graph = build_dependency_graph(&linear());
        assert!(graph.get("A").unwrap().contains("B"));
        assert!(graph.get("B").unwrap().contains("C"));
        assert_eq!(graph.get("C").unwrap().len(), 0);
    }

    #[test]
    fn build_dependency_graph_no_deps() {
        let objs = vec![make_obj("X", json!({}))];
        let graph = build_dependency_graph(&objs);
        assert_eq!(graph.get("X").unwrap().len(), 0);
    }

    // ── buildPredecessorGraph ──────────────────────────────────────

    #[test]
    fn build_predecessor_graph_inverse() {
        let graph = build_predecessor_graph(&linear());
        assert_eq!(graph.get("A").unwrap().len(), 0);
        assert!(graph.get("B").unwrap().contains("A"));
        assert!(graph.get("C").unwrap().contains("B"));
    }

    // ── topologicalSort ────────────────────────────────────────────

    #[test]
    fn topological_sort_linear() {
        let order = topological_sort(&linear());
        let a = order.iter().position(|x| x == "A").unwrap();
        let b = order.iter().position(|x| x == "B").unwrap();
        let c = order.iter().position(|x| x == "C").unwrap();
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn topological_sort_diamond() {
        let order = topological_sort(&diamond());
        let a = order.iter().position(|x| x == "A").unwrap();
        let b = order.iter().position(|x| x == "B").unwrap();
        let c = order.iter().position(|x| x == "C").unwrap();
        let d = order.iter().position(|x| x == "D").unwrap();
        assert!(a < b);
        assert!(a < c);
        assert!(b < d);
        assert!(c < d);
    }

    #[test]
    fn topological_sort_empty() {
        let order = topological_sort(&[]);
        assert!(order.is_empty());
    }

    #[test]
    fn topological_sort_appends_cyclic() {
        let order = topological_sort(&cyclic());
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn topological_sort_blocked_by() {
        let objs = vec![
            make_obj("X", json!({})),
            make_obj("Y", json!({ "blockedBy": ["X"] })),
        ];
        let order = topological_sort(&objs);
        let x = order.iter().position(|s| s == "X").unwrap();
        let y = order.iter().position(|s| s == "Y").unwrap();
        assert!(x < y);
    }

    // ── detectCycles ───────────────────────────────────────────────

    #[test]
    fn detect_cycles_acyclic() {
        assert!(detect_cycles(&linear()).is_empty());
        assert!(detect_cycles(&diamond()).is_empty());
    }

    #[test]
    fn detect_cycles_found() {
        let cycles = detect_cycles(&cyclic());
        assert!(!cycles.is_empty());
        let cycle = &cycles[0];
        assert_eq!(cycle.first(), cycle.last());
    }

    // ── findBlockingChain ──────────────────────────────────────────

    #[test]
    fn blocking_chain_linear() {
        let chain = find_blocking_chain("C", &linear());
        assert_eq!(chain, vec!["B".to_string(), "A".to_string()]);
    }

    #[test]
    fn blocking_chain_root() {
        assert!(find_blocking_chain("A", &linear()).is_empty());
    }

    #[test]
    fn blocking_chain_diamond() {
        let chain = find_blocking_chain("D", &diamond());
        assert!(chain.contains(&"B".to_string()));
        assert!(chain.contains(&"C".to_string()));
        assert!(chain.contains(&"A".to_string()));
    }

    // ── findImpactedObjects ────────────────────────────────────────

    #[test]
    fn impacted_linear() {
        let impacted = find_impacted_objects("A", &linear());
        assert_eq!(impacted, vec!["B".to_string(), "C".to_string()]);
    }

    #[test]
    fn impacted_leaf() {
        assert!(find_impacted_objects("C", &linear()).is_empty());
    }

    #[test]
    fn impacted_diamond() {
        let impacted = find_impacted_objects("A", &diamond());
        assert!(impacted.contains(&"B".to_string()));
        assert!(impacted.contains(&"C".to_string()));
        assert!(impacted.contains(&"D".to_string()));
    }

    // ── computeSlipImpact ──────────────────────────────────────────

    #[test]
    fn slip_impact_linear() {
        let impacts = compute_slip_impact("A", 3, &linear());
        assert_eq!(impacts.len(), 2);
        assert_eq!(impacts[0].object_id, "B");
        assert_eq!(impacts[0].slip_days, 3);
        assert_eq!(impacts[0].depth, 1);
        assert_eq!(impacts[1].object_id, "C");
        assert_eq!(impacts[1].slip_days, 3);
        assert_eq!(impacts[1].depth, 2);
    }

    #[test]
    fn slip_impact_diamond_max() {
        let impacts = compute_slip_impact("A", 5, &diamond());
        let d = impacts.iter().find(|i| i.object_id == "D").unwrap();
        assert_eq!(d.slip_days, 5);
    }

    #[test]
    fn slip_impact_leaf() {
        assert!(compute_slip_impact("C", 2, &linear()).is_empty());
    }

    #[test]
    fn slip_impact_sort_by_depth_then_name() {
        let impacts = compute_slip_impact("A", 1, &diamond());
        assert!(impacts[0].depth <= impacts[1].depth);
    }
}
