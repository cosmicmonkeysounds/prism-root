import type { AnyExprNode, ExprValue, ValueStore } from "./expression-types.js";
import { parse } from "./parser.js";

function toNumber(v: ExprValue): number {
  if (typeof v === "number") return v;
  if (typeof v === "boolean") return v ? 1 : 0;
  const n = parseFloat(v);
  return Number.isNaN(n) ? 0 : n;
}

function toBoolean(v: ExprValue): boolean {
  if (typeof v === "boolean") return v;
  if (typeof v === "number") return v !== 0;
  return v.length > 0;
}

type BuiltinFn = (...args: number[]) => number;

const BUILTINS: Record<string, BuiltinFn> = {
  abs: (a) => Math.abs(a),
  ceil: (a) => Math.ceil(a),
  floor: (a) => Math.floor(a),
  round: (a) => Math.round(a),
  sqrt: (a) => Math.sqrt(a),
  pow: (a, b) => Math.pow(a, b ?? 1),
  min: (a, b) => Math.min(a, b),
  max: (a, b) => Math.max(a, b),
  clamp: (v, lo, hi) => Math.min(hi, Math.max(lo, v)),
};

export function evaluate(node: AnyExprNode, store: ValueStore): ExprValue {
  try {
    return evalNode(node, store);
  } catch {
    return false;
  }
}

function evalNode(node: AnyExprNode, store: ValueStore): ExprValue {
  switch (node.kind) {
    case "literal":
      return node.value;

    case "operand":
      return store.resolve(node.operandType, node.id, node.subfield);

    case "unary":
      if (node.op === "-") return -toNumber(evalNode(node.operand, store));
      return !toBoolean(evalNode(node.operand, store));

    case "binary":
      return evalBinary(node.op, node.left, node.right, store);

    case "call": {
      const fn = BUILTINS[node.name.toLowerCase()];
      if (!fn) return 0;
      const args = node.args.map((a) => toNumber(evalNode(a, store)));
      return fn(...args);
    }
  }
}

function evalBinary(
  op: string,
  left: AnyExprNode,
  right: AnyExprNode,
  store: ValueStore,
): ExprValue {
  // Short-circuit
  if (op === "and") {
    const lv = evalNode(left, store);
    return toBoolean(lv) ? evalNode(right, store) : lv;
  }
  if (op === "or") {
    const lv = evalNode(left, store);
    return toBoolean(lv) ? lv : evalNode(right, store);
  }

  const lv = evalNode(left, store);
  const rv = evalNode(right, store);

  switch (op) {
    case "+":
      if (typeof lv === "string" || typeof rv === "string") return String(lv) + String(rv);
      return toNumber(lv) + toNumber(rv);
    case "-":
      return toNumber(lv) - toNumber(rv);
    case "*":
      return toNumber(lv) * toNumber(rv);
    case "/": {
      const d = toNumber(rv);
      return d === 0 ? 0 : toNumber(lv) / d;
    }
    case "^":
      return Math.pow(toNumber(lv), toNumber(rv));
    case "%":
      return toNumber(lv) % toNumber(rv);
    case "==":
      return lv == rv; // eslint-disable-line eqeqeq
    case "!=":
      return lv != rv; // eslint-disable-line eqeqeq
    case "<":
      return toNumber(lv) < toNumber(rv);
    case "<=":
      return toNumber(lv) <= toNumber(rv);
    case ">":
      return toNumber(lv) > toNumber(rv);
    case ">=":
      return toNumber(lv) >= toNumber(rv);
    default:
      return 0;
  }
}

const BUILTIN_NAMES = new Set(Object.keys(BUILTINS));
const KEYWORDS = new Set(["true", "false", "and", "or", "not"]);

function wrapBareIdentifiers(formula: string): string {
  const TOKEN_RE =
    /(\[[^\]]*\])|('[^']*')|("(?:[^"\\]|\\.)*")|(true|false|and|or|not)\b|([a-zA-Z_][a-zA-Z0-9_]*)|(\S|\s+)/gi;
  let result = "";
  let m: RegExpExecArray | null;
  while ((m = TOKEN_RE.exec(formula)) !== null) {
    if (m[5] !== undefined) {
      const name = m[5];
      if (!BUILTIN_NAMES.has(name.toLowerCase()) && !KEYWORDS.has(name.toLowerCase())) {
        result += `[field:${name}]`;
        continue;
      }
    }
    result += m[0];
  }
  return result;
}

function makeContextStore(context: Record<string, ExprValue>): ValueStore {
  return {
    resolve(_operandType: string, id: string, subfield: string | undefined): ExprValue {
      const val = context[id];
      if (val === undefined) return 0;
      if (subfield && typeof val === "object" && val !== null) {
        return (val as Record<string, ExprValue>)[subfield] ?? 0;
      }
      return val;
    },
  };
}

export function evaluateExpression(
  formula: string,
  context: Record<string, ExprValue>,
): { result: ExprValue; errors: string[] } {
  const wrapped = wrapBareIdentifiers(formula);
  const { node, errors: parseErrors } = parse(wrapped);
  if (!node || parseErrors.length > 0) {
    return { result: false, errors: parseErrors.map((e) => e.message) };
  }
  const store = makeContextStore(context);
  const result = evaluate(node, store);
  return { result, errors: [] };
}
