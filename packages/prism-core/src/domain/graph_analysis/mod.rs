//! `domain::graph_analysis` — pure graph analysis over `GraphObject`
//! arrays: topological sort, cycle detection, blocking chains, impact
//! analysis, and Critical Path Method (CPM) scheduling.
//!
//! Port of `packages/prism-core/src/domain/graph-analysis/*` at commit
//! 8426588. Splits the original two-file TS module (`dependency-graph.ts`,
//! `planning-engine.ts`) into the two modules below. Layer 1 only — no
//! I/O, no async, no globals. Operates on `foundation::object_model`
//! primitives.

pub mod dependency_graph;
pub mod planning_engine;

pub use dependency_graph::{
    build_dependency_graph, build_predecessor_graph, compute_slip_impact, detect_cycles,
    find_blocking_chain, find_impacted_objects, topological_sort, DependencyGraph, SlipImpact,
};
pub use planning_engine::{compute_plan, PlanNode, PlanResult};
