//! Compiled program — a parsed project indexed for runtime lookup.
//!
//! A `Bundle` holds every `.loom` file in a project plus the cross-file
//! name indices the resolver reads against. Construction (parsing +
//! indexing) lives in `project`; resolution lives in `resolver`.
//!
//! Port status: the file/declaration indices and the ITEM / FACTION /
//! CHARACTER inheritance-merge logic (pure over the parser AST) are
//! fully ported. The character/stats/tree → runtime-state *compilation*
//! and SCENE/GENERATOR coroutine lowering are layered on by the
//! meridian / simulacra / coroutine ports (see `rebuildSimulacra`).

import type {
  Beat,
  CharacterBody,
  CohortBody,
  Diagnostic,
  FactionBody,
  GeneratorBody,
  HookDecl,
  ItemBody,
  LocationBody,
  LoomFile,
  MethodDecl,
  MixinRef,
  OwnedBeat,
  PersonBody,
  Property,
  RosterBody,
  SceneBody,
} from "../parser/index.ts";
import { emptyCharacterBody, parseMixinRef, slotTypeIsRequiredHole } from "../parser/index.ts";

/** Index into `Bundle.files`. */
export type FileIdx = number;
/** Index of a `Beat` inside a file's `items` list. */
export type BeatIdx = number;

/** Stable handle to one beat inside the bundle. */
export interface BeatRef {
  file: FileIdx;
  beat: BeatIdx;
}

export function beatRefEquals(a: BeatRef, b: BeatRef): boolean {
  return a.file === b.file && a.beat === b.beat;
}

/** One loaded `.loom` file. */
export interface LoomFileEntry {
  /** Project-relative path, normalised with forward slashes. */
  path: string;
  /** File stem — `cast/Wren.loom` → `Wren`. */
  stem: string;
  /** Forward-slash qualifier — `cast/Wren.loom` → `cast`. */
  qualifier: string;
  /** Raw source, kept so span resolution needn't re-read from disk. */
  source: string;
  file: LoomFile;
  diagnostics: Diagnostic[];
}

/**
 * The entry beat for a file — the first `Item::Beat` in source order,
 * used when a divert names a file by stem instead of a beat.
 */
export function entryBeatIndex(entry: LoomFileEntry): BeatIdx | null {
  for (let idx = 0; idx < entry.file.items.length; idx++) {
    if (entry.file.items[idx]!.kind === "beat") return idx;
  }
  return null;
}

/** Project-level diagnostics emitted by the loader (cross-file). */
export type ProjectDiagnostic =
  | { kind: "missingMainFile" }
  | { kind: "entryBeatUnresolved"; name: string }
  | { kind: "noEntryBeat" }
  | { kind: "ambiguousSlot"; character: string; prop: string }
  | { kind: "requiredSlotUnfilled"; character: string; slot: string }
  | { kind: "requiredParamUnfilled"; character: string; trait: string; param: string }
  | { kind: "unresolvedTraitArg"; character: string; trait: string; param: string; arg: string }
  | { kind: "derivedBeatConflict"; character: string; beat: string }
  | { kind: "unfilledDerivedSlot"; character: string; beat: string; slot: string };

/** One compiled Loom project. */
export class Bundle {
  files: LoomFileEntry[] = [];
  /** `beatName → [BeatRef, …]`. */
  beatsByName = new Map<string, BeatRef[]>();
  /** `fileStem → [FileIdx, …]`. */
  filesByStem = new Map<string, FileIdx[]>();
  /** The starting beat from `main.loom`'s `entry:` property, or null. */
  entry: BeatRef | null = null;
  projectDiagnostics: ProjectDiagnostic[] = [];

  /**
   * Merged CHARACTER / TRAIT bodies (after `is X, Y` inheritance,
   * spec §9). The simulacra port compiles these into `CharacterState`;
   * until then the merged bodies live here so callers can read the
   * fully-resolved declaration.
   */
  mergedCharacters = new Map<string, CharacterBody>();

  /** Top-level SCENE declarations (spec §12.3), keyed by name. */
  scenes = new Map<string, SceneBody>();
  /** Top-level GENERATOR declarations (spec §12.4), keyed by name. */
  generators = new Map<string, GeneratorBody>();
  /** COHORT declarations (spec §13.1), keyed by name. */
  cohorts = new Map<string, CohortBody>();
  /** LOCATION declarations (spec §13.1), keyed by name. */
  locations = new Map<string, LocationBody>();
  /** ITEM declarations (spec §9), inheritance already resolved. */
  items = new Map<string, ItemBody>();
  /** FACTION declarations (spec §9), inheritance already resolved. */
  factions = new Map<string, FactionBody>();
  /** PERSON declarations (spec v3 §13.2), keyed by name. */
  persons = new Map<string, PersonBody>();
  /** ROSTER declarations (spec v3 §13.3), keyed by name. */
  rosters = new Map<string, RosterBody>();

  file(idx: FileIdx): LoomFileEntry {
    return this.files[idx]!;
  }

  beat(r: BeatRef): Beat {
    const item = this.file(r.file).file.items[r.beat];
    if (item === undefined || item.kind !== "beat") {
      throw new Error("BeatRef does not point at a beat — index corrupted");
    }
    return item.value;
  }

  /** All parser diagnostics across every file, in file order. */
  parserDiagnostics(): Array<[LoomFileEntry, Diagnostic]> {
    const out: Array<[LoomFileEntry, Diagnostic]> = [];
    for (const f of this.files) {
      for (const d of f.diagnostics) out.push([f, d]);
    }
    return out;
  }

  /**
   * Pre-populate the declaration indices from the parsed file ASTs.
   * Idempotent — clears existing maps first.
   *
   * The ITEM / FACTION / CHARACTER inheritance merges and the
   * abstractness diagnostics (spec §8 + §9) run here. Compiling merged
   * CHARACTER bodies into `CharacterState`, STATS into `StatsProfile`,
   * TREE into `Tree`, and lowering SCENE / GENERATOR coroutines is the
   * job of the meridian / simulacra / coroutine layer, which calls back
   * into the bodies this method resolves.
   */
  rebuildSimulacra(): void {
    this.scenes.clear();
    this.generators.clear();
    this.cohorts.clear();
    this.locations.clear();
    this.items.clear();
    this.factions.clear();
    this.persons.clear();
    this.rosters.clear();
    this.mergedCharacters.clear();
    // Idempotency: this method is the sole producer of `requiredSlotUnfilled`
    // / `ambiguousSlot`, so clear them here too. `compileModel` may run more
    // than once on a bundle (tests, re-compiles); without this a second pass
    // double-reports every abstractness diagnostic.
    this.projectDiagnostics = [];

    // Pass 1: collect raw ITEM / FACTION bodies (merge order = source).
    const rawItems = new Map<string, ItemBody>();
    const itemOrder: string[] = [];
    const rawFactions = new Map<string, FactionBody>();
    const factionOrder: string[] = [];
    for (const entry of this.files) {
      for (const item of entry.file.items) {
        if (item.kind !== "declaration") continue;
        const decl = item.value;
        switch (decl.kind) {
          case "cohort":
            if (decl.cohort) this.cohorts.set(decl.name, decl.cohort);
            break;
          case "location":
            if (decl.location) this.locations.set(decl.name, decl.location);
            break;
          case "person":
            if (decl.person) this.persons.set(decl.name, decl.person);
            break;
          case "roster":
            if (decl.roster) this.rosters.set(decl.name, decl.roster);
            break;
          case "item":
            if (decl.item) {
              if (!rawItems.has(decl.name)) itemOrder.push(decl.name);
              rawItems.set(decl.name, decl.item);
            }
            break;
          case "faction":
            if (decl.faction) {
              if (!rawFactions.has(decl.name)) factionOrder.push(decl.name);
              rawFactions.set(decl.name, decl.faction);
            }
            break;
          case "scene":
            if (decl.scene) this.scenes.set(decl.name, decl.scene);
            break;
          case "generator":
            if (decl.generator) this.generators.set(decl.name, decl.generator);
            break;
          default:
            break;
        }
      }
    }

    // Resolve ITEM / FACTION inheritance.
    const itemCache = new Map<string, ItemBody>();
    for (const name of itemOrder) {
      this.items.set(name, mergeItem(name, rawItems, itemCache));
    }
    const factionCache = new Map<string, FactionBody>();
    for (const name of factionOrder) {
      this.factions.set(name, mergeFaction(name, rawFactions, factionCache));
    }

    // Pass 2: CHARACTER + TRAIT. Collect raw declarations, merge `is`.
    // Also gather every top-level beat name so a trait arg used only as a
    // divert target can be checked against the real beats (unresolvedTraitArg).
    const rawDecls = new Map<string, RawDecl>();
    const declOrder: string[] = [];
    const globalBeats = new Set<string>();
    for (const entry of this.files) {
      for (const item of entry.file.items) {
        if (item.kind === "beat") {
          globalBeats.add(item.value.name);
          continue;
        }
        if (item.kind !== "declaration") continue;
        const decl = item.value;
        if (decl.kind === "character" || decl.kind === "role" || decl.kind === "trait") {
          if (decl.character) {
            const isTrait = decl.kind === "trait";
            const isRole = decl.kind === "role";
            if (!rawDecls.has(decl.name)) declOrder.push(decl.name);
            rawDecls.set(decl.name, { body: decl.character, inherits: decl.mixin, isTrait, isRole });
          }
        }
      }
    }
    const merged = new Map<string, CharacterBody>();
    for (const name of declOrder) {
      const mergedBody = mergeCharacter(name, rawDecls, merged, this.projectDiagnostics, new Set(), globalBeats);
      merged.set(name, mergedBody);
    }
    for (const name of declOrder) {
      const raw = rawDecls.get(name)!;
      const mergedBody = merged.get(name) ?? emptyCharacterBody();
      if (raw.isTrait) continue;
      // A ROLE is a per-person state *schema*, never an instance: an
      // `any of FACTION` slot is filled at runtime when a guest picks a
      // faction, so a role is never "abstract". Exempt it from the
      // required-slot drop below — otherwise no ROLE could mix in a trait
      // (its merged body would be discarded and `compileModel` would fall
      // back to the un-merged raw declaration).
      if (raw.isRole) {
        this.mergedCharacters.set(name, mergedBody);
        continue;
      }

      // Required-slot abstractness check (spec §8) — CHARACTERs only.
      const byName = new Map<string, Property[]>();
      for (const prop of mergedBody.typedProperties) {
        const list = byName.get(prop.name);
        if (list) list.push(prop);
        else byName.set(prop.name, [prop]);
      }
      const unfilled: string[] = [];
      for (const [slotName, entries] of byName) {
        const stillRequired = entries.every((p) => {
          const hasDefault = p.default !== null;
          return p.slotType !== null ? slotTypeIsRequiredHole(p.slotType, hasDefault) : false;
        });
        if (stillRequired) unfilled.push(slotName);
      }
      if (unfilled.length > 0) {
        for (const slot of unfilled) {
          this.projectDiagnostics.push({ kind: "requiredSlotUnfilled", character: name, slot });
        }
        continue;
      }
      this.mergedCharacters.set(name, mergedBody);
    }
  }
}

interface RawDecl {
  body: CharacterBody;
  inherits: string[];
  isTrait: boolean;
  /** True for `ROLE` — a state schema, exempt from the abstractness drop. */
  isRole: boolean;
}

/**
 * Recursively merge a character's declared body with each parent in its
 * `inherits` list (left-to-right). Child declarations win; parent slots
 * fill gaps; ambiguous parent defaults surface as `ambiguousSlot`.
 */
export function mergeCharacter(
  name: string,
  raw: Map<string, RawDecl>,
  mergedCache: Map<string, CharacterBody>,
  diagnostics: ProjectDiagnostic[],
  visiting: Set<string>,
  globalBeats: Set<string> = new Set(),
): CharacterBody {
  if (visiting.has(name)) {
    return cloneCharacterBody(mergedCache.get(name) ?? raw.get(name)?.body ?? emptyCharacterBody());
  }
  const cached = mergedCache.get(name);
  if (cached !== undefined) return cloneCharacterBody(cached);
  visiting.add(name);
  const ownDecl = raw.get(name);
  if (ownDecl === undefined) {
    visiting.delete(name);
    return emptyCharacterBody();
  }
  const own = ownDecl.body;
  const mergedBody = emptyCharacterBody();

  // Every beat this declaration will own — its own plus every one inherited
  // through the `is` chain. A trait param bound to one of these is a route to
  // an *owned* beat, so it must stay `self.`-qualified (spec §11.5); computed
  // upfront (a pure name walk) so it's stable regardless of merge order.
  const ownedBeatNames = collectOwnedBeatNames(name, raw, new Set());

  const parentPropertySources = new Map<string, string>();
  // Keyed by beat NAME → the originating `OwnedBeat` object. A diamond
  // (`Hero is Left, Right`, both `is Base`) propagates ONE authored beat by
  // reference, so identity distinguishes a real name collision (two distinct
  // authored beats) from the same beat reaching a deriver twice.
  const beatSources = new Map<string, OwnedBeat>();
  // Every parent beat by name (even ones the child overrides), so a child's
  // `super` in a re-declared beat can splice the parent template back in.
  const parentBeatsByName = new Map<string, OwnedBeat>();
  for (const entry of ownDecl.inherits) {
    // An `is`-clause entry may carry arguments: `is Scanner(crawler_report)`.
    const ref = parseMixinRef(entry);
    const parentName = ref.name;
    let parentBody = mergeCharacter(parentName, raw, mergedCache, diagnostics, visiting, globalBeats);
    const parentDecl = raw.get(parentName);
    if (parentDecl !== undefined && parentDecl.body.params.length > 0) {
      // Bind the trait's params to this application's args and rewrite every
      // `self.<param>` in the parent body (spec §2.3). Works on a deep clone,
      // so the shared trait cache is never mutated.
      parentBody = substituteParams(
        parentBody,
        parentDecl.body.params,
        ref,
        own.params,
        ownedBeatNames,
        divertOnlyParams(parentName, raw),
        globalBeats,
        diagnostics,
        name,
        parentName,
      );
    }
    for (const [k, v] of parentBody.properties) {
      if (own.properties.has(k)) continue;
      if (parentPropertySources.has(k)) {
        const existing = mergedBody.properties.get(k)?.value ?? null;
        const other = v.value;
        if (existing !== other) {
          diagnostics.push({ kind: "ambiguousSlot", character: name, prop: k });
        }
        continue;
      }
      mergedBody.properties.set(k, v);
      parentPropertySources.set(k, parentName);
    }
    if (mergedBody.statsProfile === null && own.statsProfile === null) {
      mergedBody.statsProfile = parentBody.statsProfile;
      mergedBody.statsCtor = parentBody.statsCtor;
    }
    for (const d of parentBody.disposition) {
      const exists =
        mergedBody.disposition.some((e) => e.verb === d.verb && e.target === d.target) ||
        own.disposition.some((e) => e.verb === d.verb && e.target === d.target);
      if (!exists) mergedBody.disposition.push(d);
    }
    for (const k of parentBody.knowledge) {
      const exists =
        mergedBody.knowledge.some((e) => e.name === k.name) ||
        own.knowledge.some((e) => e.name === k.name);
      if (!exists) mergedBody.knowledge.push(k);
    }
    for (const g of parentBody.goals) {
      const exists =
        mergedBody.goals.some((e) => e.name === g.name) || own.goals.some((e) => e.name === g.name);
      if (!exists) mergedBody.goals.push(g);
    }
    for (const g of parentBody.generators) {
      const exists =
        mergedBody.generators.some((e) => e.name === g.name) ||
        own.generators.some((e) => e.name === g.name);
      if (!exists) mergedBody.generators.push(g);
    }
    for (const r of parentBody.reacts) mergedBody.reacts.push(r);
    for (const h of parentBody.hooks) mergedBody.hooks.push(h);
    for (const p of parentBody.typedProperties) {
      const exists =
        mergedBody.typedProperties.some((e) => e.name === p.name) ||
        own.typedProperties.some((e) => e.name === p.name);
      if (!exists) mergedBody.typedProperties.push(p);
    }
    // A trait may ship an owned `beat` block; the deriver inherits it,
    // namespaced to the deriver at compile (model.ts). Child wins (below); two
    // *distinct* parents shipping the same beat name is a conflict.
    for (const bt of parentBody.beats) {
      if (!parentBeatsByName.has(bt.name)) parentBeatsByName.set(bt.name, bt); // for `super`
      if (own.beats.some((e) => e.name === bt.name)) continue; // child overrides
      const prior = beatSources.get(bt.name);
      if (prior !== undefined) {
        // Same authored beat via two paths (diamond) → fine; two *distinct*
        // beats of one name from different origins → a real conflict.
        if (prior !== bt) {
          diagnostics.push({ kind: "derivedBeatConflict", character: name, beat: bt.name });
        }
        continue;
      }
      beatSources.set(bt.name, bt);
      mergedBody.beats.push(bt);
    }
    // Inherit the trait's `fill` blocks (a parent trait can pre-fill a slot);
    // a child's own fill wins (layered after the loop).
    for (const [k, v] of parentBody.fills) {
      if (!own.fills.has(k) && !mergedBody.fills.has(k)) mergedBody.fills.set(k, v);
    }
  }

  // Layer the child's own declarations on top — child wins.
  for (const [k, v] of own.properties) mergedBody.properties.set(k, v);
  if (own.statsProfile !== null) {
    mergedBody.statsProfile = own.statsProfile;
    mergedBody.statsCtor = own.statsCtor;
  }
  for (const d of own.disposition) mergedBody.disposition.push(d);
  for (const k of own.knowledge) mergedBody.knowledge.push(k);
  for (const g of own.goals) mergedBody.goals.push(g);
  for (const g of own.generators) mergedBody.generators.push(g);
  for (const r of own.reacts) mergedBody.reacts.push(r);
  for (const bt of own.beats) {
    // Override-then-extend: a `super` line in a re-declared beat splices the
    // parent template's body inline (spec §11.4), mirroring hook `super`.
    const parentBeat = parentBeatsByName.get(bt.name);
    if (parentBeat !== undefined && bt.body.some((l) => l.text.trim() === "super")) {
      const composed: OwnedBeat = { name: bt.name, params: bt.params, body: [], span: bt.span };
      for (const line of bt.body) {
        if (line.text.trim() === "super") for (const p of parentBeat.body) composed.body.push(p);
        else composed.body.push(line);
      }
      mergedBody.beats.push(composed);
    } else {
      mergedBody.beats.push(bt);
    }
  }
  for (const [k, v] of own.fills) mergedBody.fills.set(k, v);

  // Hook composition with `super` + `: none` (spec §9.4 + §9.5).
  const suppressed = new Set<string>();
  for (const h of own.hooks) {
    if (h.suppressed) suppressed.add(normaliseHookEvent(h.event));
  }
  if (suppressed.size > 0) {
    mergedBody.hooks = mergedBody.hooks.filter(
      (h) => !suppressed.has(normaliseHookEvent(h.event)),
    );
  }
  for (const h of own.hooks) {
    if (h.suppressed) continue;
    const key = normaliseHookEvent(h.event);
    const hasSuper = h.body.some((line) => line.text.trim() === "super");
    if (hasSuper) {
      let parentIdx: number | null = null;
      for (let i = mergedBody.hooks.length - 1; i >= 0; i--) {
        if (normaliseHookEvent(mergedBody.hooks[i]!.event) === key) {
          parentIdx = i;
          break;
        }
      }
      const parentBody = parentIdx !== null ? mergedBody.hooks[parentIdx]!.body : [];
      if (parentIdx !== null) mergedBody.hooks.splice(parentIdx, 1);
      const composed = { event: h.event, body: [] as typeof h.body, suppressed: false, span: h.span };
      for (const line of h.body) {
        if (line.text.trim() === "super") {
          for (const p of parentBody) composed.body.push(p);
        } else {
          composed.body.push(line);
        }
      }
      mergedBody.hooks.push(composed);
    } else {
      mergedBody.hooks.push(h);
    }
  }
  for (const p of own.typedProperties) mergedBody.typedProperties.push(p);

  // Class layer (spec v3 §9.6): child init wins; methods override by name.
  if (own.init !== null) mergedBody.init = own.init;
  {
    const ownMethodNames = new Set(own.methods.map((m) => m.name));
    mergedBody.methods = mergedBody.methods.filter((m) => !ownMethodNames.has(m.name));
    for (const m of own.methods) mergedBody.methods.push(m);
  }

  visiting.delete(name);
  return mergedBody;
}

function cloneCharacterBody(b: CharacterBody): CharacterBody {
  return {
    params: [...b.params],
    properties: new Map(b.properties),
    statsProfile: b.statsProfile,
    statsCtor: b.statsCtor,
    init: b.init,
    methods: [...b.methods],
    disposition: [...b.disposition],
    reacts: [...b.reacts],
    knowledge: [...b.knowledge],
    goals: [...b.goals],
    hooks: [...b.hooks],
    generators: [...b.generators],
    typedProperties: [...b.typedProperties],
    beats: [...b.beats],
    fills: new Map(b.fills),
  };
}

/**
 * A deep clone of the string-bearing fields `substituteParams` rewrites —
 * hook events + bodies, method bodies + inline expressions, and property
 * values. The shallow `cloneCharacterBody` shares `HookDecl` / `RawLine`
 * objects with the merge cache, so mutating them in place would corrupt the
 * cached trait and make the *next* applier inherit this one's substitutions
 * (e.g. `Crawler` and `Captcha` both routing to `crawler_report`). Fields
 * substitution never touches are copied by reference.
 */
function deepCloneCharacterBody(b: CharacterBody): CharacterBody {
  const c = cloneCharacterBody(b);
  c.hooks = b.hooks.map((h): HookDecl => ({
    event: h.event,
    body: h.body.map((l) => ({ indent: l.indent, text: l.text, span: l.span })),
    suppressed: h.suppressed,
    span: h.span,
  }));
  c.methods = b.methods.map((m): MethodDecl => ({
    name: m.name,
    params: m.params,
    inlineExpr: m.inlineExpr,
    body: m.body.map((l) => ({ indent: l.indent, text: l.text, span: l.span })),
    span: m.span,
  }));
  c.properties = new Map(
    [...b.properties].map(([k, v]) => [k, { value: v.value, span: v.span }]),
  );
  c.beats = b.beats.map((bt) => ({
    name: bt.name,
    params: bt.params,
    body: bt.body.map((l) => ({ indent: l.indent, text: l.text, span: l.span })),
    span: bt.span,
  }));
  c.fills = new Map(
    [...b.fills].map(([k, lines]) => [
      k,
      lines.map((l) => ({ indent: l.indent, text: l.text, span: l.span })),
    ]),
  );
  return c;
}

/** Bind an application's args to a parent's params — named by name, positional
 *  in order (skipping named-filled slots). Mirrors `substituteParams`. */
function bindArgs(ref: MixinRef, parentParams: string[]): Map<string, string> {
  const out = new Map<string, string>();
  let cur = 0;
  for (const p of parentParams) {
    let arg = ref.named.get(p);
    if (arg === undefined) {
      arg = ref.positional[cur];
      if (arg !== undefined) cur += 1;
    }
    if (arg !== undefined) out.set(p, arg);
  }
  return out;
}

/**
 * The subset of a trait's params used ONLY as a divert target
 * (`-> self.<param>`) — locally, or by forwarding into a parent trait param
 * that is itself divert-only. An arg bound to such a param must name a beat, so
 * a non-beat arg is diagnosable (`unresolvedTraitArg`). Params used as an event
 * (`on self.<param>`) or a plain value are excluded — those take any identifier.
 */
function divertOnlyParams(
  name: string,
  raw: Map<string, RawDecl>,
  cache: Map<string, Set<string>> = new Map(),
  visiting: Set<string> = new Set(),
): Set<string> {
  const done = cache.get(name);
  if (done !== undefined) return done;
  if (visiting.has(name)) return new Set();
  visiting.add(name);
  const decl = raw.get(name);
  const out = new Set<string>();
  if (decl !== undefined) {
    for (const p of decl.body.params) {
      const tok = new RegExp(`(?<![\\w.])self\\.${p}(?![\\w])`, "u");
      let total = 0;
      let nonDivert = 0;
      const check = (text: string, isDivert: boolean): void => {
        if (tok.test(text)) {
          total += 1;
          if (!isDivert) nonDivert += 1;
        }
      };
      const b = decl.body;
      for (const h of b.hooks) {
        check(h.event, false); // `on self.<param>` — an event, not a divert
        for (const l of h.body) check(l.text, l.text.trim().startsWith("->"));
      }
      for (const bt of b.beats) for (const l of bt.body) check(l.text, l.text.trim().startsWith("->"));
      for (const m of b.methods) {
        if (m.inlineExpr !== null) check(m.inlineExpr, false);
        for (const l of m.body) check(l.text, l.text.trim().startsWith("->"));
      }
      for (const [, v] of b.properties) check(v.value, false);
      // Forwarded: `p` passed as an arg to a parent trait's param.
      for (const entry of decl.inherits) {
        const ref = parseMixinRef(entry);
        const parent = raw.get(ref.name);
        if (parent === undefined) continue;
        const parentDivertOnly = divertOnlyParams(ref.name, raw, cache, visiting);
        for (const [pp, arg] of bindArgs(ref, parent.body.params)) {
          if (arg === p) {
            total += 1;
            if (!parentDivertOnly.has(pp)) nonDivert += 1;
          }
        }
      }
      if (total > 0 && nonDivert === 0) out.add(p);
    }
  }
  visiting.delete(name);
  cache.set(name, out);
  return out;
}

/**
 * Bind a parameterized trait's params to an application's args and rewrite
 * every `self.<param>` token in the (deep-cloned) parent body (spec §2.3).
 * Positional args bind by index, named args by name. An arg that is itself a
 * parameter of the *applier* — or names one of the applier's own beats —
 * forwards as `self.<arg>` (stays unbound for the next layer); any other arg
 * is concrete and consumes the `self.` prefix.
 */
function substituteParams(
  parentBody: CharacterBody,
  parentParams: string[],
  ref: MixinRef,
  applierParams: string[],
  ownedBeatNames: Set<string>,
  divertOnly: Set<string>,
  globalBeats: Set<string>,
  diagnostics: ProjectDiagnostic[],
  applierName: string,
  traitName: string,
): CharacterBody {
  const bindings = new Map<string, string>();
  let posCursor = 0;
  for (const param of parentParams) {
    let arg: string | undefined = ref.named.get(param);
    if (arg === undefined) {
      arg = ref.positional[posCursor];
      if (arg !== undefined) posCursor += 1;
    }
    if (arg === undefined) {
      diagnostics.push({
        kind: "requiredParamUnfilled",
        character: applierName,
        trait: traitName,
        param,
      });
      continue;
    }
    // Forwarding: an arg that names one of the applier's own params, or routes
    // to an owned beat, stays `self.<arg>` so a later layer can bind it;
    // otherwise it is concrete.
    const forward = applierParams.includes(arg) || ownedBeatNames.has(arg);
    if (!forward && divertOnly.has(param) && !globalBeats.has(arg)) {
      // The param is used only as `-> self.<param>`, so the arg must name a
      // beat — but it matches no global beat, no owned beat, and no forwarding
      // param. Almost certainly a typo or a literal placeholder word.
      diagnostics.push({ kind: "unresolvedTraitArg", character: applierName, trait: traitName, param, arg });
    }
    bindings.set(param, forward ? `self.${arg}` : arg);
  }
  if (bindings.size === 0) return parentBody;

  const body = deepCloneCharacterBody(parentBody);
  const subst = (text: string): string => {
    let out = text;
    for (const [param, value] of bindings) {
      // Hygienic: match the whole `self.<param>` token only — never a longer
      // dotted path (`myself.x`) or a partial identifier (`self.params`).
      out = out.replace(new RegExp(`(?<![\\w.])self\\.${param}(?![\\w])`, "gu"), value);
    }
    return out;
  };
  for (const h of body.hooks) {
    h.event = subst(h.event);
    for (const l of h.body) l.text = subst(l.text);
  }
  for (const m of body.methods) {
    if (m.inlineExpr !== null) m.inlineExpr = subst(m.inlineExpr);
    for (const l of m.body) l.text = subst(l.text);
  }
  for (const [, v] of body.properties) v.value = subst(v.value);
  for (const bt of body.beats) {
    for (const l of bt.body) l.text = subst(l.text);
  }
  return body;
}

/**
 * The transitive set of beat names a declaration owns — its own `beat`
 * blocks plus every one reachable through its `is` chain. A pure name walk
 * (no body merge) so it is stable regardless of merge order.
 */
function collectOwnedBeatNames(
  name: string,
  raw: Map<string, RawDecl>,
  seen: Set<string>,
): Set<string> {
  const out = new Set<string>();
  if (seen.has(name)) return out;
  seen.add(name);
  const decl = raw.get(name);
  if (decl === undefined) return out;
  for (const bt of decl.body.beats) out.add(bt.name);
  for (const entry of decl.inherits) {
    for (const n of collectOwnedBeatNames(parseMixinRef(entry).name, raw, seen)) out.add(n);
  }
  return out;
}

/** Whitespace-collapsing comparison key for hook event clauses. */
export function normaliseHookEvent(event: string): string {
  return event.split(/\s+/u).filter((s) => s.length > 0).join(" ");
}

/** Merge a `Property` list — parent fills gaps; child wins on name. */
export function mergeProperties(parent: Property[], child: Property[]): Property[] {
  const out: Property[] = [];
  for (const p of parent) {
    if (child.some((c) => c.name === p.name)) continue;
    out.push(p);
  }
  for (const p of child) out.push(p);
  return out;
}

function mergeItem(
  name: string,
  raw: Map<string, ItemBody>,
  cache: Map<string, ItemBody>,
): ItemBody {
  const cached = cache.get(name);
  if (cached !== undefined) return cached;
  const own = raw.get(name);
  if (own === undefined) return { inherits: [], properties: [] };
  let mergedProps: Property[] = [];
  for (const parent of own.inherits) {
    const parentBody = mergeItem(parent, raw, cache);
    mergedProps = mergeProperties(mergedProps, parentBody.properties);
  }
  mergedProps = mergeProperties(mergedProps, own.properties);
  const merged: ItemBody = { inherits: [...own.inherits], properties: mergedProps };
  cache.set(name, merged);
  return merged;
}

function mergeFaction(
  name: string,
  raw: Map<string, FactionBody>,
  cache: Map<string, FactionBody>,
): FactionBody {
  const cached = cache.get(name);
  if (cached !== undefined) return cached;
  const own = raw.get(name);
  if (own === undefined) return { inherits: [], properties: [] };
  let mergedProps: Property[] = [];
  for (const parent of own.inherits) {
    const parentBody = mergeFaction(parent, raw, cache);
    mergedProps = mergeProperties(mergedProps, parentBody.properties);
  }
  mergedProps = mergeProperties(mergedProps, own.properties);
  const merged: FactionBody = { inherits: [...own.inherits], properties: mergedProps };
  cache.set(name, merged);
  return merged;
}
