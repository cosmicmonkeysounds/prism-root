//! Tiny expression language used by directive arguments and reactive
//! `let` bindings (spec §12.1, §14).
//!
//! Literals, dotted paths, parenthesised sub-expressions, unary `-`/`!`,
//! binary arithmetic / comparison / logic, `name(args)` call form, and
//! `[value for var in source where filter]` list comprehensions. The
//! runtime evaluates everything natively against a `World` scope.

// ---------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------

export type Value =
  | { kind: "null" }
  | { kind: "bool"; value: boolean }
  | { kind: "number"; value: number }
  | { kind: "string"; value: string }
  | { kind: "list"; items: Value[] };

export const VNULL: Value = { kind: "null" };
export function vBool(value: boolean): Value {
  return { kind: "bool", value };
}
export function vNumber(value: number): Value {
  return { kind: "number", value };
}
export function vString(value: string): Value {
  return { kind: "string", value };
}
export function vList(items: Value[]): Value {
  return { kind: "list", items };
}

export function truthy(v: Value): boolean {
  switch (v.kind) {
    case "null":
      return false;
    case "bool":
      return v.value;
    case "number":
      return v.value !== 0 && !Number.isNaN(v.value);
    case "string":
      return v.value.length > 0;
    case "list":
      return v.items.length > 0;
  }
}

export function asNumber(v: Value): number | null {
  switch (v.kind) {
    case "number":
      return v.value;
    case "bool":
      return v.value ? 1 : 0;
    default:
      return null;
  }
}

export function asList(v: Value): Value[] | null {
  return v.kind === "list" ? v.items : null;
}

export function display(v: Value): string {
  switch (v.kind) {
    case "null":
      return "null";
    case "bool":
      return v.value ? "true" : "false";
    case "number": {
      const n = v.value;
      if (Number.isInteger(n) && Number.isFinite(n) && Math.abs(n) < 1e16) {
        return String(n);
      }
      return String(n);
    }
    case "string":
      return v.value;
    case "list":
      return `[${v.items.map(display).join(", ")}]`;
  }
}

export function valuesEqual(a: Value, b: Value): boolean {
  if (a.kind === "null" && b.kind === "null") return true;
  if (a.kind === "bool" && b.kind === "bool") return a.value === b.value;
  if (a.kind === "number" && b.kind === "number") return a.value === b.value;
  if (a.kind === "string" && b.kind === "string") return a.value === b.value;
  const x = asNumber(a);
  const y = asNumber(b);
  return x !== null && y !== null && x === y;
}

// ---------------------------------------------------------------------
// World scope
// ---------------------------------------------------------------------

/**
 * Read/write scope used by `evaluate` and the `set` builtin. Names are
 * flat dotted paths (`Wren.trust`). `collections` exposes virtual list
 * values for project-wide groups (`Characters`, …) so comprehensions
 * see the live population (spec §12.1).
 */
export class World {
  private values = new Map<string, Value>();
  private collections = new Map<string, Value>();

  get(key: string): Value {
    return this.values.get(key) ?? this.collections.get(key) ?? VNULL;
  }

  set(key: string, value: Value): void {
    this.values.set(key, value);
  }

  /** Remove a key; returns the prior value, if any. */
  unset(key: string): Value | null {
    const prior = this.values.get(key) ?? null;
    this.values.delete(key);
    return prior;
  }

  /** Borrow the current value without the `Null` fallback. */
  peek(key: string): Value | null {
    return this.values.get(key) ?? null;
  }

  /** Entries in sorted-key order, matching the Rust `BTreeMap`. */
  entries(): Array<[string, Value]> {
    return [...this.values.entries()].sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
  }

  setCollection(key: string, value: Value): void {
    this.collections.set(key, value);
  }

  collection(key: string): Value | null {
    return this.collections.get(key) ?? null;
  }
}

// ---------------------------------------------------------------------
// Expression AST
// ---------------------------------------------------------------------

export type UnOp = "neg" | "not";
export type BinOp =
  | "add"
  | "sub"
  | "mul"
  | "div"
  | "mod"
  | "eq"
  | "ne"
  | "lt"
  | "le"
  | "gt"
  | "ge"
  | "and"
  | "or";

export type Expr =
  | { kind: "null" }
  | { kind: "bool"; value: boolean }
  | { kind: "number"; value: number }
  | { kind: "string"; value: string }
  | { kind: "path"; segments: string[] }
  | { kind: "list"; items: Expr[] }
  | { kind: "unary"; op: UnOp; inner: Expr }
  | { kind: "binary"; op: BinOp; left: Expr; right: Expr }
  | { kind: "call"; name: string; args: Expr[] }
  | { kind: "listComp"; value: Expr; var: string; source: Expr; filter: Expr | null };

export class ExprError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ExprError";
  }
}

// ---------------------------------------------------------------------
// Call arguments
// ---------------------------------------------------------------------

/** One argument passed to a `callFn` — both the parsed AST and value. */
export class CallArg {
  constructor(
    public expr: Expr,
    public value: Value,
  ) {}

  /** Best-effort symbolic name for a path argument, else null. */
  symbol(): string | null {
    return this.expr.kind === "path" ? this.expr.segments.join(".") : null;
  }

  /** Coerce to a string: literal string, else symbolic name, else display. */
  asName(): string {
    if (this.value.kind === "string") return this.value.value;
    if (this.value.kind === "null") return this.symbol() ?? "";
    return this.symbol() ?? display(this.value);
  }
}

export type CallFn = (name: string, args: CallArg[]) => Value;

/**
 * Hook-scope bindings: a name → entity-id map (`guest` → `"g42"`,
 * `self` → `"Moderator_Prime"`). Unlike comprehension locals, these
 * substitute into *any* path segment, so `Moderator_Prime.trusts.guest`
 * resolves to `Moderator_Prime.trusts.g42`. Used by the sim runtime.
 */
export type Bindings = Map<string, string>;

// ---------------------------------------------------------------------
// Parse + evaluate
// ---------------------------------------------------------------------

/** Parse `source` into one `Expr`. Trailing garbage is an error. */
export function parseExpr(source: string): Expr {
  const tokens = tokenize(source);
  const p = new ExprParser(tokens);
  const expr = p.parseExpr(0);
  if (p.pos !== p.tokens.length) {
    const tok = p.tokens[p.pos]!;
    throw new ExprError(`unexpected token \`${tokKindLabel(tok.kind)}\` at ${tok.start}`);
  }
  return expr;
}

/** Evaluate `expr` against `world`, delegating calls to `callFn`. */
export function evaluate(expr: Expr, world: World, callFn: CallFn, bindings?: Bindings): Value {
  return evalScoped(expr, world, [], callFn, bindings);
}

type Local = [string, Value];

function evalScoped(
  expr: Expr,
  world: World,
  locals: Local[],
  callFn: CallFn,
  bindings: Bindings | undefined,
): Value {
  switch (expr.kind) {
    case "null":
      return VNULL;
    case "bool":
      return vBool(expr.value);
    case "number":
      return vNumber(expr.value);
    case "string":
      return vString(expr.value);
    case "path":
      return resolvePath(expr.segments, world, locals, bindings);
    case "list": {
      const out: Value[] = [];
      for (const it of expr.items) out.push(evalScoped(it, world, locals, callFn, bindings));
      return vList(out);
    }
    case "unary": {
      const v = evalScoped(expr.inner, world, locals, callFn, bindings);
      if (expr.op === "neg") return vNumber(-(asNumber(v) ?? 0));
      return vBool(!truthy(v));
    }
    case "binary": {
      const op = expr.op;
      if (op === "and" || op === "or") {
        const lv = evalScoped(expr.left, world, locals, callFn, bindings);
        const lt = truthy(lv);
        if (op === "and" && !lt) return vBool(false);
        if (op === "or" && lt) return vBool(true);
        return vBool(truthy(evalScoped(expr.right, world, locals, callFn, bindings)));
      }
      const lv = evalScoped(expr.left, world, locals, callFn, bindings);
      const rv = evalScoped(expr.right, world, locals, callFn, bindings);
      return evalBinary(op, lv, rv);
    }
    case "call": {
      const packed: CallArg[] = [];
      for (const a of expr.args) {
        packed.push(new CallArg(a, evalScoped(a, world, locals, callFn, bindings)));
      }
      return callFn(expr.name, packed);
    }
    case "listComp": {
      const srcValue = evalScoped(expr.source, world, locals, callFn, bindings);
      const items = srcValue.kind === "list" ? srcValue.items : [];
      const out: Value[] = [];
      for (const item of items) {
        const combined: Local[] = [...locals, [expr.var, item]];
        if (expr.filter !== null) {
          if (!truthy(evalScoped(expr.filter, world, combined, callFn, bindings))) continue;
        }
        out.push(evalScoped(expr.value, world, combined, callFn, bindings));
      }
      return vList(out);
    }
  }
}

function resolvePath(
  segments: string[],
  world: World,
  locals: Local[],
  bindings: Bindings | undefined,
): Value {
  // Hook-scope bindings substitute into every segment first.
  const segs =
    bindings !== undefined ? segments.map((s) => bindings.get(s) ?? s) : segments;
  const head = segs[0] ?? "";
  for (let i = locals.length - 1; i >= 0; i--) {
    const [name, value] = locals[i]!;
    if (name === head) {
      if (segs.length === 1) return value;
      if (value.kind === "string") {
        const key = [value.value, ...segs.slice(1)].join(".");
        return world.get(key);
      }
      return value;
    }
  }
  return world.get(segs.join("."));
}

/** Expand a dotted path against hook-scope `bindings` (segment-wise). */
export function expandPath(segments: string[], bindings: Bindings): string {
  return segments.map((s) => bindings.get(s) ?? s).join(".");
}

function evalBinary(op: BinOp, l: Value, r: Value): Value {
  const num = (v: Value): number => asNumber(v) ?? 0;
  switch (op) {
    case "add":
      if (l.kind === "string") return vString(l.value + display(r));
      if (r.kind === "string") return vString(display(l) + r.value);
      return vNumber(num(l) + num(r));
    case "sub":
      return vNumber(num(l) - num(r));
    case "mul":
      return vNumber(num(l) * num(r));
    case "div":
      return vNumber(num(l) / num(r));
    case "mod":
      return vNumber(num(l) % num(r));
    case "eq":
      return vBool(valuesEqual(l, r));
    case "ne":
      return vBool(!valuesEqual(l, r));
    case "lt":
      return vBool(num(l) < num(r));
    case "le":
      return vBool(num(l) <= num(r));
    case "gt":
      return vBool(num(l) > num(r));
    case "ge":
      return vBool(num(l) >= num(r));
    case "and":
    case "or":
      throw new ExprError("short-circuited above");
  }
}

// ---------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------

type Tok =
  | { t: "number"; value: number }
  | { t: "string"; value: string }
  | { t: "ident"; value: string }
  | { t: "true" }
  | { t: "false" }
  | { t: "null" }
  | { t: "and" }
  | { t: "or" }
  | { t: "plus" }
  | { t: "minus" }
  | { t: "star" }
  | { t: "slash" }
  | { t: "percent" }
  | { t: "bang" }
  | { t: "eqeq" }
  | { t: "noteq" }
  | { t: "lt" }
  | { t: "le" }
  | { t: "gt" }
  | { t: "ge" }
  | { t: "lparen" }
  | { t: "rparen" }
  | { t: "lbracket" }
  | { t: "rbracket" }
  | { t: "comma" }
  | { t: "dot" }
  | { t: "for" }
  | { t: "in" }
  | { t: "where" };

interface Token {
  kind: Tok;
  start: number;
}

function tokKindLabel(k: Tok): string {
  return k.t;
}

function isAsciiWhitespace(c: string): boolean {
  return c === " " || c === "\t" || c === "\n" || c === "\r" || c === "\f";
}

function tokenize(source: string): Token[] {
  const out: Token[] = [];
  let i = 0;
  while (i < source.length) {
    const c = source[i]!;
    if (isAsciiWhitespace(c)) {
      i += 1;
      continue;
    }
    const start = i;
    if (i + 1 < source.length) {
      const pair = source.slice(i, i + 2);
      let two: Tok | null = null;
      if (pair === "==") two = { t: "eqeq" };
      else if (pair === "!=") two = { t: "noteq" };
      else if (pair === "<=") two = { t: "le" };
      else if (pair === ">=") two = { t: "ge" };
      else if (pair === "&&") two = { t: "and" };
      else if (pair === "||") two = { t: "or" };
      if (two !== null) {
        out.push({ kind: two, start });
        i += 2;
        continue;
      }
    }
    switch (c) {
      case "+":
        out.push({ kind: { t: "plus" }, start });
        i += 1;
        break;
      case "-":
        out.push({ kind: { t: "minus" }, start });
        i += 1;
        break;
      case "*":
        out.push({ kind: { t: "star" }, start });
        i += 1;
        break;
      case "/":
        out.push({ kind: { t: "slash" }, start });
        i += 1;
        break;
      case "%":
        out.push({ kind: { t: "percent" }, start });
        i += 1;
        break;
      case "!":
        out.push({ kind: { t: "bang" }, start });
        i += 1;
        break;
      case "<":
        out.push({ kind: { t: "lt" }, start });
        i += 1;
        break;
      case ">":
        out.push({ kind: { t: "gt" }, start });
        i += 1;
        break;
      case "(":
        out.push({ kind: { t: "lparen" }, start });
        i += 1;
        break;
      case ")":
        out.push({ kind: { t: "rparen" }, start });
        i += 1;
        break;
      case "[":
        out.push({ kind: { t: "lbracket" }, start });
        i += 1;
        break;
      case "]":
        out.push({ kind: { t: "rbracket" }, start });
        i += 1;
        break;
      case ",":
        out.push({ kind: { t: "comma" }, start });
        i += 1;
        break;
      case ".":
        out.push({ kind: { t: "dot" }, start });
        i += 1;
        break;
      case '"':
      case "'": {
        const quote = c;
        i += 1;
        const strStart = i;
        while (i < source.length && source[i] !== quote) i += 1;
        if (i >= source.length) throw new ExprError("unterminated string literal");
        const s = source.slice(strStart, i);
        i += 1;
        out.push({ kind: { t: "string", value: s }, start });
        break;
      }
      default: {
        if (c >= "0" && c <= "9") {
          let j = i;
          while (j < source.length && ((source[j]! >= "0" && source[j]! <= "9") || source[j] === ".")) {
            j += 1;
          }
          const slice = source.slice(i, j);
          const n = Number(slice);
          if (Number.isNaN(n)) throw new ExprError(`unexpected token \`${slice}\` at ${i}`);
          out.push({ kind: { t: "number", value: n }, start });
          i = j;
        } else if (/[A-Za-z_]/.test(c)) {
          let j = i;
          while (j < source.length && /[0-9A-Za-z_]/.test(source[j]!)) j += 1;
          const word = source.slice(i, j);
          let kind: Tok;
          switch (word) {
            case "true":
              kind = { t: "true" };
              break;
            case "false":
              kind = { t: "false" };
              break;
            case "null":
              kind = { t: "null" };
              break;
            case "and":
              kind = { t: "and" };
              break;
            case "or":
              kind = { t: "or" };
              break;
            case "not":
              kind = { t: "bang" };
              break;
            case "for":
              kind = { t: "for" };
              break;
            case "in":
              kind = { t: "in" };
              break;
            case "where":
              kind = { t: "where" };
              break;
            default:
              kind = { t: "ident", value: word };
              break;
          }
          out.push({ kind, start });
          i = j;
        } else {
          throw new ExprError(`unexpected token \`${c}\` at ${i}`);
        }
      }
    }
  }
  return out;
}

// ---------------------------------------------------------------------
// Parser (precedence climbing)
// ---------------------------------------------------------------------

class ExprParser {
  tokens: Token[];
  pos: number;

  constructor(tokens: Token[]) {
    this.tokens = tokens;
    this.pos = 0;
  }

  peek(): Tok | null {
    const t = this.tokens[this.pos];
    return t !== undefined ? t.kind : null;
  }

  bump(): Token | null {
    const t = this.tokens[this.pos];
    if (t !== undefined) this.pos += 1;
    return t ?? null;
  }

  parseExpr(minPrec: number): Expr {
    let lhs = this.parsePrefix();
    let t = this.peek();
    while (t !== null) {
      const p = binopPrec(t);
      if (p === null) break;
      const [op, prec] = p;
      if (prec < minPrec) break;
      this.bump();
      const rhs = this.parseExpr(prec + 1);
      lhs = { kind: "binary", op, left: lhs, right: rhs };
      t = this.peek();
    }
    return lhs;
  }

  parsePrefix(): Expr {
    const tok = this.bump();
    if (tok === null) throw new ExprError("expected expression");
    const k = tok.kind;
    switch (k.t) {
      case "number":
        return { kind: "number", value: k.value };
      case "string":
        return { kind: "string", value: k.value };
      case "true":
        return { kind: "bool", value: true };
      case "false":
        return { kind: "bool", value: false };
      case "null":
        return { kind: "null" };
      case "minus":
        return { kind: "unary", op: "neg", inner: this.parseExpr(PRECEDENCE_UNARY) };
      case "bang":
        return { kind: "unary", op: "not", inner: this.parseExpr(PRECEDENCE_UNARY) };
      case "lparen": {
        const inner = this.parseExpr(0);
        const close = this.bump();
        if (close === null || close.kind.t !== "rparen") throw new ExprError("expected `)`");
        return inner;
      }
      case "lbracket": {
        if (this.peek()?.t === "rbracket") {
          this.bump();
          return { kind: "list", items: [] };
        }
        const first = this.parseExpr(0);
        if (this.peek()?.t === "for") {
          this.bump();
          const varTok = this.bump();
          if (varTok === null || varTok.kind.t !== "ident") {
            throw new ExprError("expected identifier after `for`");
          }
          const varName = varTok.kind.value;
          if (this.peek()?.t !== "in") throw new ExprError("expected `in`");
          this.bump();
          const source = this.parseExpr(0);
          let filter: Expr | null = null;
          if (this.peek()?.t === "where") {
            this.bump();
            filter = this.parseExpr(0);
          }
          const close = this.bump();
          if (close === null || close.kind.t !== "rbracket") throw new ExprError("expected `]`");
          return { kind: "listComp", value: first, var: varName, source, filter };
        }
        const items = [first];
        while (this.peek()?.t === "comma") {
          this.bump();
          if (this.peek()?.t === "rbracket") break;
          items.push(this.parseExpr(0));
        }
        const close = this.bump();
        if (close === null || close.kind.t !== "rbracket") throw new ExprError("expected `]`");
        return { kind: "list", items };
      }
      case "ident": {
        const name = k.value;
        if (this.peek()?.t === "lparen") {
          this.bump();
          const args: Expr[] = [];
          if (this.peek()?.t !== "rparen") {
            for (;;) {
              args.push(this.parseExpr(0));
              if (this.peek()?.t === "comma") {
                this.bump();
                continue;
              }
              break;
            }
          }
          const close = this.bump();
          if (close === null || close.kind.t !== "rparen") throw new ExprError("expected `)`");
          return { kind: "call", name, args };
        }
        const path = [name];
        while (this.peek()?.t === "dot") {
          this.bump();
          const seg = this.bump();
          if (seg === null || seg.kind.t !== "ident") {
            throw new ExprError("expected identifier after `.`");
          }
          path.push(seg.kind.value);
        }
        return { kind: "path", segments: path };
      }
      default:
        throw new ExprError(`unexpected token \`${k.t}\` at ${tok.start}`);
    }
  }
}

function binopPrec(t: Tok): [BinOp, number] | null {
  switch (t.t) {
    case "or":
      return ["or", 1];
    case "and":
      return ["and", 2];
    case "eqeq":
      return ["eq", 3];
    case "noteq":
      return ["ne", 3];
    case "lt":
      return ["lt", 4];
    case "le":
      return ["le", 4];
    case "gt":
      return ["gt", 4];
    case "ge":
      return ["ge", 4];
    case "plus":
      return ["add", 5];
    case "minus":
      return ["sub", 5];
    case "star":
      return ["mul", 6];
    case "slash":
      return ["div", 6];
    case "percent":
      return ["mod", 6];
    default:
      return null;
  }
}

const PRECEDENCE_UNARY = 7;
