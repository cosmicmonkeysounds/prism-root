//! Project-wide story-graph extraction (the node-editor model).
//!
//! Lifts every open document in a [`Workspace`] plus the compiled
//! [`SimModel`] into one flat graph: beats (top-level, class-owned, and
//! trait-derived) as nodes, and every narrative transition — diverts,
//! choices, tunnels, `-> END`, and reactive hook routings — as edges,
//! with secondary relationship edges (cast / setting / faction / `is` /
//! owns / contains) for overlays. Cross-file structure is first-class:
//! a `-> lockdown` in `arrival.loom` resolves into `algorithm.loom`
//! exactly the way the sim's `resolveBeat` would route it at play time.
//!
//! Every edge carries a **source anchor** where one can be recovered:
//! the divert's span in the authored file plus the exact offset range of
//! the target text (`targetRange`) so a graph-side rewire is a pure
//! range splice. Class-owned beat bodies are lowered through the same
//! synthetic-beat re-parse the runtime uses; their anchors are remapped
//! line-by-line onto the original `RawLine` spans. Trait-derived beats
//! (param-substituted template instances) are honest about being
//! projections: they carry `structural: "derived"` and no edit anchors —
//! the editor routes edits to the template or the deriver's `fill`s.

import type {
  Beat,
  BodyItem,
  Declaration,
  DeclarationKind,
  DivertTarget,
  LoomFile,
  RawLine,
} from "../parser/ast.ts";
import { isRoleLike, parseMixinRef } from "../parser/index.ts";
import type { Span } from "../parser/source.ts";
import { lowerRawBody } from "../runtime/sim/effects.ts";
import type { Hook, SimModel } from "../runtime/sim/model.ts";

/** How much of a beat the structural edit API can safely rewrite. */
export type BeatStructural =
  /** A top-level `== name` file item — full structural edit surface. */
  | "file"
  /** A class-owned `beat name(…)` block — edges retarget, body is raw. */
  | "owned"
  /** A trait-shipped template instance — a projection; edit the template. */
  | "derived";

/** One beat node in the project graph, keyed like `SimModel.beats`. */
export interface GraphBeat {
  /** Global key — bare `lockdown` or owner-qualified `TheAdmin.tally`. */
  key: string;
  /** Bare beat name (the part after the owner qualifier, if any). */
  name: string;
  /** Owning CHARACTER for `Owner.name` beats, else null. */
  owner: string | null;
  params: string[];
  /** `cast:` contract members (top-level beats only). */
  cast: string[];
  /** `setting:` contract value, if any. */
  setting: string | null;
  /** Document that authored this beat (template file for derived). */
  uri: string | null;
  /** Span of the authored declaration in `uri`. */
  span: Span | null;
  structural: BeatStructural;
  /** True when this is the project's `entry:` beat. */
  entry: boolean;
  /**
   * True when another beat with the same global key exists earlier in
   * the project — this one is unreachable by name (last file wins in
   * the compiled model; earlier duplicates surface shadowed instead).
   */
  shadowed: boolean;
  /** Body statistics for the node card. */
  counts: { dialogues: number; choices: number; diverts: number };
  /** First few body lines, pre-rendered for the node card. */
  preview: string[];
  /** Contains a bare `<-` tunnel return. */
  tunnelReturn: boolean;
}

export type GraphEdgeKind =
  /** `-> target` (plain divert). */
  | "divert"
  /** A divert reached through a `*` / `+` choice option. */
  | "choice"
  /** `(target) ->` tunnel call. */
  | "tunnel"
  /** `-> END`. */
  | "end"
  /** A character hook body routing into a beat (`on scan guest` → `-> b`). */
  | "hook"
  // ---- secondary (relationship overlay) kinds ----
  | "cast"
  | "setting"
  | "member"
  | "contains"
  | "is"
  | "owns";

/** Exact offset range of a divert's target text, for rewiring. */
export interface TargetRange {
  uri: string;
  start: number;
  end: number;
}

export interface GraphEdge {
  id: string;
  /** Source node — a beat key or an entity id (`character:X` for hooks). */
  from: string;
  /** Resolved destination beat key / entity id; null when unresolved or END. */
  to: string | null;
  kind: GraphEdgeKind;
  /** True for story-flow kinds (divert/choice/tunnel/end/hook). */
  narrative: boolean;
  /** Choice text, or `on <event>` for hook edges. */
  label: string | null;
  /** Nearest guard context — `<if:>` condition, match arm, visit phase. */
  condition: string | null;
  /** Sticky (`+`) vs once-only (`*`) for choice edges, else null. */
  sticky: boolean | null;
  /** Raw target text when the divert resolves to no beat. */
  unresolved: string | null;
  /**
   * True when the target is `self.`-qualified in a context where the
   * binding is only known at play time (resolution shown is best-effort
   * via the beat's first cast member).
   */
  dynamic: boolean;
  /** Anchor of the divert (or hook/choice line) in authored source. */
  uri: string | null;
  span: Span | null;
  /** Exact target-text range for retargeting, when safely recoverable. */
  targetRange: TargetRange | null;
}

/** A non-beat declaration surfaced in the bin / as an overlay node. */
export interface GraphEntity {
  /** `<kind>:<name>` — stable node id, distinct from beat keys. */
  id: string;
  kind: DeclarationKind;
  name: string;
  uri: string;
  span: Span;
  /** `is X, Y` mixins (role-likes). */
  mixins: string[];
  /** Resolved faction id (characters), if any. */
  faction: string | null;
  /** Keys of the beats this entity owns (role-likes). */
  ownedBeats: string[];
  /** Count of reactive hooks that fire on this entity (role-likes). */
  hookCount: number;
}

/** Per-document grouping for the canvas's file containers. */
export interface GraphFile {
  uri: string;
  /** Beat keys authored in this document (incl. shadowed duplicates). */
  beats: string[];
  /** Entity ids declared in this document. */
  entities: string[];
}

/** The whole project as one graph. */
export interface StoryGraph {
  beats: Map<string, GraphBeat>;
  entities: Map<string, GraphEntity>;
  edges: GraphEdge[];
  files: GraphFile[];
  /** Resolved `entry:` beat key, if it names a real beat. */
  entry: string | null;
  /** True when any `-> END` exists (the canvas shows an END node). */
  hasEnd: boolean;
}

/** Stable entity node id. */
export function entityId(kind: DeclarationKind, name: string): string {
  return `${kind}:${name}`;
}

/** The minimal doc surface the builder reads (matches `Workspace.docs`). */
export interface GraphDoc {
  text: string;
  file: LoomFile;
}

/**
 * Build the project story graph from every open document plus the
 * compiled model. `model` may be stale relative to `docs` mid-keystroke
 * (the workspace keeps the last good compile); the builder tolerates
 * that by attributing from the docs first and only consulting the model
 * for merged artifacts (derived beats, inherited hooks, entry).
 */
export function buildStoryGraph(
  docs: ReadonlyMap<string, GraphDoc>,
  model: SimModel | null,
): StoryGraph {
  const b = new GraphBuilder(docs, model);
  return b.build();
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/** Guard-context stack entry pushed while walking a body tree. */
interface WalkCtx {
  from: string;
  uri: string | null;
  /** Map a lowered item's span to an authored anchor (null = anchorless). */
  anchor: (span: Span) => { uri: string; span: Span } | null;
  /** Owner for `self.` / `me.` resolution, if statically known. */
  owner: string | null;
  /** Best-effort `self` fallback (first cast member) — marks edges dynamic. */
  castFallback: string | null;
  choice: { text: string; sticky: boolean } | null;
  conditions: string[];
}

class GraphBuilder {
  private readonly out: StoryGraph = {
    beats: new Map(),
    entities: new Map(),
    edges: [],
    files: [],
    entry: null,
    hasEnd: false,
  };
  private edgeSeq = 0;
  /** Exact `uri@offset` → graph key mapping recorded when nodes register. */
  private readonly keyBySite = new Map<string, string>();
  /** `owner::event` pairs whose hooks were emitted from authored decls. */
  private readonly anchoredHooks = new Set<string>();
  /** `contains:` links waiting for the full entity registry. */
  private readonly pendingContains: Array<{
    from: string;
    inner: string;
    uri: string;
    span: Span;
  }> = [];
  /** Trait name → its declaration (for derived-beat attribution). */
  private readonly traitDecls = new Map<string, { uri: string; decl: Declaration }>();
  /** Character/role name → declaration (for mixin chains). */
  private readonly roleLikeDecls = new Map<string, { uri: string; decl: Declaration }>();

  private readonly docs: ReadonlyMap<string, GraphDoc>;
  private readonly model: SimModel | null;

  constructor(docs: ReadonlyMap<string, GraphDoc>, model: SimModel | null) {
    this.docs = docs;
    this.model = model;
  }

  build(): StoryGraph {
    // Pass 0 — declaration registry (trait/derived attribution needs it).
    for (const [uri, doc] of this.docs) {
      for (const item of doc.file.items) {
        if (item.kind !== "declaration") continue;
        const decl = item.value;
        if (decl.kind === "trait") this.traitDecls.set(decl.name, { uri, decl });
        if (isRoleLike(decl.kind)) this.roleLikeDecls.set(decl.name, { uri, decl });
      }
    }

    // Pass 1 — authored surface: beats, owned beats, entities, hooks.
    for (const [uri, doc] of this.docs) {
      const group: GraphFile = { uri, beats: [], entities: [] };
      this.out.files.push(group);
      for (const item of doc.file.items) {
        if (item.kind === "beat") this.addFileBeat(uri, item.value, group);
        else if (item.kind === "declaration") this.addDeclaration(uri, item.value, group);
      }
    }

    this.flushContains();

    // Pass 2 — merged artifacts from the compiled model.
    if (this.model !== null) {
      this.addDerivedBeats(this.model);
      this.addMergedHooks(this.model);
      this.addFactionMembership(this.model);
      const entry = this.model.entry;
      if (entry !== null && this.out.beats.has(entry)) {
        this.out.entry = entry;
        this.out.beats.get(entry)!.entry = true;
      }
    }

    // Pass 3 — edge extraction over every node with an authored body.
    for (const [uri, doc] of this.docs) {
      for (const item of doc.file.items) {
        if (item.kind === "beat") this.walkFileBeat(uri, doc, item.value);
        else if (item.kind === "declaration") this.walkDeclaration(uri, doc, item.value);
      }
    }
    if (this.model !== null) {
      this.walkDerivedBeats(this.model);
      this.walkMergedHooks(this.model);
    }

    this.out.hasEnd = this.out.edges.some((e) => e.kind === "end");
    return this.out;
  }

  // ------------------------------------------------------------------
  // Pass 1 — nodes
  // ------------------------------------------------------------------

  /** Register a top-level `== name` beat (deduping shadowed collisions). */
  private addFileBeat(uri: string, beat: Beat, group: GraphFile): void {
    const key = beat.name;
    const prev = this.out.beats.get(key);
    if (prev !== undefined && prev.owner === null) {
      // A same-named beat was seen earlier. The compiled model keeps the
      // *last* declaration (last `set` wins in file/item order), so
      // re-key the earlier entry as the shadowed duplicate and let this
      // one claim the bare name.
      const reKey = `${prev.name}~${this.shadowCount(prev.name) + 1}`;
      prev.key = reKey;
      prev.shadowed = true;
      this.out.beats.delete(key);
      this.out.beats.set(reKey, prev);
      if (prev.uri !== null && prev.span !== null) {
        this.keyBySite.set(site(prev.uri, prev.span), reKey);
      }
      const prevGroup = this.out.files.find((f) => f.uri === prev.uri);
      if (prevGroup) {
        const i = prevGroup.beats.indexOf(key);
        if (i >= 0) prevGroup.beats[i] = reKey;
      }
    }
    const cast = beat.contract.get("cast")?.value ?? null;
    const node: GraphBeat = {
      key,
      name: beat.name,
      owner: null,
      params: [...beat.params],
      cast: cast === null ? [] : cast.split(",").map((s) => s.trim()).filter((s) => s.length > 0),
      setting: beat.contract.get("setting")?.value ?? null,
      uri,
      span: beat.span,
      structural: "file",
      entry: false,
      shadowed: false,
      counts: countBody(beat.body),
      preview: previewBody(beat.body),
      tunnelReturn: hasTunnelReturn(beat.body),
    };
    this.out.beats.set(key, node);
    this.keyBySite.set(site(uri, beat.span), key);
    group.beats.push(key);
  }

  private shadowCount(name: string): number {
    let n = 0;
    for (const k of this.out.beats.keys()) if (k.startsWith(`${name}~`)) n += 1;
    return n;
  }

  /** Register a declaration entity + its own owned beats. */
  private addDeclaration(uri: string, decl: Declaration, group: GraphFile): void {
    const id = entityId(decl.kind, decl.name);
    this.out.entities.set(id, {
      id,
      kind: decl.kind,
      name: decl.name,
      uri,
      span: decl.span,
      mixins: [...decl.mixin],
      faction: null,
      ownedBeats: [],
      hookCount: 0,
    });
    group.entities.push(id);

    if (isRoleLike(decl.kind) && decl.character !== null) {
      const ent = this.out.entities.get(id)!;
      ent.hookCount = decl.character.hooks.filter((h) => !h.suppressed).length;
      for (const ob of decl.character.beats) {
        const key = `${decl.name}.${ob.name}`;
        const lowered = lowerRawBody(ob.body);
        this.out.beats.set(key, {
          key,
          name: ob.name,
          owner: decl.name,
          params: [...ob.params],
          cast: [],
          setting: null,
          uri,
          span: ob.span,
          structural: "owned",
          entry: false,
          shadowed: false,
          counts: countBody(lowered),
          preview: previewBody(lowered),
          tunnelReturn: hasTunnelReturn(lowered),
        });
        ent.ownedBeats.push(key);
        group.beats.push(key);
        this.edge({
          from: id,
          to: key,
          kind: "owns",
          narrative: false,
          uri,
          span: ob.span,
        });
      }
      // `is` edges to each mixin that names a known trait.
      for (const mixin of decl.mixin) {
        const ref = parseMixinRef(mixin).name;
        const trait = this.traitDecls.get(ref);
        if (trait) {
          this.edge({
            from: id,
            to: entityId("trait", ref),
            kind: "is",
            narrative: false,
            uri,
            span: decl.span,
          });
        }
      }
    }
    if (decl.kind === "location" && decl.location !== null) {
      for (const inner of decl.location.contains) {
        // Deferred — the inner LOCATION may be declared in a later file.
        this.pendingContains.push({ from: id, inner, uri, span: decl.span });
      }
    }
  }

  /** Emit `contains` edges once every entity is registered. */
  private flushContains(): void {
    for (const p of this.pendingContains) {
      const to = entityId("location", p.inner);
      if (!this.out.entities.has(to)) continue;
      this.edge({ from: p.from, to, kind: "contains", narrative: false, uri: p.uri, span: p.span });
    }
  }

  /**
   * Beats present in the compiled model but not authored directly on a
   * character — trait-shipped template instances (`Deriver.name`).
   * Attribute them to the shipping trait's file + template span.
   */
  private addDerivedBeats(model: SimModel): void {
    for (const [key, beat] of model.beats) {
      if (this.out.beats.has(key)) continue;
      const dot = key.indexOf(".");
      const owner = dot >= 0 ? key.slice(0, dot) : null;
      const name = dot >= 0 ? key.slice(dot + 1) : key;
      const template = owner !== null ? this.findShippedTemplate(owner, name) : null;
      this.out.beats.set(key, {
        key,
        name,
        owner,
        params: [...beat.params],
        cast: [],
        setting: null,
        uri: template?.uri ?? null,
        span: template?.span ?? null,
        structural: "derived",
        entry: false,
        shadowed: false,
        counts: countBody(beat.body),
        preview: previewBody(beat.body),
        tunnelReturn: hasTunnelReturn(beat.body),
      });
      if (owner !== null) {
        const ownerEnt = this.roleLikeEntity(owner);
        if (ownerEnt !== null) {
          ownerEnt.ownedBeats.push(key);
          this.edge({
            from: ownerEnt.id,
            to: key,
            kind: "owns",
            narrative: false,
            uri: template?.uri ?? null,
            span: template?.span ?? null,
          });
        }
      }
      const file = template !== null ? this.out.files.find((f) => f.uri === template.uri) : null;
      if (file) file.beats.push(key);
    }
  }

  /** Walk `owner`'s mixin chain for the trait that ships beat `name`. */
  private findShippedTemplate(
    owner: string,
    name: string,
  ): { uri: string; span: Span; lines: RawLine[] } | null {
    const seen = new Set<string>();
    const queue = [...(this.roleLikeDecls.get(owner)?.decl.mixin ?? [])];
    while (queue.length > 0) {
      const ref = parseMixinRef(queue.shift()!).name;
      if (seen.has(ref)) continue;
      seen.add(ref);
      const trait = this.traitDecls.get(ref);
      if (!trait) continue;
      const ob = trait.decl.character?.beats.find((x) => x.name === name);
      if (ob) return { uri: trait.uri, span: ob.span, lines: ob.body };
      queue.push(...trait.decl.mixin);
    }
    return null;
  }

  private roleLikeEntity(name: string): GraphEntity | null {
    return (
      this.out.entities.get(entityId("character", name)) ??
      this.out.entities.get(entityId("role", name)) ??
      null
    );
  }

  /** Seed `member` edges + faction ids from the compiled characters. */
  private addFactionMembership(model: SimModel): void {
    for (const [id, def] of model.characters) {
      const ent = this.roleLikeEntity(id);
      if (ent === null) continue;
      ent.faction = def.faction;
      if (def.faction !== null && this.out.entities.has(entityId("faction", def.faction))) {
        this.edge({
          from: ent.id,
          to: entityId("faction", def.faction),
          kind: "member",
          narrative: false,
          uri: ent.uri,
          span: ent.span,
        });
      }
    }
  }

  /** Bump hook counts for trait-inherited hooks the raw decl can't see. */
  private addMergedHooks(model: SimModel): void {
    for (const [id, def] of model.characters) {
      const ent = this.roleLikeEntity(id);
      if (ent !== null) ent.hookCount = Math.max(ent.hookCount, def.hooks.length);
    }
    for (const [id, def] of model.roles) {
      const ent = this.roleLikeEntity(id);
      if (ent !== null) ent.hookCount = Math.max(ent.hookCount, def.hooks.length);
    }
  }

  // ------------------------------------------------------------------
  // Pass 3 — edges
  // ------------------------------------------------------------------

  private walkFileBeat(uri: string, doc: GraphDoc, beat: Beat): void {
    const key = this.authoredKey(uri, beat);
    if (key === null) return;
    const node = this.out.beats.get(key)!;
    // cast / setting overlay edges.
    for (const member of node.cast) {
      const ent = this.roleLikeEntity(member);
      if (ent !== null) {
        this.edge({ from: key, to: ent.id, kind: "cast", narrative: false, uri, span: beat.span });
      }
    }
    if (node.setting !== null && this.out.entities.has(entityId("location", node.setting))) {
      this.edge({
        from: key,
        to: entityId("location", node.setting),
        kind: "setting",
        narrative: false,
        uri,
        span: beat.span,
      });
    }
    this.walkBody(beat.body, {
      from: key,
      uri,
      anchor: (span) => ({ uri, span }),
      owner: null,
      castFallback: node.cast[0] ?? null,
      choice: null,
      conditions: [],
    }, doc.text);
  }

  private walkDeclaration(uri: string, doc: GraphDoc, decl: Declaration): void {
    if (!isRoleLike(decl.kind) || decl.character === null) return;
    // Own owned beats — lower with a synthetic-line → RawLine remap.
    for (const ob of decl.character.beats) {
      const key = `${decl.name}.${ob.name}`;
      if (this.out.beats.get(key)?.structural !== "owned") continue;
      const lowered = lowerRawBody(ob.body);
      this.walkBody(lowered, {
        from: key,
        uri,
        anchor: rawLineAnchor(uri, ob.body),
        owner: decl.name,
        castFallback: null,
        choice: null,
        conditions: [],
      }, doc.text);
    }
    // Authored hooks — `on scan guest` bodies route into beats.
    const id = entityId(decl.kind, decl.name);
    for (const hook of decl.character.hooks) {
      if (hook.suppressed) continue;
      this.anchoredHooks.add(`${decl.name}::${hook.event}`);
      const lowered = lowerRawBody(hook.body);
      this.walkBody(lowered, {
        from: id,
        uri,
        anchor: rawLineAnchor(uri, hook.body),
        owner: decl.name,
        castFallback: null,
        choice: null,
        conditions: [],
      }, doc.text, { hookEvent: hook.event, hookSpan: hook.span });
    }
  }

  /** Derived beats: walk the model's filled body, anchorless. */
  private walkDerivedBeats(model: SimModel): void {
    for (const [key, beat] of model.beats) {
      const node = this.out.beats.get(key);
      if (node === undefined || node.structural !== "derived") continue;
      this.walkBody(beat.body, {
        from: key,
        uri: node.uri,
        anchor: () => null,
        owner: node.owner,
        castFallback: null,
        choice: null,
        conditions: [],
      }, null);
    }
  }

  /** Trait-inherited hooks the authored pass never saw — anchorless. */
  private walkMergedHooks(model: SimModel): void {
    const owners: Array<[string, { hooks: Hook[] }]> = [];
    for (const [id, def] of model.characters) owners.push([id, def]);
    for (const [id, def] of model.roles) owners.push([id, def]);
    for (const [id, def] of owners) {
      const ent = this.roleLikeEntity(id);
      if (ent === null) continue;
      for (const hook of def.hooks) {
        if (this.anchoredHooks.has(`${id}::${hook.event}`)) continue;
        this.walkBody(hook.body, {
          from: ent.id,
          uri: null,
          anchor: () => null,
          owner: id,
          castFallback: null,
          choice: null,
          conditions: [],
        }, null, { hookEvent: hook.event, hookSpan: null });
      }
    }
  }

  /** Resolve the graph key an authored top-level beat landed under. */
  private authoredKey(uri: string, beat: Beat): string | null {
    return this.keyBySite.get(site(uri, beat.span)) ?? null;
  }

  // ------------------------------------------------------------------
  // Body walker
  // ------------------------------------------------------------------

  private walkBody(
    items: BodyItem[],
    ctx: WalkCtx,
    docText: string | null,
    hook?: { hookEvent: string; hookSpan: Span | null },
  ): void {
    for (const item of items) {
      switch (item.kind) {
        case "divert": {
          const d = item.value;
          if (d.kind === "return") break;
          if (d.kind === "end") {
            this.emitNarrative(ctx, hook, {
              to: null,
              end: true,
              target: null,
              span: d.span,
              docText,
            });
            break;
          }
          this.emitNarrative(ctx, hook, {
            to: null,
            end: false,
            target: d.target,
            tunnel: d.kind === "tunnel",
            span: d.span,
            docText,
          });
          if (d.kind === "to") {
            for (const slotBody of d.slots.values()) this.walkBody(slotBody, ctx, docText, hook);
          }
          break;
        }
        case "choice":
          this.walkBody(item.value.body, {
            ...ctx,
            choice: { text: item.value.text, sticky: item.value.sticky },
          }, docText, hook);
          break;
        case "conditional":
          for (const arm of item.value.arms) {
            const cond = arm.condition === null ? "else" : `if ${arm.condition}`;
            this.walkBody(arm.body, { ...ctx, conditions: [...ctx.conditions, cond] }, docText, hook);
          }
          break;
        case "match":
          for (const arm of item.value.arms) {
            const cond = `${item.value.scrutinee} = ${arm.pattern}`;
            this.walkBody(arm.body, { ...ctx, conditions: [...ctx.conditions, cond] }, docText, hook);
          }
          break;
        case "eachVisit": {
          const phases: Array<[string, BodyItem[]]> = [
            ["first visit", item.value.first],
            ["later visits", item.value.then],
            ["final visits", item.value.finally],
          ];
          for (const [phase, body] of phases) {
            this.walkBody(body, { ...ctx, conditions: [...ctx.conditions, phase] }, docText, hook);
          }
          break;
        }
        case "afterMorph":
          this.walkBody(item.value.after, {
            ...ctx,
            conditions: [...ctx.conditions, `after ${item.value.condition}`],
          }, docText, hook);
          this.walkBody(item.value.otherwise, {
            ...ctx,
            conditions: [...ctx.conditions, "otherwise"],
          }, docText, hook);
          break;
        case "dialogue":
          this.walkBody(item.value.body, ctx, docText, hook);
          break;
        case "directiveBlock":
          this.walkBody(item.value.body, ctx, docText, hook);
          break;
        default:
          break;
      }
    }
  }

  /** Emit one narrative edge (divert / choice / tunnel / end / hook). */
  private emitNarrative(
    ctx: WalkCtx,
    hook: { hookEvent: string; hookSpan: Span | null } | undefined,
    d: {
      to: string | null;
      end: boolean;
      target: DivertTarget | null;
      tunnel?: boolean;
      span: Span;
      docText: string | null;
    },
  ): void {
    const anchor = ctx.anchor(d.span);
    let to: string | null = null;
    let unresolved: string | null = null;
    let dynamic = false;

    if (!d.end && d.target !== null) {
      const r = this.resolveTarget(d.target, ctx.owner, ctx.castFallback);
      to = r.key;
      dynamic = r.dynamic;
      if (to === null) unresolved = displayTarget(d.target);
    }

    let kind: GraphEdgeKind;
    if (d.end) kind = "end";
    else if (hook !== undefined) kind = "hook";
    else if (ctx.choice !== null) kind = "choice";
    else if (d.tunnel === true) kind = "tunnel";
    else kind = "divert";

    // Hook edges keep the choice text in the label too, if both apply.
    const label =
      hook !== undefined
        ? `on ${hook.hookEvent}`
        : ctx.choice !== null
          ? ctx.choice.text
          : null;

    let targetRange: TargetRange | null = null;
    if (anchor !== null && d.target !== null && d.docText !== null) {
      targetRange = findTargetRange(d.docText, anchor.uri, anchor.span, d.target);
    }

    this.edge({
      from: ctx.from,
      to,
      kind,
      narrative: true,
      label,
      condition: ctx.conditions.length > 0 ? ctx.conditions.join(" · ") : null,
      sticky: ctx.choice?.sticky ?? null,
      unresolved,
      dynamic,
      uri: anchor !== null ? anchor.uri : hook?.hookSpan != null ? ctx.uri : null,
      span: anchor !== null ? anchor.span : hook?.hookSpan ?? null,
      targetRange,
    });
  }

  /**
   * Static mirror of the sim's `resolveBeat` (spec §11.2): a qualifier
   * resolves `self`/`me` against the statically-known owner (or the
   * beat's first cast member as the runtime's SELF fallback), an
   * explicit `Owner.` is taken as-is, and every qualified miss falls
   * through to the flat global lookup — so `-> lockdown` routes the
   * same here as at play time.
   */
  private resolveTarget(
    t: DivertTarget,
    owner: string | null,
    castFallback: string | null,
  ): { key: string | null; dynamic: boolean } {
    const beats = this.out.beats;
    let dynamic = false;
    if (t.qualifier !== null) {
      let q: string | null = t.qualifier;
      if (q === "self" || q === "me") {
        q = owner;
        if (q === null) {
          q = castFallback;
          dynamic = true;
        }
      }
      if (q !== null) {
        const key = `${q}.${t.name}`;
        if (beats.has(key)) return { key, dynamic };
      } else {
        dynamic = true;
      }
    }
    if (beats.has(t.name)) return { key: t.name, dynamic };
    return { key: null, dynamic };
  }

  private edge(
    e: Partial<GraphEdge> & { from: string; to: string | null; kind: GraphEdgeKind; narrative: boolean },
  ): void {
    this.out.edges.push({
      id: `e${this.edgeSeq++}`,
      label: null,
      condition: null,
      sticky: null,
      unresolved: null,
      dynamic: false,
      uri: null,
      span: null,
      targetRange: null,
      ...e,
    });
  }
}

// ---------------------------------------------------------------------------
// Anchoring helpers
// ---------------------------------------------------------------------------

/** Exact declaration-site key — `uri@startOffset`. */
function site(uri: string, span: Span): string {
  return `${uri}@${span.start.offset}`;
}

/**
 * Anchor mapper for bodies lowered through `lowerRawBody`'s synthetic
 * `== __hook` beat: synthetic line `L` (0-based; line 0 is the opener)
 * is authored raw line `L - 1`, whose `RawLine.span` points at the real
 * document. Column information does not survive the dedent, so anchors
 * are line-grained.
 */
function rawLineAnchor(uri: string, lines: RawLine[]): (span: Span) => { uri: string; span: Span } | null {
  return (span) => {
    const idx = span.start.line - 1;
    const raw = lines[idx];
    if (raw === undefined) return null;
    return { uri, span: raw.span };
  };
}

/** Render a `DivertTarget` back to its written display form. */
export function displayTarget(t: DivertTarget): string {
  const base = t.qualifier === null ? t.name : `${t.qualifier}.${t.name}`;
  return t.knot === null ? base : `${base}#${t.knot}`;
}

/**
 * Locate the exact offset range of a divert's target text inside its
 * anchored line(s), trying each written form the parser accepts
 * (`q/name`, `q.name`, bare). Returns null when the source text no
 * longer contains the expected form (e.g. a param-substituted clone).
 */
export function findTargetRange(
  docText: string,
  uri: string,
  span: Span,
  target: DivertTarget,
): TargetRange | null {
  const from = Math.max(0, Math.min(span.start.offset, docText.length));
  const to = Math.max(from, Math.min(span.end.offset, docText.length));
  // Widen to whole lines so line-grained anchors still find the target.
  let ls = from;
  while (ls > 0 && docText[ls - 1] !== "\n") ls -= 1;
  let le = to;
  while (le < docText.length && docText[le] !== "\n") le += 1;
  const slice = docText.slice(ls, le);

  const names: string[] = [];
  if (target.qualifier !== null) {
    names.push(`${target.qualifier}/${target.name}`, `${target.qualifier}.${target.name}`);
  } else {
    names.push(target.name);
  }
  const candidates: string[] = [];
  for (const n of names) {
    if (target.knot !== null) candidates.push(`${n}#${target.knot}`);
    candidates.push(n);
  }
  for (const cand of candidates) {
    let at = slice.lastIndexOf(cand);
    while (at >= 0) {
      const before = at === 0 ? "" : slice[at - 1]!;
      const afterIdx = at + cand.length;
      const after = afterIdx >= slice.length ? "" : slice[afterIdx]!;
      const boundary = (c: string) => c === "" || !/[0-9A-Za-z_./#]/.test(c);
      if (boundary(before) && boundary(after)) {
        return { uri, start: ls + at, end: ls + at + cand.length };
      }
      at = slice.lastIndexOf(cand, at - 1);
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Node-card helpers
// ---------------------------------------------------------------------------

function countBody(items: BodyItem[]): GraphBeat["counts"] {
  const counts = { dialogues: 0, choices: 0, diverts: 0 };
  const walk = (body: BodyItem[]): void => {
    for (const item of body) {
      switch (item.kind) {
        case "dialogue":
          counts.dialogues += 1;
          walk(item.value.body);
          break;
        case "choice":
          counts.choices += 1;
          walk(item.value.body);
          break;
        case "divert":
          if (item.value.kind !== "return") counts.diverts += 1;
          break;
        case "conditional":
          for (const arm of item.value.arms) walk(arm.body);
          break;
        case "match":
          for (const arm of item.value.arms) walk(arm.body);
          break;
        case "eachVisit":
          walk(item.value.first);
          walk(item.value.then);
          walk(item.value.finally);
          break;
        case "afterMorph":
          walk(item.value.after);
          walk(item.value.otherwise);
          break;
        case "directiveBlock":
          walk(item.value.body);
          break;
        default:
          break;
      }
    }
  };
  walk(items);
  return counts;
}

const PREVIEW_LINES = 3;
const PREVIEW_WIDTH = 64;

/** First few body lines rendered for the node card. */
export function previewBody(items: BodyItem[]): string[] {
  const out: string[] = [];
  const push = (s: string): boolean => {
    const trimmed = s.trim();
    if (trimmed.length === 0) return out.length < PREVIEW_LINES;
    out.push(trimmed.length > PREVIEW_WIDTH ? `${trimmed.slice(0, PREVIEW_WIDTH - 1)}…` : trimmed);
    return out.length < PREVIEW_LINES;
  };
  const walk = (body: BodyItem[]): boolean => {
    for (const item of body) {
      switch (item.kind) {
        case "action":
          if (!push(item.value.value)) return false;
          break;
        case "sceneHeading":
          if (!push(item.value.value)) return false;
          break;
        case "dialogue": {
          const first = firstText(item.value.body);
          if (!push(first === null ? item.value.speaker : `${item.value.speaker}: ${first}`)) {
            return false;
          }
          break;
        }
        case "choice":
          if (!push(`${item.value.sticky ? "+" : "*"} ${item.value.text}`)) return false;
          break;
        case "divert": {
          const d = item.value;
          const text =
            d.kind === "end"
              ? "-> END"
              : d.kind === "return"
                ? "<-"
                : d.kind === "tunnel"
                  ? `(${displayTarget(d.target)}) ->`
                  : `-> ${displayTarget(d.target)}`;
          if (!push(text)) return false;
          break;
        }
        case "directive":
          if (!push(item.value.raw)) return false;
          break;
        case "conditional":
          for (const arm of item.value.arms) if (!walk(arm.body)) return false;
          break;
        default:
          break;
      }
    }
    return true;
  };
  walk(items);
  return out;
}

function firstText(items: BodyItem[]): string | null {
  for (const item of items) {
    if (item.kind === "action") return item.value.value;
  }
  return null;
}

function hasTunnelReturn(items: BodyItem[]): boolean {
  for (const item of items) {
    if (item.kind === "divert" && item.value.kind === "return") return true;
    if (item.kind === "conditional") {
      for (const arm of item.value.arms) if (hasTunnelReturn(arm.body)) return true;
    }
    if (item.kind === "choice" && hasTunnelReturn(item.value.body)) return true;
    if (item.kind === "dialogue" && hasTunnelReturn(item.value.body)) return true;
  }
  return false;
}
