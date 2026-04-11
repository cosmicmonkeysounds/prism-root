import { describe, it, expect } from "vitest";
import { parse } from "./parser.js";
import { evaluate, evaluateExpression } from "./evaluator.js";
import type { ValueStore, AnyExprNode } from "./expression-types.js";

const EMPTY_STORE: ValueStore = {
  resolve: () => 0,
};

function parseOk(source: string): AnyExprNode {
  const { node, errors } = parse(source);
  expect(errors).toHaveLength(0);
  expect(node).not.toBeNull();
  return node as AnyExprNode;
}

describe("parser — literals", () => {
  it("parses number", () => {
    const node = parseOk("42");
    expect(node).toEqual({ kind: "literal", value: 42, exprType: "number" });
  });

  it("parses float", () => {
    const node = parseOk("3.14");
    expect(node).toEqual({ kind: "literal", value: 3.14, exprType: "number" });
  });

  it("parses string", () => {
    const node = parseOk("'hello'");
    expect(node).toEqual({ kind: "literal", value: "hello", exprType: "string" });
  });

  it("parses boolean true", () => {
    const node = parseOk("true");
    expect(node).toEqual({ kind: "literal", value: true, exprType: "boolean" });
  });

  it("parses boolean false", () => {
    const node = parseOk("false");
    expect(node).toEqual({ kind: "literal", value: false, exprType: "boolean" });
  });
});

describe("parser — operands", () => {
  it("parses [type:id]", () => {
    const node = parseOk("[var:health]");
    expect(node).toEqual({ kind: "operand", operandType: "var", id: "health" });
  });

  it("parses [type:id.subfield]", () => {
    const node = parseOk("[obj:player.name]");
    expect(node).toEqual({
      kind: "operand",
      operandType: "obj",
      id: "player",
      subfield: "name",
    });
  });

  it("parses bare identifier as field operand", () => {
    const node = parseOk("foo");
    expect(node).toEqual({ kind: "operand", operandType: "field", id: "foo" });
  });
});

describe("parser — binary operators", () => {
  it("parses addition", () => {
    const node = parseOk("1 + 2");
    expect(node).toMatchObject({ kind: "binary", op: "+" });
  });

  it("parses subtraction", () => {
    const node = parseOk("5 - 3");
    expect(node).toMatchObject({ kind: "binary", op: "-" });
  });

  it("parses multiplication", () => {
    const node = parseOk("2 * 3");
    expect(node).toMatchObject({ kind: "binary", op: "*" });
  });

  it("parses division", () => {
    const node = parseOk("6 / 2");
    expect(node).toMatchObject({ kind: "binary", op: "/" });
  });

  it("parses modulo", () => {
    const node = parseOk("7 % 3");
    expect(node).toMatchObject({ kind: "binary", op: "%" });
  });

  it("parses power", () => {
    const node = parseOk("2 ^ 3");
    expect(node).toMatchObject({ kind: "binary", op: "^" });
  });

  it("parses comparison ==", () => {
    const node = parseOk("a == b");
    expect(node).toMatchObject({ kind: "binary", op: "==" });
  });

  it("parses comparison !=", () => {
    const node = parseOk("a != b");
    expect(node).toMatchObject({ kind: "binary", op: "!=" });
  });

  it("parses comparison <", () => {
    const node = parseOk("a < b");
    expect(node).toMatchObject({ kind: "binary", op: "<" });
  });

  it("parses comparison >=", () => {
    const node = parseOk("a >= b");
    expect(node).toMatchObject({ kind: "binary", op: ">=" });
  });

  it("parses and", () => {
    const node = parseOk("a and b");
    expect(node).toMatchObject({ kind: "binary", op: "and" });
  });

  it("parses or", () => {
    const node = parseOk("a or b");
    expect(node).toMatchObject({ kind: "binary", op: "or" });
  });
});

describe("parser — unary operators", () => {
  it("parses negation", () => {
    const node = parseOk("-5");
    expect(node).toMatchObject({ kind: "unary", op: "-" });
  });

  it("parses not", () => {
    const node = parseOk("not true");
    expect(node).toMatchObject({ kind: "unary", op: "not" });
  });
});

describe("parser — function calls", () => {
  it("parses single arg", () => {
    const node = parseOk("abs(-5)");
    expect(node).toMatchObject({ kind: "call", name: "abs" });
    if (node.kind === "call") expect(node.args).toHaveLength(1);
  });

  it("parses multiple args", () => {
    const node = parseOk("clamp(x, 0, 100)");
    expect(node).toMatchObject({ kind: "call", name: "clamp" });
    if (node.kind === "call") expect(node.args).toHaveLength(3);
  });

  it("parses no args", () => {
    const node = parseOk("foo()");
    if (node.kind === "call") expect(node.args).toHaveLength(0);
  });
});

describe("parser — precedence", () => {
  it("multiplication before addition", () => {
    const node = parseOk("1 + 2 * 3");
    // Should be 1 + (2 * 3)
    expect(node).toMatchObject({
      kind: "binary",
      op: "+",
      right: { kind: "binary", op: "*" },
    });
  });

  it("and before or", () => {
    const node = parseOk("a or b and c");
    // Should be a or (b and c)
    expect(node).toMatchObject({
      kind: "binary",
      op: "or",
      right: { kind: "binary", op: "and" },
    });
  });

  it("power is right-associative", () => {
    const node = parseOk("2 ^ 3 ^ 4");
    // Should be 2 ^ (3 ^ 4)
    expect(node).toMatchObject({
      kind: "binary",
      op: "^",
      right: { kind: "binary", op: "^" },
    });
  });

  it("parentheses override precedence", () => {
    const node = parseOk("(1 + 2) * 3");
    expect(node).toMatchObject({
      kind: "binary",
      op: "*",
      left: { kind: "binary", op: "+" },
    });
  });
});

describe("parser — errors", () => {
  it("empty input returns null node with no errors", () => {
    const { node, errors } = parse("");
    expect(node).toBeNull();
    expect(errors).toHaveLength(0);
  });

  it("unexpected token produces error", () => {
    const { errors } = parse(")");
    expect(errors.length).toBeGreaterThan(0);
  });

  it("unclosed paren produces error", () => {
    const { errors } = parse("(1 + 2");
    expect(errors.length).toBeGreaterThan(0);
  });
});

describe("evaluator — arithmetic", () => {
  it("addition", () => {
    expect(evaluate(parseOk("1 + 2"), EMPTY_STORE)).toBe(3);
  });

  it("subtraction", () => {
    expect(evaluate(parseOk("10 - 3"), EMPTY_STORE)).toBe(7);
  });

  it("multiplication", () => {
    expect(evaluate(parseOk("4 * 5"), EMPTY_STORE)).toBe(20);
  });

  it("division", () => {
    expect(evaluate(parseOk("15 / 3"), EMPTY_STORE)).toBe(5);
  });

  it("division by zero returns 0", () => {
    expect(evaluate(parseOk("5 / 0"), EMPTY_STORE)).toBe(0);
  });

  it("modulo", () => {
    expect(evaluate(parseOk("7 % 3"), EMPTY_STORE)).toBe(1);
  });

  it("power", () => {
    expect(evaluate(parseOk("2 ^ 3"), EMPTY_STORE)).toBe(8);
  });

  it("negation", () => {
    expect(evaluate(parseOk("-5"), EMPTY_STORE)).toBe(-5);
  });

  it("complex expression", () => {
    expect(evaluate(parseOk("(1 + 2) * 3 - 1"), EMPTY_STORE)).toBe(8);
  });
});

describe("evaluator — string concatenation", () => {
  it("string + string", () => {
    expect(evaluate(parseOk("'hello' + ' world'"), EMPTY_STORE)).toBe("hello world");
  });

  it("string + number", () => {
    expect(evaluate(parseOk("'count: ' + 42"), EMPTY_STORE)).toBe("count: 42");
  });
});

describe("evaluator — comparison", () => {
  it("== true", () => {
    expect(evaluate(parseOk("1 == 1"), EMPTY_STORE)).toBe(true);
  });

  it("== false", () => {
    expect(evaluate(parseOk("1 == 2"), EMPTY_STORE)).toBe(false);
  });

  it("!= true", () => {
    expect(evaluate(parseOk("1 != 2"), EMPTY_STORE)).toBe(true);
  });

  it("< true", () => {
    expect(evaluate(parseOk("1 < 2"), EMPTY_STORE)).toBe(true);
  });

  it(">= true", () => {
    expect(evaluate(parseOk("5 >= 5"), EMPTY_STORE)).toBe(true);
  });
});

describe("evaluator — boolean logic", () => {
  it("and short-circuits", () => {
    expect(evaluate(parseOk("false and true"), EMPTY_STORE)).toBe(false);
  });

  it("or short-circuits", () => {
    expect(evaluate(parseOk("true or false"), EMPTY_STORE)).toBe(true);
  });

  it("not", () => {
    expect(evaluate(parseOk("not true"), EMPTY_STORE)).toBe(false);
  });

  it("complex boolean", () => {
    expect(evaluate(parseOk("true and not false"), EMPTY_STORE)).toBe(true);
  });
});

describe("evaluator — builtin functions", () => {
  it("abs", () => {
    expect(evaluate(parseOk("abs(-5)"), EMPTY_STORE)).toBe(5);
  });

  it("ceil", () => {
    expect(evaluate(parseOk("ceil(1.3)"), EMPTY_STORE)).toBe(2);
  });

  it("floor", () => {
    expect(evaluate(parseOk("floor(1.9)"), EMPTY_STORE)).toBe(1);
  });

  it("round", () => {
    expect(evaluate(parseOk("round(1.5)"), EMPTY_STORE)).toBe(2);
  });

  it("sqrt", () => {
    expect(evaluate(parseOk("sqrt(16)"), EMPTY_STORE)).toBe(4);
  });

  it("min", () => {
    expect(evaluate(parseOk("min(3, 7)"), EMPTY_STORE)).toBe(3);
  });

  it("max", () => {
    expect(evaluate(parseOk("max(3, 7)"), EMPTY_STORE)).toBe(7);
  });

  it("clamp", () => {
    expect(evaluate(parseOk("clamp(15, 0, 10)"), EMPTY_STORE)).toBe(10);
  });

  it("pow", () => {
    expect(evaluate(parseOk("pow(2, 8)"), EMPTY_STORE)).toBe(256);
  });
});

describe("evaluator — operand resolution", () => {
  it("resolves operands via ValueStore", () => {
    const store: ValueStore = {
      resolve: (type, id) => (type === "var" && id === "hp" ? 100 : 0),
    };
    expect(evaluate(parseOk("[var:hp] > 50"), store)).toBe(true);
  });
});

describe("evaluateExpression (convenience)", () => {
  it("evaluates with bare identifiers", () => {
    const { result, errors } = evaluateExpression("x + y", { x: 10, y: 20 });
    expect(errors).toHaveLength(0);
    expect(result).toBe(30);
  });

  it("evaluates with full operand syntax", () => {
    const { result } = evaluateExpression("[field:score] * 2", { score: 5 });
    expect(result).toBe(10);
  });

  it("handles string comparison", () => {
    const { result } = evaluateExpression("status == 'done'", { status: "done" });
    expect(result).toBe(true);
  });

  it("handles complex expression", () => {
    const { result } = evaluateExpression("subtotal + tax", { subtotal: 100, tax: 8.5 });
    expect(result).toBe(108.5);
  });

  it("returns errors for invalid syntax", () => {
    const { errors } = evaluateExpression("1 +", {});
    expect(errors.length).toBeGreaterThan(0);
  });

  it("preserves builtins as function calls", () => {
    const { result } = evaluateExpression("abs(x)", { x: -5 });
    expect(result).toBe(5);
  });
});

describe("evaluator — extended string builtins", () => {
  it("len", () => {
    expect(evaluate(parseOk("len('hello')"), EMPTY_STORE)).toBe(5);
  });

  it("lower", () => {
    expect(evaluate(parseOk("lower('HeLLo')"), EMPTY_STORE)).toBe("hello");
  });

  it("upper", () => {
    expect(evaluate(parseOk("upper('hey')"), EMPTY_STORE)).toBe("HEY");
  });

  it("trim", () => {
    const { result } = evaluateExpression("trim(s)", { s: "  hi  " });
    expect(result).toBe("hi");
  });

  it("concat", () => {
    expect(evaluate(parseOk("concat('a', 'b', 'c')"), EMPTY_STORE)).toBe("abc");
  });

  it("left", () => {
    expect(evaluate(parseOk("left('prism', 3)"), EMPTY_STORE)).toBe("pri");
  });

  it("right", () => {
    expect(evaluate(parseOk("right('prism', 3)"), EMPTY_STORE)).toBe("ism");
  });

  it("mid", () => {
    expect(evaluate(parseOk("mid('prism', 1, 3)"), EMPTY_STORE)).toBe("ris");
  });

  it("substitute", () => {
    expect(evaluate(parseOk("substitute('foo bar foo', 'foo', 'baz')"), EMPTY_STORE)).toBe("baz bar baz");
  });
});

describe("evaluator — extended date builtins", () => {
  it("year", () => {
    expect(evaluate(parseOk("year('2025-06-15')"), EMPTY_STORE)).toBe(2025);
  });

  it("month", () => {
    expect(evaluate(parseOk("month('2025-06-15')"), EMPTY_STORE)).toBe(6);
  });

  it("day", () => {
    expect(evaluate(parseOk("day('2025-06-15')"), EMPTY_STORE)).toBe(15);
  });

  it("datediff days", () => {
    expect(evaluate(parseOk("datediff('2025-01-01', '2025-01-11', 'days')"), EMPTY_STORE)).toBe(10);
  });

  it("datediff months", () => {
    expect(evaluate(parseOk("datediff('2025-01-01', '2025-04-01', 'months')"), EMPTY_STORE)).toBe(3);
  });

  it("datediff years", () => {
    expect(evaluate(parseOk("datediff('2020-01-01', '2025-01-01', 'years')"), EMPTY_STORE)).toBe(5);
  });

  it("today returns YYYY-MM-DD", () => {
    const result = evaluate(parseOk("today()"), EMPTY_STORE);
    expect(typeof result).toBe("string");
    expect(result as string).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  it("now returns ISO timestamp", () => {
    const result = evaluate(parseOk("now()"), EMPTY_STORE);
    expect(typeof result).toBe("string");
    expect(result as string).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });
});

describe("evaluator — aggregate builtins", () => {
  it("sum", () => {
    expect(evaluate(parseOk("sum(1, 2, 3, 4)"), EMPTY_STORE)).toBe(10);
  });

  it("avg", () => {
    expect(evaluate(parseOk("avg(2, 4, 6)"), EMPTY_STORE)).toBe(4);
  });

  it("count", () => {
    expect(evaluate(parseOk("count(1, 2, 3, 4, 5)"), EMPTY_STORE)).toBe(5);
  });

  it("avg of empty returns 0", () => {
    expect(evaluate(parseOk("avg()"), EMPTY_STORE)).toBe(0);
  });
});
