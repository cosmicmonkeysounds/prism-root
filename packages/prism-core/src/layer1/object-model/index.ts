// ── Types ──────────────────────────────────────────────────────────────────────
export type {
  ObjectId,
  EdgeId,
  EntityFieldType,
  EntityFieldDef,
  GraphObject,
  ObjectEdge,
  ResolvedEdge,
  EdgeBehavior,
  EdgeTypeDef,
  EntityDef,
  CategoryRule,
  TabDefinition,
} from "./types.js";
export { objectId, edgeId } from "./types.js";

// ── Registry ───────────────────────────────────────────────────────────────────
export { ObjectRegistry } from "./registry.js";
export type {
  SlotDef,
  SlotRegistration,
  TreeNode,
  WeakRefChildNode,
} from "./registry.js";

// ── Tree Model ─────────────────────────────────────────────────────────────────
export { TreeModel, TreeModelError } from "./tree-model.js";
export type {
  TreeModelEvent,
  TreeModelEventListener,
  TreeModelHooks,
  TreeModelErrorCode,
  TreeModelOptions,
  AddOptions,
  DuplicateOptions,
} from "./tree-model.js";

// ── Edge Model ─────────────────────────────────────────────────────────────────
export { EdgeModel } from "./edge-model.js";
export type {
  EdgeModelEvent,
  EdgeModelEventListener,
  EdgeModelHooks,
  EdgeModelOptions,
} from "./edge-model.js";

// ── Weak References ────────────────────────────────────────────────────────────
export { WeakRefEngine } from "./weak-ref.js";
export type {
  WeakRefExtraction,
  WeakRefProvider,
  WeakRefChild,
  WeakRefEngineEvent,
  WeakRefEngineEventListener,
  WeakRefEngineOptions,
} from "./weak-ref.js";

// ── NSID & Addressing ──────────────────────────────────────────────────────────
export { NSIDRegistry } from "./nsid.js";
export type { NSID, PrismAddress } from "./nsid.js";
export {
  isValidNSID,
  parseNSID,
  nsid,
  nsidAuthority,
  nsidName,
  isValidPrismAddress,
  prismAddress,
  parsePrismAddress,
} from "./nsid.js";

// ── Query ──────────────────────────────────────────────────────────────────────
export type { ObjectQuery } from "./query.js";
export {
  queryToParams,
  paramsToQuery,
  matchesQuery,
  sortObjects,
} from "./query.js";
