import { describe, expect, it } from "vitest";
import {
  CallArg,
  ExprError,
  World,
  display,
  evaluate,
  parseExpr,
  vNumber,
  vString,
  type Value,
} from "../src/runtime/expr.ts";

function noCalls(name: string): Value {
  throw new ExprError(`unknown function \`${name}\``);
}

function ev(src: string, world: World): Value {
  return evaluate(parseExpr(src), world, noCalls);
}

describe("expr", () => {
  it("evaluates literals and arithmetic", () => {
    const w = new World();
    expect(ev("1 + 2 * 3", w)).toEqual(vNumber(7));
    expect(ev("(1 + 2) * 3", w)).toEqual(vNumber(9));
    expect(ev("-5 + 10", w)).toEqual(vNumber(5));
  });

  it("evaluates comparisons and logic", () => {
    const w = new World();
    expect(ev("3 > 2 and 1 == 1", w)).toEqual({ kind: "bool", value: true });
    expect(ev("3 < 2 or 1 == 1", w)).toEqual({ kind: "bool", value: true });
    expect(ev("not false", w)).toEqual({ kind: "bool", value: true });
  });

  it("defaults unknown paths to null", () => {
    const w = new World();
    w.set("Wren.trust", vNumber(72));
    expect(ev("Wren.trust", w)).toEqual(vNumber(72));
    expect(ev("Wren.unknown", w)).toEqual({ kind: "null" });
    expect(ev("Wren.trust > 50", w)).toEqual({ kind: "bool", value: true });
  });

  it("concatenates strings", () => {
    const w = new World();
    expect(ev("'hi ' + 'there'", w)).toEqual(vString("hi there"));
    expect(ev("'n=' + 3", w)).toEqual(vString("n=3"));
  });

  it("filters and maps a list comprehension", () => {
    const w = new World();
    w.setCollection("Characters", {
      kind: "list",
      items: [vString("Wren"), vString("Fisher"), vString("Mara")],
    });
    w.set("Player.faction", vString("dawn"));
    w.set("Wren.faction", vString("dawn"));
    w.set("Fisher.faction", vString("dusk"));
    w.set("Mara.faction", vString("dawn"));
    const v = ev("[c for c in Characters where c.faction == Player.faction]", w);
    const names = v.kind === "list" ? v.items.map(display) : [];
    expect(names).toEqual(["Wren", "Mara"]);
  });

  it("chains comprehensions through let results", () => {
    const w = new World();
    w.set("nearby", { kind: "list", items: [vString("Wren"), vString("Fisher")] });
    w.set("Wren.disposition.Player", vString("hostile"));
    w.set("Fisher.disposition.Player", vString("warm"));
    const v = ev("[c for c in nearby where c.disposition.Player == 'hostile']", w);
    const names = v.kind === "list" ? v.items.map(display) : [];
    expect(names).toEqual(["Wren"]);
  });

  it("invokes a user function", () => {
    const w = new World();
    const expr = parseExpr("count(7) + 1");
    let calls = 0;
    const f = (name: string, args: CallArg[]): Value => {
      expect(name).toBe("count");
      expect(args).toHaveLength(1);
      expect(args[0]!.value).toEqual(vNumber(7));
      calls += 1;
      return vNumber(10);
    };
    expect(evaluate(expr, w, f)).toEqual(vNumber(11));
    expect(calls).toBe(1);
  });
});
