export type ExprType = "number" | "boolean" | "string" | "unknown";
export type ExprValue = number | boolean | string;

export interface ExprError {
  message: string;
  offset?: number;
}

export interface LiteralNode {
  kind: "literal";
  value: ExprValue;
  exprType: ExprType;
}

export interface OperandNode {
  kind: "operand";
  operandType: string;
  id: string;
  subfield?: string;
}

export type BinaryOp =
  | "+"
  | "-"
  | "*"
  | "/"
  | "^"
  | "%"
  | "=="
  | "!="
  | "<"
  | "<="
  | ">"
  | ">="
  | "and"
  | "or";

export interface BinaryNode {
  kind: "binary";
  op: BinaryOp;
  left: AnyExprNode;
  right: AnyExprNode;
}

export interface UnaryNode {
  kind: "unary";
  op: "-" | "not";
  operand: AnyExprNode;
}

export interface CallNode {
  kind: "call";
  name: string;
  args: AnyExprNode[];
}

export type AnyExprNode = LiteralNode | OperandNode | BinaryNode | UnaryNode | CallNode;

export interface ParseResult {
  node: AnyExprNode | null;
  errors: ExprError[];
}

export interface ValueStore {
  resolve(operandType: string, id: string, subfield: string | undefined): ExprValue;
}
