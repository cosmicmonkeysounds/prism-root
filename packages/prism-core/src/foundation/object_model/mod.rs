//! `object_model` — the unified graph primitive: objects, edges,
//! containment rules, registry, tree and graph indices.
//!
//! Straight port of `foundation/object-model/` from the legacy TS
//! tree. The module is deliberately decomposed along the same file
//! boundaries (types / nsid / case_str / registry / tree_model /
//! edge_model / query / context_engine / weak_ref) so the parity
//! tests in §10 of the migration plan can run fixture-for-fixture.

pub mod case_str;
pub mod context_engine;
pub mod edge_model;
pub mod error;
pub mod nsid;
pub mod query;
pub mod registry;
pub mod tree_model;
pub mod types;
pub mod weak_ref;

pub use case_str::{camel, pascal, singular};
pub use context_engine::{ContextEngine, EvaluationContext};
pub use edge_model::{EdgeModel, EdgeModelEvent, EdgeModelHooks};
pub use error::{ObjectModelError, ObjectModelErrorCode};
pub use nsid::{
    is_valid_nsid, is_valid_prism_address, nsid, nsid_authority, nsid_name, parse_nsid,
    parse_prism_address, prism_address, Nsid, NsidRegistry, PrismAddress,
};
pub use query::{apply_filters, matches_filter, ObjectFilter, ObjectFilterOp};
pub use registry::{ObjectRegistry, SlotDef, SlotRegistration, TreeNode, WeakRefChildNode};
pub use tree_model::{AddOptions, DuplicateOptions, TreeModel, TreeModelEvent, TreeModelHooks};
pub use types::{
    edge_id, object_id, ApiOperation, CategoryRule, EdgeBehavior, EdgeId, EdgeScope, EdgeTypeDef,
    EntityDef, EntityFieldDef, EntityFieldType, EnumOption, GraphObject, ObjectEdge, ObjectId,
    ObjectTypeApiConfig, ResolvedEdge, RollupFunction, TabDefinition, UiHints,
};
pub use weak_ref::{
    WeakRefChild, WeakRefEngine, WeakRefEngineEvent, WeakRefExtraction, WeakRefLocation,
    WeakRefProvider, WeakRefScope,
};
