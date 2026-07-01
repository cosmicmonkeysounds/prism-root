//! Lower a `Declaration`'s raw body into structured sub-ASTs for the
//! kinds that have grammar (CHARACTER / TRAIT → CharacterBody, STATS →
//! StatsBody, TREE → TreeBody, …). Spec §8–§13.
//!
//! The raw `RawLine[]` is kept verbatim on `decl.body`; structured
//! fields are additive.

import {
  emptyAxisDecl,
  emptyCharacterBody,
  emptyCohortBody,
  emptyGeneratorBody,
  emptyGeneratorDecl,
  emptyGoalDecl,
  emptyChannelBody,
  emptyLocationBody,
  emptyPersonBody,
  emptyPoolDecl,
  emptySpaceBody,
  emptyRosterBody,
  emptySceneBody,
  emptyStatsBody,
  emptyTreeNodeDecl,
  type AttributeDecl,
  type ChannelBody,
  type ChannelKindWord,
  type CharacterBody,
  type CohortBody,
  type ConstructorCall,
  type Declaration,
  type DispositionAxis,
  type FactionBody,
  type GeneratorBody,
  type GoalDecl,
  type InitDecl,
  type InitParam,
  type ItemBody,
  type KnowledgeField,
  type LocationBody,
  type MethodDecl,
  type OwnedBeat,
  type PersonBody,
  type Property,
  type RawLine,
  type ReactClause,
  type RosterAssignment,
  type RosterBody,
  type RosterCastEntry,
  type RosterCohortEntry,
  type RosterLocationEntry,
  type RosterSwingEntry,
  type SceneBody,
  type SceneState,
  type SlotType,
  type SpaceBody,
  type StatsBody,
  type TreeBody,
} from "./ast.ts";
import { Code, errorDiagnostic, type Diagnostic } from "./diagnostics.ts";
import { span, type Span } from "./source.ts";
import {
  allChars,
  anyChar,
  isAsciiAlphabetic,
  isAsciiAlphanumeric,
  isAsciiUppercase,
  parseF64,
  parseU32,
  splitOnce,
  splitTopLevelCommas,
  stripPrefix,
  stripSuffix,
  trimStartMatches,
} from "./rust.ts";

/** Attach structured bodies to `decl` based on its kind. */
export function lower(decl: Declaration, diagnostics: Diagnostic[]): void {
  switch (decl.kind) {
    case "character":
    case "role":
      decl.character = lowerCharacter(decl.body, diagnostics);
      break;
    case "trait": {
      // `TRAIT Scanner(beat)` — the name carries a parameter list, split off
      // exactly like SCENE (spec §2.1). The params ride on the CharacterBody.
      const [name, params] = splitNameAndParams(decl.name);
      decl.name = name;
      decl.character = lowerCharacter(decl.body, diagnostics);
      decl.character.params = params;
      break;
    }
    case "stats":
      decl.stats = lowerStats(decl.body, diagnostics);
      break;
    case "tree":
      decl.tree = lowerTree(decl.body, diagnostics);
      break;
    case "scene": {
      const [name, params] = splitNameAndParams(decl.name);
      decl.name = name;
      decl.scene = lowerScene(params, decl.body, diagnostics);
      break;
    }
    case "generator":
      decl.generator = lowerGenerator(decl.body, diagnostics, decl.span);
      break;
    case "cohort":
      decl.cohort = lowerCohort(decl.name, decl.body, decl.span, diagnostics);
      break;
    case "location":
      decl.location = lowerLocation(decl.body);
      break;
    case "item": {
      const item: ItemBody = {
        inherits: [...decl.mixin],
        properties: lowerTypedProperties(decl.body),
      };
      decl.item = item;
      break;
    }
    case "faction": {
      const faction: FactionBody = {
        inherits: [...decl.mixin],
        properties: lowerTypedProperties(decl.body),
      };
      decl.faction = faction;
      break;
    }
    case "person":
      decl.person = lowerPerson(decl.body);
      break;
    case "roster":
      decl.roster = lowerRoster(decl.body);
      break;
    case "space":
      decl.space = lowerSpace(decl.body);
      break;
    case "channel":
      decl.channel = lowerChannel(decl.name, decl.body, decl.span);
      break;
  }
}

// ---------------------------------------------------------------------
// SPACE / CHANNEL (authored chatrooms)
// ---------------------------------------------------------------------

const CHANNEL_KINDS: ReadonlyArray<ChannelKindWord> = ["open", "private", "faction", "group", "dm"];

function asChannelKind(value: string): ChannelKindWord {
  const v = value.trim().toLowerCase();
  return (CHANNEL_KINDS as readonly string[]).includes(v) ? (v as ChannelKindWord) : "open";
}

function splitList(value: string): string[] {
  return value
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** Apply a `key: value` line to a channel body. Shared by nested + standalone. */
function applyChannelProp(out: ChannelBody, key: string, value: string, sp: Span): void {
  if (key === "label") out.label = value;
  else if (key === "kind") out.kind = asChannelKind(value);
  else if (key === "space") out.space = value;
  else if (key === "faction") out.faction = value;
  else if (key === "members") out.members = splitList(value);
  else if (key === "invite") out.invite = value;
  else if (key === "type") out.type = value;
  else if (key === "post") out.post = value;
  else if (key === "threads") out.threads = value;
  else if (key === "routes") out.routes = value;
  else if (key === "slow") out.slow = value;
  else if (key === "ephemeral") out.ephemeral = value;
  out.properties.set(key, { value, span: sp });
}

/** Lower a standalone `CHANNEL <name>` body (flat `key: value` lines). */
function lowerChannel(name: string, body: RawLine[], sp: Span): ChannelBody {
  const out = emptyChannelBody(name.trim(), sp);
  for (const line of body) {
    const kv = splitProperty(line.text.trim());
    if (kv !== null) applyChannelProp(out, kv[0], kv[1], line.span);
  }
  return out;
}

/**
 * Lower a `SPACE` body: its own `label:` / properties, plus nested
 * `CHANNEL <name>` sub-blocks (their indented lines become each channel's
 * body). Mirrors the SCENE-state collection pattern.
 */
function lowerSpace(body: RawLine[]): SpaceBody {
  const out = emptySpaceBody();
  const baseIndent = body.length > 0 ? body[0]!.indent : 0;
  let i = 0;
  while (i < body.length) {
    const line = body[i]!;
    if (line.indent > baseIndent) {
      i += 1;
      continue;
    }
    const text = line.text.trim();
    const opener = stripPrefix(text, "CHANNEL ");
    if (opener !== null) {
      const chName = opener.trim();
      const sub: RawLine[] = [];
      i += 1;
      while (i < body.length && body[i]!.indent > line.indent) {
        sub.push(body[i]!);
        i += 1;
      }
      out.channels.push(lowerChannel(chName, sub, line.span));
      continue;
    }
    const kv = splitProperty(text);
    if (kv !== null) {
      if (kv[0] === "label") out.label = kv[1];
      out.properties.set(kv[0], { value: kv[1], span: line.span });
    }
    i += 1;
  }
  return out;
}

// ---------------------------------------------------------------------
// Typed slots (spec §8)
// ---------------------------------------------------------------------

function lowerTypedProperties(body: RawLine[]): Property[] {
  const baseIndent = body.length > 0 ? body[0]!.indent : 0;
  const out: Property[] = [];
  for (const line of body) {
    if (line.indent !== baseIndent) continue;
    const text = line.text.trim();
    const prop = parseTypedProperty(text, line.span);
    if (prop !== null) out.push(prop);
  }
  return out;
}

export function parseTypedProperty(text: string, sp: Span): Property | null {
  const colon = text.indexOf(":");
  if (colon < 0) return null;
  const name = text.slice(0, colon).trim();
  if (
    name.length === 0 ||
    !allChars(name, (c) => isAsciiAlphanumeric(c) || c === "_" || c === "-")
  ) {
    return null;
  }
  const rest = text.slice(colon + 1).trim();
  const split = splitTypeAndDefault(rest);
  const typePart = split !== null ? split[0] : rest;
  const defaultPart = split !== null ? split[1] : null;
  const slotType = parseSlotType(typePart, defaultPart);
  return {
    name,
    slotType,
    default: defaultPart,
    rawType: typePart.length === 0 ? null : typePart,
    span: sp,
  };
}

function splitTypeAndDefault(rest: string): [string, string] | null {
  let depth = 0;
  let lastEq: number | null = null;
  for (let i = 0; i < rest.length; i++) {
    const ch = rest[i]!;
    if (ch === "(" || ch === "[" || ch === "{") depth += 1;
    else if (ch === ")" || ch === "]" || ch === "}") depth -= 1;
    else if (ch === "=" && depth === 0) {
      if (rest[i + 1] === "=" || (i > 0 && rest[i - 1] === "=")) continue;
      lastEq = i;
    }
  }
  if (lastEq === null) return null;
  return [rest.slice(0, lastEq).trim(), rest.slice(lastEq + 1).trim()];
}

export function parseSlotType(raw: string, defaultText: string | null): SlotType | null {
  raw = raw.trim();
  if (raw.length === 0) return null;
  // `text?` — optional wrapper.
  {
    const inner = stripSuffix(raw, "?");
    if (inner !== null) {
      const parsed = parseSlotType(inner.trim(), null);
      if (parsed === null) return null;
      return { kind: "optional", inner: parsed };
    }
  }
  // `list of X` / `map of K to V`.
  {
    const rest = stripPrefix(raw, "list of ");
    if (rest !== null) {
      const inner =
        parseSlotType(rest.trim(), null) ?? ({ kind: "concrete", name: rest.trim() } as SlotType);
      return { kind: "listOf", inner };
    }
  }
  {
    const rest = stripPrefix(raw, "map of ");
    if (rest !== null) {
      const kv = splitOnce(rest, " to ");
      if (kv !== null) {
        const key =
          parseSlotType(kv[0].trim(), null) ??
          ({ kind: "concrete", name: kv[0].trim() } as SlotType);
        const value =
          parseSlotType(kv[1].trim(), null) ??
          ({ kind: "concrete", name: kv[1].trim() } as SlotType);
        return { kind: "mapOf", key, value };
      }
    }
  }
  // `any` / `any of LOCATION`.
  if (raw === "any") {
    return { kind: "any" };
  }
  {
    const rest = stripPrefix(raw, "any of ");
    if (rest !== null) {
      return { kind: "anyOf", name: rest.trim() };
    }
  }
  // `range LO to HI` or bare `LO to HI`.
  {
    const rangeBody = stripPrefix(raw, "range ") ?? raw;
    const lh = splitOnce(rangeBody, " to ");
    if (lh !== null) {
      const lo = parseF64(lh[0].trim());
      const hi = parseF64(lh[1].trim());
      if (lo !== null && hi !== null) {
        const def = defaultText !== null ? parseF64(defaultText.trim()) : null;
        return { kind: "range", lo, hi, default: def };
      }
    }
  }
  // Sum: `a | b | c` (at least two pipes).
  if (raw.includes("|")) {
    const parts = raw
      .split("|")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (parts.length >= 2) {
      return { kind: "sum", variants: parts };
    }
  }
  // Bare type name — must look like an identifier.
  if (allChars(raw, (c) => isAsciiAlphanumeric(c) || c === "_" || c === "-")) {
    return { kind: "concrete", name: raw };
  }
  return null;
}

// ---------------------------------------------------------------------
// COHORT / LOCATION (spec §13.1)
// ---------------------------------------------------------------------

function lowerCohort(
  name: string,
  body: RawLine[],
  declSpan: Span,
  diagnostics: Diagnostic[],
): CohortBody {
  const out = emptyCohortBody();
  for (const line of body) {
    const text = line.text.trim();
    const kv = splitProperty(text);
    if (kv !== null) {
      const [key, value] = kv;
      if (key === "label") out.label = value;
      else if (key === "capacity") out.capacity = parseU32(value);
      out.properties.set(key, { value, span: line.span });
    }
  }
  if (out.capacity === null) {
    diagnostics.push(
      errorDiagnostic(
        Code.L1142CohortNoCapacity,
        declSpan,
        `\`COHORT ${name}\` is missing a \`capacity:\` field`,
      ),
    );
  }
  return out;
}

function lowerLocation(body: RawLine[]): LocationBody {
  const out = emptyLocationBody();
  for (const line of body) {
    const text = line.text.trim();
    const kv = splitProperty(text);
    if (kv !== null) {
      const [key, value] = kv;
      if (key === "label") out.label = value;
      else if (key === "ambient") out.ambient = value;
      else if (key === "capacity") out.capacity = parseU32(value);
      else if (key === "contains") {
        out.contains = value
          .split(",")
          .map((s) => s.trim())
          .filter((s) => s.length > 0);
      }
      out.properties.set(key, { value, span: line.span });
    }
  }
  return out;
}

function splitNameAndParams(raw: string): [string, string[]] {
  raw = raw.trim();
  const open = raw.indexOf("(");
  if (open >= 0) {
    const head = raw.slice(0, open).trim();
    const tail = raw.slice(open + 1);
    const close = tail.lastIndexOf(")");
    const inner = close >= 0 ? tail.slice(0, close) : tail;
    const params = inner
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    return [head, params];
  }
  return [raw, []];
}

// ---------------------------------------------------------------------
// SCENE
// ---------------------------------------------------------------------

function lowerScene(params: string[], body: RawLine[], diagnostics: Diagnostic[]): SceneBody {
  const out = emptySceneBody(params);
  const baseIndent = body.length > 0 ? body[0]!.indent : 0;

  let i = 0;
  let sawState = false;
  while (i < body.length) {
    const line = body[i]!;
    if (line.indent > baseIndent) {
      if (!sawState) out.entry.push(line);
      i += 1;
      continue;
    }
    const text = line.text.trim();

    {
      const t = stripPrefix(text, "tier:");
      if (t !== null) {
        out.tier = t.trim();
        i += 1;
        continue;
      }
    }
    {
      const p = stripPrefix(text, "priority:");
      if (p !== null) {
        out.priority = parseF64(p.trim());
        i += 1;
        continue;
      }
    }

    const next = body[i + 1];
    if (isStateLabel(text) && next !== undefined && next.indent > line.indent) {
      sawState = true;
      const name = text;
      if (name.length === 0) {
        diagnostics.push(
          errorDiagnostic(
            Code.L1130SceneStateUnnamed,
            line.span,
            "scene state opener is missing a name",
          ),
        );
      }
      const state: SceneState = { name, body: [], span: line.span };
      i += 1;
      while (i < body.length && body[i]!.indent > line.indent) {
        state.body.push(body[i]!);
        state.span = span(state.span.start, body[i]!.span.end);
        i += 1;
      }
      out.states.push(state);
      continue;
    }

    out.entry.push(line);
    i += 1;
  }
  return out;
}

function isStateLabel(text: string): boolean {
  if (text.length === 0) return false;
  if (anyChar(text, (c) => !(isAsciiAlphanumeric(c) || c === "_"))) return false;
  const first = text[0]!;
  return isAsciiAlphabetic(first) || first === "_";
}

// ---------------------------------------------------------------------
// GENERATOR (top-level)
// ---------------------------------------------------------------------

function lowerGenerator(
  body: RawLine[],
  diagnostics: Diagnostic[],
  declSpan: Span,
): GeneratorBody {
  const out = emptyGeneratorBody();
  const baseIndent = body.length > 0 ? body[0]!.indent : 0;
  let i = 0;
  while (i < body.length) {
    const line = body[i]!;
    if (line.indent > baseIndent) {
      out.body.push(line);
      i += 1;
      continue;
    }
    const text = line.text.trim();
    {
      const t = stripPrefix(text, "tier:");
      if (t !== null) {
        out.tier = t.trim();
        i += 1;
        continue;
      }
    }
    {
      const p = stripPrefix(text, "priority:");
      if (p !== null) {
        out.priority = parseF64(p.trim());
        i += 1;
        continue;
      }
    }
    {
      const rest = stripPrefix(text, "on ");
      if (rest !== null) {
        out.startWhen = rest.trim();
        i += 1;
        while (i < body.length && body[i]!.indent > baseIndent) {
          out.body.push(body[i]!);
          i += 1;
        }
        continue;
      }
    }
    out.body.push(line);
    i += 1;
  }
  if (out.body.length === 0) {
    diagnostics.push(
      errorDiagnostic(Code.L1131GeneratorMissingBody, declSpan, "generator declaration has no body"),
    );
  }
  return out;
}

// ---------------------------------------------------------------------
// CHARACTER / TRAIT
// ---------------------------------------------------------------------

function lowerCharacter(body: RawLine[], diagnostics: Diagnostic[]): CharacterBody {
  const out = emptyCharacterBody();
  const baseIndent = body.length > 0 ? body[0]!.indent : 0;

  let i = 0;
  while (i < body.length) {
    const line = body[i]!;
    if (line.indent > baseIndent) {
      i += 1;
      continue;
    }
    const text = line.text.trim();

    // `knows:` opens a typed-slot block.
    if (text === "knows:" || text === "knows") {
      i += 1;
      while (i < body.length && body[i]!.indent > baseIndent) {
        const field = parseKnowledgeField(body[i]!, diagnostics);
        if (field !== null) out.knowledge.push(field);
        i += 1;
      }
      continue;
    }

    // `goal name`.
    {
      const rest = stripPrefix(text, "goal ");
      if (rest !== null) {
        const name = rest.trim();
        if (name.length === 0) {
          diagnostics.push(
            errorDiagnostic(Code.L1104UnnamedGoal, line.span, "`goal` declaration is missing a name"),
          );
          i += 1;
          continue;
        }
        const goal = emptyGoalDecl(name, line.span);
        i += 1;
        while (i < body.length && body[i]!.indent > baseIndent) {
          fillGoalField(goal, body[i]!.text);
          goal.span = span(goal.span.start, body[i]!.span.end);
          i += 1;
        }
        out.goals.push(goal);
        continue;
      }
    }

    // `on <event>` hook (with `: none` suppression, spec §9.5).
    {
      const rest0 = stripPrefix(text, "on ");
      if (rest0 !== null) {
        let rest = rest0.trim();
        // Inline opener divert: `on scan guest -> beat` (spec §2.6). Split a
        // trailing `-> target` off the opener into a synthetic body divert.
        let inlineDivert: string | null = null;
        {
          const arrow = rest.indexOf("->");
          if (arrow >= 0) {
            const target = rest.slice(arrow + 2).trim();
            rest = rest.slice(0, arrow).trim();
            if (target.length > 0) inlineDivert = target;
          }
        }
        const stripped = stripSuppression(rest);
        const eventText = stripped !== null ? stripped : rest;
        const suppressed = stripped !== null;
        const spanStart = line.span.start;
        const hook = { event: eventText, body: [] as RawLine[], suppressed, span: line.span };
        if (inlineDivert !== null && !suppressed) {
          hook.body.push({
            indent: baseIndent + 2,
            text: `-> ${inlineDivert}`,
            span: line.span,
          });
        }
        i += 1;
        while (i < body.length && body[i]!.indent > baseIndent) {
          if (!suppressed) hook.body.push(body[i]!);
          hook.span = span(spanStart, body[i]!.span.end);
          i += 1;
        }
        out.hooks.push(hook);
        continue;
      }
    }

    // `init(args)` constructor (spec v3 §9.6).
    if (isInitOpener(text)) {
      const [params] = parseInitSignature(text);
      const init: InitDecl = { params, body: [], span: line.span };
      i += 1;
      while (i < body.length && body[i]!.indent > baseIndent) {
        init.body.push(body[i]!);
        init.span = span(init.span.start, body[i]!.span.end);
        i += 1;
      }
      out.init = init;
      continue;
    }

    // `method name(args)` (spec v3 §9.6).
    {
      const rest = stripPrefix(text, "method ");
      if (rest !== null) {
        const method = parseMethodOpener(rest, line.span);
        if (method !== null) {
          if (method.inlineExpr === null) {
            i += 1;
            while (i < body.length && body[i]!.indent > baseIndent) {
              method.body.push(body[i]!);
              method.span = span(method.span.start, body[i]!.span.end);
              i += 1;
            }
          } else {
            i += 1;
          }
          out.methods.push(method);
          continue;
        }
        i += 1;
        continue;
      }
    }

    // `generator name`.
    {
      const rest = stripPrefix(text, "generator ");
      if (rest !== null) {
        const gen = emptyGeneratorDecl(rest.trim(), line.span);
        i += 1;
        while (i < body.length && body[i]!.indent > baseIndent) {
          const inner = body[i]!.text.trim();
          const t = stripPrefix(inner, "tier:");
          const p = stripPrefix(inner, "priority:");
          if (t !== null) {
            gen.tier = t.trim();
          } else if (p !== null) {
            gen.priority = parseF64(p.trim());
          } else {
            gen.body.push(body[i]!);
          }
          gen.span = span(gen.span.start, body[i]!.span.end);
          i += 1;
        }
        out.generators.push(gen);
        continue;
      }
    }

    // `beat name(params)` — a class-owned beat block (spec §11.1),
    // collected verbatim exactly like the generator block above.
    {
      const rest = stripPrefix(text, "beat ");
      if (rest !== null) {
        const [bname, bparams] = splitNameAndParams(rest.trim());
        const beat: OwnedBeat = { name: bname, params: bparams, body: [], span: line.span };
        i += 1;
        while (i < body.length && body[i]!.indent > baseIndent) {
          beat.body.push(body[i]!);
          beat.span = span(beat.span.start, body[i]!.span.end);
          i += 1;
        }
        out.beats.push(beat);
        continue;
      }
    }

    // `fill name` — content for a `slot: name` hole in a derived beat template
    // (spec §11.3). Pure body lines, collected verbatim like a beat block.
    {
      const rest = stripPrefix(text, "fill ");
      if (rest !== null) {
        const fname = rest.trim();
        const lines: RawLine[] = [];
        i += 1;
        while (i < body.length && body[i]!.indent > baseIndent) {
          lines.push(body[i]!);
          i += 1;
        }
        if (fname.length > 0) out.fills.set(fname, lines);
        continue;
      }
    }

    // `reacts <cond> -> <tag>`.
    {
      const rest = stripPrefix(text, "reacts ");
      if (rest !== null) {
        const react = parseReactClause(rest, line.span);
        if (react !== null) {
          out.reacts.push(react);
        } else {
          diagnostics.push(
            errorDiagnostic(
              Code.L1102MalformedReactClause,
              line.span,
              "`reacts` clause needs `<condition> → <tag>` (or `-> tag`)",
            ),
          );
        }
        i += 1;
        continue;
      }
    }

    // Disposition: `trusts X: N of M [mirror …]`.
    {
      const dv = stripDispositionVerb(text);
      if (dv !== null) {
        const [verb, rest] = dv;
        const res = parseDisposition(verb, rest, line.span);
        if (res.ok) {
          out.disposition.push(res.value);
        } else {
          diagnostics.push(errorDiagnostic(res.code, line.span, codeMessage(res.code)));
        }
        i += 1;
        continue;
      }
    }

    // Plain `key: value` property with stats special-cases.
    {
      const kv = splitProperty(text);
      if (kv !== null) {
        const [key, value] = kv;
        if (key === "stats") {
          const call = parseConstructorCall(value, line.span);
          if (call !== null) {
            out.statsProfile = call.class;
            out.statsCtor = call;
            i += 1;
            continue;
          }
          // Bare `stats: Combat` — possibly followed by indented block sugar.
          out.statsProfile = value;
          const ctor: ConstructorCall = {
            class: value,
            args: new Map<string, string>(),
            span: line.span,
          };
          i += 1;
          while (i < body.length && body[i]!.indent > baseIndent) {
            const inner = body[i]!.text.trim();
            const innerKv = splitProperty(inner);
            if (innerKv !== null) {
              ctor.args.set(innerKv[0], innerKv[1]);
              ctor.span = span(ctor.span.start, body[i]!.span.end);
            }
            i += 1;
          }
          if (ctor.args.size > 0) out.statsCtor = ctor;
          continue;
        }
        // Dotted override `stats.<field>: <value>`.
        const field = stripPrefix(key, "stats.");
        if (field !== null) {
          let entry = out.statsCtor;
          if (entry === null) {
            entry = {
              class: out.statsProfile ?? "",
              args: new Map<string, string>(),
              span: line.span,
            };
            out.statsCtor = entry;
          }
          entry.args.set(field, value);
          entry.span = span(entry.span.start, line.span.end);
          out.properties.set(key, { value, span: line.span });
          i += 1;
          continue;
        }
        out.properties.set(key, { value, span: line.span });
        const prop = parseTypedProperty(text, line.span);
        if (prop !== null) out.typedProperties.push(prop);
        i += 1;
        continue;
      }
    }

    i += 1;
  }
  return out;
}

// ---------------------------------------------------------------------
// Class layer (spec v3 §9.6)
// ---------------------------------------------------------------------

function isInitOpener(text: string): boolean {
  return text === "init" || text.startsWith("init(") || text.startsWith("init ");
}

function parseInitSignature(text: string): [InitParam[], string | null] {
  const rest = trimStartMatches(text, "init").trim();
  if (rest.length === 0) return [[], null];
  const stripped = stripPrefix(rest, "(");
  if (stripped !== null) {
    const end = stripped.lastIndexOf(")");
    if (end >= 0) {
      return [parseParamList(stripped.slice(0, end)), null];
    }
  }
  return [[], null];
}

function parseParamList(inner: string): InitParam[] {
  const params: InitParam[] = [];
  for (let raw of splitTopLevelCommas(inner)) {
    raw = raw.trim();
    if (raw.length === 0) continue;
    const eqSplit = splitOnce(raw, "=");
    const head = eqSplit !== null ? eqSplit[0].trim() : raw;
    const def = eqSplit !== null ? eqSplit[1].trim() : null;
    const colonSplit = splitOnce(head, ":");
    const name = colonSplit !== null ? colonSplit[0].trim() : head;
    const rawType = colonSplit !== null ? colonSplit[1].trim() : null;
    if (name.length === 0) continue;
    params.push({ name, rawType, default: def });
  }
  return params;
}

function parseMethodOpener(rest: string, sp: Span): MethodDecl | null {
  rest = rest.trim();
  if (rest.length === 0) return null;
  const eqSplit = splitOnce(rest, "=");
  if (eqSplit !== null) {
    const head = eqSplit[0].trim();
    const [name, params] = parseMethodHead(head);
    if (name.length === 0) return null;
    return { name, params, inlineExpr: eqSplit[1].trim(), body: [], span: sp };
  }
  const [name, params] = parseMethodHead(rest);
  if (name.length === 0) return null;
  return { name, params, inlineExpr: null, body: [], span: sp };
}

function parseMethodHead(text: string): [string, InitParam[]] {
  const open = text.indexOf("(");
  if (open >= 0) {
    const head = text.slice(0, open).trim();
    const tail = text.slice(open + 1);
    const close = tail.lastIndexOf(")");
    const params = parseParamList(close >= 0 ? tail.slice(0, close) : tail);
    return [head, params];
  }
  return [text.trim(), []];
}

export function parseConstructorCall(text: string, sp: Span): ConstructorCall | null {
  text = text.trim();
  const open = text.indexOf("(");
  if (open < 0) return null;
  const cls = text.slice(0, open).trim();
  if (cls.length === 0) return null;
  if (!allChars(cls, (c) => isAsciiAlphanumeric(c) || c === "_")) return null;
  const first = cls[0]!;
  if (!(isAsciiUppercase(first) || first === "_")) return null;
  const tail = text.slice(open + 1);
  const close = tail.lastIndexOf(")");
  if (close < 0) return null;
  const inner = tail.slice(0, close);
  const args = new Map<string, string>();
  for (let raw of splitTopLevelCommas(inner)) {
    raw = raw.trim();
    if (raw.length === 0) continue;
    const kv = splitOnce(raw, ":");
    if (kv !== null) {
      args.set(kv[0].trim(), kv[1].trim());
    }
  }
  return { class: cls, args, span: sp };
}

/**
 * A parsed `is`-clause entry (spec §2.2). A bare name (`Algo`) has empty
 * args; a call keeps BOTH positional (`Scanner(crawler_report)`) and named
 * (`CellWatch(loc: Internet, signal: lockdown)`) arguments — unlike
 * `parseConstructorCall`, which drops colon-less args.
 */
export interface MixinRef {
  name: string;
  positional: string[];
  named: Map<string, string>;
}

export function parseMixinRef(entry: string): MixinRef {
  entry = entry.trim();
  const open = entry.indexOf("(");
  if (open < 0) return { name: entry, positional: [], named: new Map() };
  const name = entry.slice(0, open).trim();
  const close = entry.lastIndexOf(")");
  const inner = close > open ? entry.slice(open + 1, close) : entry.slice(open + 1);
  const positional: string[] = [];
  const named = new Map<string, string>();
  for (let raw of splitTopLevelCommas(inner)) {
    raw = raw.trim();
    if (raw.length === 0) continue;
    const colon = topLevelColonIndex(raw);
    if (colon >= 0) {
      named.set(raw.slice(0, colon).trim(), raw.slice(colon + 1).trim());
    } else {
      // Value kept whole so a multi-word arg (`enters Internet`) survives.
      positional.push(raw);
    }
  }
  return { name, positional, named };
}

/** Index of the first `:` at bracket depth 0, or -1. */
function topLevelColonIndex(text: string): number {
  let depth = 0;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i]!;
    if (ch === "(" || ch === "[" || ch === "{") depth += 1;
    else if (ch === ")" || ch === "]" || ch === "}") depth -= 1;
    else if (ch === ":" && depth === 0) return i;
  }
  return -1;
}


/** Detect the `: none` suppression suffix on a hook event clause. */
function stripSuppression(text: string): string | null {
  const trimmed = text.replace(/\s+$/u, "");
  const stripped = stripSuffix(trimmed, "none");
  if (stripped === null) return null;
  const head = stripped.replace(/\s+$/u, "");
  const head2 = stripSuffix(head, ":");
  if (head2 === null) return null;
  return head2.replace(/\s+$/u, "");
}

function stripDispositionVerb(text: string): [string, string] | null {
  for (const verb of ["trusts", "respects", "fears"]) {
    const rest = stripPrefix(text, verb);
    if (rest !== null && rest.startsWith(" ")) {
      return [verb, rest.replace(/^\s+/u, "")];
    }
  }
  return null;
}

type DispositionResult =
  | { ok: true; value: DispositionAxis }
  | { ok: false; code: Code };

function parseDisposition(verb: string, rest: string, sp: Span): DispositionResult {
  const colon = rest.indexOf(":");
  if (colon < 0) return { ok: false, code: Code.L1100MissingDispositionTarget };
  const target = rest.slice(0, colon).trim();
  if (target.length === 0) return { ok: false, code: Code.L1100MissingDispositionTarget };
  const valuePart = rest.slice(colon + 1).trim();
  let amountPart: string;
  let mirror: string | null;
  const mIdx = valuePart.indexOf(" mirror ");
  if (mIdx >= 0) {
    amountPart = valuePart.slice(0, mIdx).trim();
    mirror = valuePart.slice(mIdx + 8).trim();
  } else {
    amountPart = valuePart;
    mirror = null;
  }
  const ofSplit = splitOnce(amountPart, " of ");
  if (ofSplit === null) return { ok: false, code: Code.L1101MalformedDispositionAmount };
  const current = parseF64(ofSplit[0].trim());
  const max = parseF64(ofSplit[1].trim());
  if (current === null || max === null) {
    return { ok: false, code: Code.L1101MalformedDispositionAmount };
  }
  return { ok: true, value: { verb, target, current, max, mirror, span: sp } };
}

function parseReactClause(text: string, sp: Span): ReactClause | null {
  // Allow both `→` and `->`.
  let sepIdx: number;
  let sepLen: number;
  const arrow = text.indexOf("→");
  if (arrow >= 0) {
    sepIdx = arrow;
    sepLen = 1;
  } else {
    const a = text.indexOf("->");
    if (a < 0) return null;
    sepIdx = a;
    sepLen = 2;
  }
  const condition = text.slice(0, sepIdx).trim();
  const tag = text.slice(sepIdx + sepLen).trim();
  if (condition.length === 0 || tag.length === 0) return null;
  return { condition, tag, span: sp };
}

function parseKnowledgeField(line: RawLine, diagnostics: Diagnostic[]): KnowledgeField | null {
  const text = line.text.trim();
  const colon = text.indexOf(":");
  if (colon < 0) {
    diagnostics.push(
      errorDiagnostic(
        Code.L1103MalformedKnowledgeField,
        line.span,
        "knowledge field needs `name: type [= default]`",
      ),
    );
    return null;
  }
  const name = text.slice(0, colon).trim();
  const rest = text.slice(colon + 1).trim();
  const eq = rest.indexOf("=");
  let typeSpec: string;
  let def: string | null;
  if (eq >= 0) {
    typeSpec = rest.slice(0, eq).trim();
    def = rest.slice(eq + 1).trim();
  } else {
    typeSpec = rest;
    def = null;
  }
  if (name.length === 0 || typeSpec.length === 0) {
    diagnostics.push(
      errorDiagnostic(
        Code.L1103MalformedKnowledgeField,
        line.span,
        "knowledge field needs `name: type [= default]`",
      ),
    );
    return null;
  }
  return { name, typeSpec, default: def, span: line.span };
}

function fillGoalField(goal: GoalDecl, text: string): void {
  text = text.trim();
  let v: string | null;
  if ((v = stripKeyed(text, "priority:")) !== null) {
    goal.priority = parseF64(v);
  } else if ((v = stripKeyed(text, "active when:")) !== null) {
    goal.activeWhen = v;
  } else if ((v = stripKeyed(text, "completes when:")) !== null) {
    goal.completesWhen = v;
  } else if ((v = stripKeyed(text, "fails when:")) !== null) {
    goal.failsWhen = v;
  } else if ((v = stripKeyed(text, "drives:")) !== null) {
    goal.drives = v;
  } else if ((v = stripKeyed(text, "on complete:")) !== null) {
    goal.onComplete = v;
  } else if ((v = stripKeyed(text, "on fail:")) !== null) {
    goal.onFail = v;
  }
}

function stripKeyed(text: string, key: string): string | null {
  const rest = stripPrefix(text, key);
  return rest !== null ? rest.trim() : null;
}

function splitProperty(text: string): [string, string] | null {
  const colon = text.indexOf(":");
  if (colon < 0) return null;
  const key = text.slice(0, colon).trim();
  if (
    key.length === 0 ||
    !allChars(key, (c) => isAsciiAlphanumeric(c) || c === "_" || c === "-")
  ) {
    return null;
  }
  return [key, text.slice(colon + 1).trim()];
}

function codeMessage(code: Code): string {
  switch (code) {
    case Code.L1100MissingDispositionTarget:
      return "disposition line is missing a target name";
    case Code.L1101MalformedDispositionAmount:
      return "expected `<current> of <max>` numeric pair";
    default:
      return "malformed declaration";
  }
}

// ---------------------------------------------------------------------
// STATS
// ---------------------------------------------------------------------

function lowerStats(body: RawLine[], diagnostics: Diagnostic[]): StatsBody {
  const out = emptyStatsBody();
  const baseIndent = body.length > 0 ? body[0]!.indent : 0;

  let i = 0;
  while (i < body.length) {
    const line = body[i]!;
    if (line.indent > baseIndent) {
      i += 1;
      continue;
    }
    const text = line.text.trim();

    {
      const rest = stripPrefix(text, "attribute ");
      if (rest !== null) {
        const a = parseAttribute(rest, line.span);
        if (a !== null) {
          out.attributes.push(a);
        } else {
          diagnostics.push(
            errorDiagnostic(
              Code.L1111MalformedAttribute,
              line.span,
              "expected `attribute name = N [, range LO to HI]`",
            ),
          );
        }
        i += 1;
        continue;
      }
    }

    {
      const rest = stripPrefix(text, "axis ");
      if (rest !== null) {
        const axis = emptyAxisDecl(rest.trim(), line.span);
        i += 1;
        while (i < body.length && body[i]!.indent > baseIndent) {
          const inner = body[i]!.text.trim();
          let v: string | null;
          if ((v = stripKeyed(inner, "mode:")) !== null) {
            axis.mode = v;
          } else if ((v = stripKeyed(inner, "curve:")) !== null) {
            axis.curve = v;
          } else if ((v = stripKeyed(inner, "on advance:")) !== null) {
            axis.onAdvance = v;
          } else if ((v = stripKeyed(inner, "milestones:")) !== null) {
            axis.milestones = v
              .split(",")
              .map((s) => s.trim())
              .filter((s) => s.length > 0);
          }
          axis.span = span(axis.span.start, body[i]!.span.end);
          i += 1;
        }
        if (axis.mode === null) {
          diagnostics.push(
            errorDiagnostic(
              Code.L1110AxisMissingMode,
              axis.span,
              `\`axis ${axis.name}\` is missing a \`mode:\` field`,
            ),
          );
        }
        out.axes.push(axis);
        continue;
      }
    }

    {
      const rest = stripPrefix(text, "pool ");
      if (rest !== null) {
        const pool = emptyPoolDecl(rest.trim(), line.span);
        i += 1;
        while (i < body.length && body[i]!.indent > baseIndent) {
          const inner = body[i]!.text.trim();
          let v: string | null;
          if ((v = stripKeyed(inner, "max:")) !== null) {
            pool.max = v;
          } else if ((v = stripKeyed(inner, "regen:")) !== null) {
            pool.regen = v;
          } else if ((v = stripKeyed(inner, "cost:")) !== null) {
            pool.cost = v;
          }
          pool.span = span(pool.span.start, body[i]!.span.end);
          i += 1;
        }
        if (pool.max === null) {
          diagnostics.push(
            errorDiagnostic(
              Code.L1112PoolMissingMax,
              pool.span,
              `\`pool ${pool.name}\` is missing a \`max:\` field`,
            ),
          );
        }
        out.pools.push(pool);
        continue;
      }
    }

    {
      const rest = stripPrefix(text, "stat ");
      if (rest !== null) {
        const eq = rest.indexOf("=");
        if (eq >= 0) {
          const name = rest.slice(0, eq).trim();
          const expression = rest.slice(eq + 1).trim();
          if (name.length > 0 && expression.length > 0) {
            out.stats.push({ name, expression, span: line.span });
          }
        }
        i += 1;
        continue;
      }
    }

    if (isInitOpener(text)) {
      const [params] = parseInitSignature(text);
      const init: InitDecl = { params, body: [], span: line.span };
      i += 1;
      while (i < body.length && body[i]!.indent > baseIndent) {
        init.body.push(body[i]!);
        init.span = span(init.span.start, body[i]!.span.end);
        i += 1;
      }
      out.init = init;
      continue;
    }

    {
      const rest = stripPrefix(text, "method ");
      if (rest !== null) {
        const method = parseMethodOpener(rest, line.span);
        if (method !== null) {
          if (method.inlineExpr === null) {
            i += 1;
            while (i < body.length && body[i]!.indent > baseIndent) {
              method.body.push(body[i]!);
              method.span = span(method.span.start, body[i]!.span.end);
              i += 1;
            }
          } else {
            i += 1;
          }
          out.methods.push(method);
          continue;
        }
        i += 1;
        continue;
      }
    }

    i += 1;
  }
  return out;
}

function parseAttribute(rest: string, sp: Span): AttributeDecl | null {
  const eq = rest.indexOf("=");
  if (eq < 0) return null;
  const name = rest.slice(0, eq).trim();
  const after = rest.slice(eq + 1);
  let defaultPart: string;
  let rangePart: string | null;
  const comma = after.indexOf(",");
  if (comma >= 0) {
    defaultPart = after.slice(0, comma).trim();
    rangePart = after.slice(comma + 1).trim();
  } else {
    defaultPart = after.trim();
    rangePart = null;
  }
  const def = parseF64(defaultPart);
  if (def === null) return null;
  let min = Number.NEGATIVE_INFINITY;
  let max = Number.POSITIVE_INFINITY;
  if (rangePart !== null) {
    const r = stripPrefix(rangePart, "range ");
    if (r !== null) {
      const lh = splitOnce(r, " to ");
      if (lh !== null) {
        const lo = parseF64(lh[0].trim());
        const hi = parseF64(lh[1].trim());
        if (lo === null || hi === null) return null;
        min = lo;
        max = hi;
      }
    }
  }
  if (name.length === 0) return null;
  return { name, default: def, min, max, span: sp };
}

// ---------------------------------------------------------------------
// TREE
// ---------------------------------------------------------------------

function lowerTree(body: RawLine[], diagnostics: Diagnostic[]): TreeBody {
  const out: TreeBody = { nodes: [] };
  const baseIndent = body.length > 0 ? body[0]!.indent : 0;
  let i = 0;
  while (i < body.length) {
    const line = body[i]!;
    if (line.indent > baseIndent) {
      i += 1;
      continue;
    }
    const text = line.text.trim();
    const rest = stripPrefix(text, "node ");
    if (rest !== null) {
      const name = rest.trim();
      if (name.length === 0) {
        diagnostics.push(
          errorDiagnostic(Code.L1113UnnamedTreeNode, line.span, "`node` declaration is missing a name"),
        );
        i += 1;
        continue;
      }
      const node = emptyTreeNodeDecl(name, line.span);
      i += 1;
      while (i < body.length && body[i]!.indent > baseIndent) {
        const inner = body[i]!.text.trim();
        let v: string | null;
        if ((v = stripKeyed(inner, "cost:")) !== null) {
          node.cost = v;
        } else if ((v = stripKeyed(inner, "requires:")) !== null) {
          node.requires = v;
        } else if ((v = stripKeyed(inner, "effect:")) !== null) {
          node.effects.push(v);
        }
        node.span = span(node.span.start, body[i]!.span.end);
        i += 1;
      }
      out.nodes.push(node);
      continue;
    }
    i += 1;
  }
  return out;
}

// ---------------------------------------------------------------------
// PERSON (spec v3 §13.2)
// ---------------------------------------------------------------------

function lowerPerson(body: RawLine[]): PersonBody {
  const out = emptyPersonBody();
  for (const line of body) {
    const text = line.text.trim();
    const kv = splitProperty(text);
    if (kv === null) continue;
    const [key, value] = kv;
    switch (key) {
      case "display_name":
        out.displayName = value;
        break;
      case "pronouns":
        out.pronouns = value;
        break;
      case "email":
        out.email = value;
        break;
      case "device":
        out.device = value;
        break;
      case "notes":
        out.notes = value;
        break;
      case "content_tolerance":
        out.contentTolerance = parseBracketedList(value);
        break;
      case "accessibility":
        out.accessibility = parseBracketedList(value);
        break;
    }
    out.properties.set(key, { value, span: line.span });
  }
  return out;
}

function parseBracketedList(raw: string): string[] {
  let trimmed = raw.trim();
  trimmed = trimStartMatches(trimmed, "[");
  // trim_end_matches(']')
  while (trimmed.endsWith("]")) trimmed = trimmed.slice(0, trimmed.length - 1);
  trimmed = trimmed.trim();
  if (trimmed.length === 0) return [];
  return trimmed
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

// ---------------------------------------------------------------------
// ROSTER (spec v3 §13.3)
// ---------------------------------------------------------------------

function lowerRoster(body: RawLine[]): RosterBody {
  const out = emptyRosterBody();
  const baseIndent = body.length > 0 ? body[0]!.indent : 0;

  let i = 0;
  while (i < body.length) {
    const line = body[i]!;
    if (line.indent > baseIndent) {
      i += 1;
      continue;
    }
    const text = line.text.trim();

    if (text === "cast" || text === "cast:") {
      i += 1;
      while (i < body.length && body[i]!.indent > baseIndent) {
        const entry = parseCastEntry(body[i]!.text.trim(), body[i]!.span);
        if (entry !== null) out.cast.push(entry);
        i += 1;
      }
      continue;
    }
    if (text === "swings" || text === "swings:") {
      i += 1;
      while (i < body.length && body[i]!.indent > baseIndent) {
        const entry = parseSwingEntry(body[i]!.text.trim(), body[i]!.span);
        if (entry !== null) out.swings.push(entry);
        i += 1;
      }
      continue;
    }
    if (text === "cohorts" || text === "cohorts:") {
      i += 1;
      while (i < body.length && body[i]!.indent > baseIndent) {
        const entry = parseCohortEntry(body[i]!.text.trim(), body[i]!.span);
        if (entry !== null) out.cohorts.push(entry);
        i += 1;
      }
      continue;
    }
    if (text === "locations" || text === "locations:") {
      i += 1;
      while (i < body.length && body[i]!.indent > baseIndent) {
        const entry = parseLocationEntry(body[i]!.text.trim(), body[i]!.span);
        if (entry !== null) out.locations.push(entry);
        i += 1;
      }
      continue;
    }
    if (text === "notes" || text === "notes:") {
      i += 1;
      while (i < body.length && body[i]!.indent > baseIndent) {
        const kv = splitProperty(body[i]!.text.trim());
        if (kv !== null) {
          out.notes.set(kv[0], { value: kv[1], span: body[i]!.span });
        }
        i += 1;
      }
      continue;
    }

    const kv = splitProperty(text);
    if (kv !== null) {
      const [key, value] = kv;
      if (key === "date") out.date = value;
      else if (key === "capacity") out.capacity = parseU32(value);
      out.properties.set(key, { value, span: line.span });
    }
    i += 1;
  }
  return out;
}

function parseCastEntry(text: string, sp: Span): RosterCastEntry | null {
  const split = splitOnce(text, ":=");
  if (split === null) return null;
  const role = split[0].trim();
  const rhs = split[1].trim();
  if (role.length === 0) return null;
  let assignment: RosterAssignment;
  const anyOf = stripPrefix(rhs, "any of ");
  if (rhs.length === 0 || rhs === "none") {
    assignment = { kind: "ghost" };
  } else if (anyOf !== null) {
    assignment = { kind: "anyOf", pool: parseBracketedList(anyOf) };
  } else {
    assignment = { kind: "person", name: rhs };
  }
  return { role, assignment, span: sp };
}

function parseSwingEntry(text: string, sp: Span): RosterSwingEntry | null {
  const split = splitOnce(text, ":=");
  if (split === null) return null;
  const role = split[0].trim();
  const fallbacks = parseBracketedList(split[1].trim());
  if (role.length === 0) return null;
  return { role, fallbacks, span: sp };
}

function parseCohortEntry(text: string, sp: Span): RosterCohortEntry | null {
  const split = splitOnce(text, "start with:");
  if (split === null) return null;
  const cohort = split[0].trim();
  if (cohort.length === 0) return null;
  return { cohort, startWith: parseBracketedList(split[1].trim()), span: sp };
}

function parseLocationEntry(text: string, sp: Span): RosterLocationEntry | null {
  const split = splitOnce(text, "start with:");
  if (split === null) return null;
  const location = split[0].trim();
  if (location.length === 0) return null;
  return { location, startWith: parseBracketedList(split[1].trim()), span: sp };
}
