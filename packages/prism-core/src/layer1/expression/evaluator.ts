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

function toStringVal(v: ExprValue): string {
  return typeof v === "string" ? v : String(v);
}

type BuiltinFn = (...args: number[]) => number;
type ExtendedBuiltinFn = (...args: ExprValue[]) => ExprValue;

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

// ── Extended builtins — string, date, aggregate ─────────────────────────────

function parseIsoDate(v: ExprValue): Date | null {
  const s = toStringVal(v);
  if (!s) return null;
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? null : d;
}

function toIsoDate(d: Date): string {
  return d.toISOString().slice(0, 10);
}

const EXTENDED_BUILTINS: Record<string, ExtendedBuiltinFn> = {
  // String
  len: (s) => toStringVal(s ?? "").length,
  lower: (s) => toStringVal(s ?? "").toLowerCase(),
  upper: (s) => toStringVal(s ?? "").toUpperCase(),
  trim: (s) => toStringVal(s ?? "").trim(),
  concat: (...args) => args.map((a) => toStringVal(a ?? "")).join(""),
  left: (s, n) => toStringVal(s ?? "").slice(0, Math.max(0, Math.floor(toNumber(n ?? 0)))),
  right: (s, n) => {
    const str = toStringVal(s ?? "");
    const count = Math.max(0, Math.floor(toNumber(n ?? 0)));
    return count === 0 ? "" : str.slice(-count);
  },
  mid: (s, start, len) => {
    const str = toStringVal(s ?? "");
    const startIdx = Math.max(0, Math.floor(toNumber(start ?? 0)));
    const length = Math.max(0, Math.floor(toNumber(len ?? 0)));
    return str.slice(startIdx, startIdx + length);
  },
  substitute: (s, oldStr, newStr) => {
    const str = toStringVal(s ?? "");
    const find = toStringVal(oldStr ?? "");
    const replace = toStringVal(newStr ?? "");
    if (find.length === 0) return str;
    return str.split(find).join(replace);
  },

  // Date
  today: () => toIsoDate(new Date()),
  now: () => new Date().toISOString(),
  year: (iso) => {
    const d = parseIsoDate(iso ?? "");
    return d ? d.getUTCFullYear() : 0;
  },
  month: (iso) => {
    const d = parseIsoDate(iso ?? "");
    return d ? d.getUTCMonth() + 1 : 0;
  },
  day: (iso) => {
    const d = parseIsoDate(iso ?? "");
    return d ? d.getUTCDate() : 0;
  },
  datediff: (a, b, unit) => {
    const da = parseIsoDate(a ?? "");
    const db = parseIsoDate(b ?? "");
    if (!da || !db) return 0;
    const diffMs = db.getTime() - da.getTime();
    const unitStr = toStringVal(unit ?? "days").toLowerCase();
    if (unitStr === "months") {
      return (db.getUTCFullYear() - da.getUTCFullYear()) * 12 + (db.getUTCMonth() - da.getUTCMonth());
    }
    if (unitStr === "years") {
      return db.getUTCFullYear() - da.getUTCFullYear();
    }
    // default: days
    return Math.floor(diffMs / (1000 * 60 * 60 * 24));
  },

  // Aggregate (variadic over literal arg lists; rollup integration lives in field-resolver.ts)
  sum: (...args) => args.reduce<number>((acc, v) => acc + toNumber(v), 0),
  avg: (...args) => (args.length === 0 ? 0 : args.reduce<number>((acc, v) => acc + toNumber(v), 0) / args.length),
  count: (...args) => args.length,
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
      const name = node.name.toLowerCase();
      const extFn = EXTENDED_BUILTINS[name];
      if (extFn) {
        const args = node.args.map((a) => evalNode(a, store));
        return extFn(...args);
      }
      const fn = BUILTINS[name];
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
      return lv == rv;
    case "!=":
      return lv != rv;
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

const BUILTIN_NAMES = new Set([...Object.keys(BUILTINS), ...Object.keys(EXTENDED_BUILTINS)]);
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
