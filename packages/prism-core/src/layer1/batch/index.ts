// @prism/core/batch — public barrel

export { createBatchTransaction } from "./batch-transaction.js";
export type {
  BatchTransaction,
  BatchTransactionOptions,
} from "./batch-transaction.js";

export type {
  BatchOp,
  CreateObjectOp,
  UpdateObjectOp,
  DeleteObjectOp,
  MoveObjectOp,
  CreateEdgeOp,
  UpdateEdgeOp,
  DeleteEdgeOp,
  BatchResult,
  BatchProgress,
  BatchProgressCallback,
  BatchValidationError,
  BatchValidationResult,
  BatchExecuteOptions,
} from "./batch-types.js";
