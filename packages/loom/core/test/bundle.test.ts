import { describe, expect, it } from "vitest";
import { parse } from "../src/parser/index.ts";
import { Bundle, type LoomFileEntry } from "../src/runtime/index.ts";

function bundleFrom(...sources: string[]): Bundle {
  const b = new Bundle();
  sources.forEach((src, i) => {
    const [file, diagnostics] = parse(src);
    const entry: LoomFileEntry = {
      path: `f${i}.loom`,
      stem: `f${i}`,
      qualifier: "",
      source: src,
      file,
      diagnostics,
    };
    b.files.push(entry);
  });
  b.rebuildSimulacra();
  return b;
}

describe("bundle inheritance", () => {
  it("merges ITEM properties across `is` inheritance", () => {
    const b = bundleFrom(`ITEM LootBag
  contents: list of ITEM = []
  gold: int = 0
  weight: int = 5

ITEM goblin_pouch is LootBag
  gold: 3
`);
    const pouch = b.items.get("goblin_pouch")!;
    expect(pouch.inherits).toEqual(["LootBag"]);
    const names = pouch.properties.map((p) => p.name);
    expect(names).toEqual(["contents", "weight", "gold"]);
    // The child's `gold: 3` carries no `= default`, so `default` is null
    // (parity with the Rust parser — only `gold: int = 0` has one).
    const gold = pouch.properties.find((p) => p.name === "gold")!;
    expect(gold.default).toBeNull();
    // The parent LootBag keeps its `= 0` default.
    const bagGold = b.items.get("LootBag")!.properties.find((p) => p.name === "gold")!;
    expect(bagGold.default).toBe("0");
  });

  it("materialises a character once its required slot is filled", () => {
    const b = bundleFrom(`TRAIT Keeper
  voice: any

CHARACTER Wren is Keeper
  voice: female_alto
`);
    expect(b.mergedCharacters.has("Wren")).toBe(true);
    // TRAITs are not standalone characters.
    expect(b.mergedCharacters.has("Keeper")).toBe(false);
    const wren = b.mergedCharacters.get("Wren")!;
    expect(wren.properties.get("voice")!.value).toBe("female_alto");
    expect(b.projectDiagnostics).toHaveLength(0);
  });

  it("flags an unfilled required slot as abstract", () => {
    const b = bundleFrom(`CHARACTER Keeper
  voice: any
`);
    expect(b.mergedCharacters.has("Keeper")).toBe(false);
    expect(
      b.projectDiagnostics.some(
        (d) => d.kind === "requiredSlotUnfilled" && d.character === "Keeper" && d.slot === "voice",
      ),
    ).toBe(true);
  });

  it("composes inherited hooks and disposition", () => {
    const b = bundleFrom(`TRAIT Guardian
  trusts Player: 10 of 100
  on meeting Player
    -> greet

CHARACTER Wren is Guardian
  on meeting Player
    -> wave
`);
    const wren = b.mergedCharacters.get("Wren")!;
    // Inherited + own hook both present (source order: parent then child).
    expect(wren.hooks).toHaveLength(2);
    expect(wren.disposition.find((d) => d.verb === "trusts" && d.target === "Player")).toBeTruthy();
  });
});
