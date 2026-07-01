//! Compile a parsed `Bundle` into a `SimModel` — the static shape of
//! the world the runtime drives: factions, locations, roles,
//! characters, their reactive hooks, and the scripted beats.

import type {
  Beat,
  BodyItem,
  ChannelBody,
  ChannelKindWord,
  CharacterBody,
  FactionBody,
  LocationBody,
  Property,
  RawLine,
  SpaceBody,
} from "../../parser/index.ts";
import { stripPrefix } from "../../parser/rust.ts";
import type { Bundle } from "../bundle.ts";
import { vBool, vNumber, vString, type Value } from "../expr.ts";
import { lowerRawBody } from "./effects.ts";
import { resolveRules, type ChannelRules } from "./channel-types.ts";

export type EntityKind = "person" | "character" | "faction" | "location" | "role";

/** A time-driven trigger — `on every 30s` / `on after 2m`. */
export interface TimerSpec {
  mode: "every" | "after";
  ms: number;
}

/** A reactive rule attached to a character or role. */
export interface Hook {
  ownerId: string;
  ownerKind: "character" | "role";
  /** Trigger verb — `scan`, `captured`, `join`, a signal name, … */
  verb: string;
  /** Subject bound from the event, e.g. `guest` in `on scan guest`. */
  param: string | null;
  /** Entity filter, e.g. `Mods` in `on join Mods`. */
  filter: string | null;
  /** A time trigger when this is `on every …` / `on after …`, else null. */
  timer: TimerSpec | null;
  /** Structured effect body (shared executor with beats). */
  body: BodyItem[];
  /** Raw `on …` clause, kept for diagnostics. */
  event: string;
}

/** A compiled ambient generator — emits a bark on a fixed interval. */
export interface GenSpec {
  id: string;
  intervalMs: number;
  barks: string[];
}

export interface FactionDef {
  id: string;
  ethos: string | null;
  hidden: boolean;
  rival: string | null;
}

export interface LocationDef {
  id: string;
  label: string | null;
  prison: boolean;
  capacity: number | null;
  contains: string[];
}

export interface RoleDef {
  id: string;
  /** Per-person state defaults from the typed slots. */
  defaults: Map<string, Value>;
  hooks: Hook[];
}

export interface CharDef {
  id: string;
  faction: string | null;
  hooks: Hook[];
  /** `trusts X: N of M` axes (relationship seeds against the role). */
  disposition: CharacterBody["disposition"];
}

/** The default space authored channels fall into when none is named. */
export const DEFAULT_SPACE_ID = "internet";

export interface SpaceDef {
  id: string;
  title: string;
  /** Channel ids in source order. */
  channelIds: string[];
}

/** An authored chatroom (spec: SPACE/CHANNEL). `id` is `room:<name>`. */
export interface ChannelDef {
  id: string;
  spaceId: string;
  kind: ChannelKindWord;
  title: string;
  /** For `kind: faction`, the faction whose members this channel serves. */
  faction: string | null;
  /** Declared seed members (character/role names or guest ids). */
  members: string[];
  /** Who may invite into a private channel (`members` | `anyone` | role). */
  invite: string | null;
  /** Behaviour bundle resolved from the channel-type registry (post policy,
   *  threadability, broadcast routing, …). */
  rules: ChannelRules;
}

/** The channel-id namespace for an authored channel name. */
export function channelId(name: string): string {
  return `room:${name.trim()}`;
}

export interface SimModel {
  factions: Map<string, FactionDef>;
  locations: Map<string, LocationDef>;
  roles: Map<string, RoleDef>;
  characters: Map<string, CharDef>;
  /** Authored spaces (Discord-style sidebar containers). */
  spaces: Map<string, SpaceDef>;
  /** Authored channels, keyed by `room:<name>`. */
  channels: Map<string, ChannelDef>;
  beats: Map<string, Beat>;
  /** id → kind, for seeding self-identity values + dispatch. */
  entityKind: Map<string, EntityKind>;
  /** The role a fresh Person is cast into (first `ROLE` declared). */
  defaultRole: string | null;
  /** `entry:` beat from a file header, if any. */
  entry: string | null;
  /** Character-owned time-driven hooks, fired by `Sim.tick`. */
  timerHooks: Hook[];
  /** Ambient generators (top-level + character-bound), fired by `tick`. */
  gens: GenSpec[];
}

/** Compile every loaded file in `bundle` into one `SimModel`. */
export function compileModel(bundle: Bundle): SimModel {
  const model: SimModel = {
    factions: new Map(),
    locations: new Map(),
    roles: new Map(),
    characters: new Map(),
    spaces: new Map(),
    channels: new Map(),
    beats: new Map(),
    entityKind: new Map(),
    defaultRole: null,
    entry: null,
    timerHooks: [],
    gens: [],
  };

  for (const entry of bundle.files) {
    const entryProp = entry.file.header.properties.get("entry");
    if (model.entry === null && entryProp !== undefined) model.entry = entryProp.value;

    for (const item of entry.file.items) {
      if (item.kind === "beat") {
        model.beats.set(item.value.name, item.value);
        continue;
      }
      if (item.kind !== "declaration") continue;
      const decl = item.value;
      switch (decl.kind) {
        case "faction":
          if (decl.faction) {
            model.factions.set(decl.name, factionDef(decl.name, decl.faction));
            model.entityKind.set(decl.name, "faction");
          }
          break;
        case "location":
          if (decl.location) {
            model.locations.set(decl.name, locationDef(decl.name, decl.location));
            model.entityKind.set(decl.name, "location");
          }
          break;
        case "role":
          if (decl.character) {
            model.roles.set(decl.name, roleDef(decl.name, decl.character));
            model.entityKind.set(decl.name, "role");
            if (model.defaultRole === null) model.defaultRole = decl.name;
          }
          break;
        case "character":
          if (decl.character) {
            model.characters.set(decl.name, charDef(decl.name, decl.character));
            model.entityKind.set(decl.name, "character");
            // Character-bound generators (spec §10.5) become ambient emitters.
            for (const gen of decl.character.generators) {
              const spec = genSpec(`${decl.name}.${gen.name}`, gen.body);
              if (spec !== null) model.gens.push(spec);
            }
          }
          break;
        case "generator":
          if (decl.generator) {
            const spec = genSpec(decl.name, decl.generator.body);
            if (spec !== null) model.gens.push(spec);
          }
          break;
        case "space":
          if (decl.space) registerSpace(model, decl.name, decl.space);
          break;
        case "channel":
          if (decl.channel) registerChannel(model, decl.channel, decl.channel.space);
          break;
        default:
          break;
      }
    }
  }

  // Character-owned time hooks drive the autonomous clock.
  for (const char of model.characters.values()) {
    for (const hook of char.hooks) if (hook.timer !== null) model.timerHooks.push(hook);
  }
  // Every channel's space must exist; synthesise one for `space:`-referenced
  // or default-space channels, and keep `channelIds` consistent + ordered.
  for (const ch of model.channels.values()) {
    const space = ensureSpace(model, ch.spaceId);
    if (!space.channelIds.includes(ch.id)) space.channelIds.push(ch.id);
  }
  return model;
}

function channelDef(body: ChannelBody, spaceId: string): ChannelDef {
  return {
    id: channelId(body.name),
    spaceId,
    kind: body.kind,
    title: body.label ?? `#${body.name}`,
    faction: body.faction,
    members: [...body.members],
    invite: body.invite,
    rules: resolveRules(body.kind, body.type, {
      post: body.post,
      threads: body.threads,
      routes: body.routes,
      slow: body.slow,
      ephemeral: body.ephemeral,
    }),
  };
}

function registerChannel(model: SimModel, body: ChannelBody, spaceId: string | null): void {
  const def = channelDef(body, spaceId ?? DEFAULT_SPACE_ID);
  model.channels.set(def.id, def);
}

function registerSpace(model: SimModel, name: string, body: SpaceBody): void {
  const space: SpaceDef = ensureSpace(model, name, body.label ?? name);
  for (const ch of body.channels) {
    const def = channelDef(ch, name);
    model.channels.set(def.id, def);
    if (!space.channelIds.includes(def.id)) space.channelIds.push(def.id);
  }
}

/** Get-or-create a space (a `space:`-referenced or default space is implicit). */
function ensureSpace(model: SimModel, id: string, title?: string): SpaceDef {
  let space = model.spaces.get(id);
  if (space === undefined) {
    space = { id, title: title ?? id, channelIds: [] };
    model.spaces.set(id, space);
  } else if (title !== undefined) {
    space.title = title; // a later explicit SPACE wins over an implicit one
  }
  return space;
}

function factionDef(id: string, body: FactionBody): FactionDef {
  return {
    id,
    ethos: scalarProp(body.properties, "ethos"),
    hidden: scalarProp(body.properties, "hidden") === "true",
    rival: scalarProp(body.properties, "rival"),
  };
}

function locationDef(id: string, body: LocationBody): LocationDef {
  return {
    id,
    label: body.label,
    prison: body.properties.get("prison")?.value === "true",
    capacity: body.capacity,
    contains: body.contains,
  };
}

function roleDef(id: string, body: CharacterBody): RoleDef {
  const defaults = new Map<string, Value>();
  for (const prop of body.typedProperties) {
    const v = defaultValueOf(prop);
    if (v !== null) defaults.set(prop.name, v);
  }
  return { id, defaults, hooks: hooksOf(id, "role", body) };
}

function charDef(id: string, body: CharacterBody): CharDef {
  return {
    id,
    faction: body.properties.get("faction")?.value ?? null,
    hooks: hooksOf(id, "character", body),
    disposition: body.disposition,
  };
}

function hooksOf(ownerId: string, ownerKind: "character" | "role", body: CharacterBody): Hook[] {
  return body.hooks
    .filter((h) => !h.suppressed)
    .map((h) => {
      const { verb, param, filter } = parseTrigger(h.event);
      return {
        ownerId,
        ownerKind,
        verb,
        param,
        filter,
        timer: parseTimer(h.event),
        body: lowerRawBody(h.body),
        event: h.event,
      };
    });
}

/** Parse `every 30s` / `after 2m` into a timer spec, else null. */
export function parseTimer(event: string): TimerSpec | null {
  const words = event.trim().split(/\s+/u).filter((w) => w.length > 0);
  const head = words[0];
  if (head !== "every" && head !== "after" && head !== "at") return null;
  const ms = parseDuration(words.slice(1).join(""));
  if (ms === null) return null;
  return { mode: head === "every" ? "every" : "after", ms };
}

/** Parse a duration token (`30s`, `200ms`, `2m`, bare = seconds) to ms. */
export function parseDuration(s: string): number | null {
  const m = /^(\d+(?:\.\d+)?)(ms|s|m)?$/u.exec(s.trim());
  if (m === null) return null;
  const n = Number(m[1]);
  const unit = m[2] ?? "s";
  return unit === "ms" ? n : unit === "m" ? n * 60000 : n * 1000;
}

/** Lower a generator body into an interval + bark list, or null. */
function genSpec(id: string, body: RawLine[]): GenSpec | null {
  let intervalMs: number | null = null;
  const barks: string[] = [];
  for (const line of body) {
    const text = line.text.trim();
    const every = stripPrefix(text, "every ");
    if (every !== null) {
      // `every 20s` or `wait random(20s, 60s)` → take the first duration.
      const tok = every.trim().split(/[\s,()]+/u).find((t) => /^\d/u.test(t));
      if (tok !== undefined) intervalMs = parseDuration(tok);
      continue;
    }
    const barkFrom = stripPrefix(text, "yield bark from ");
    if (barkFrom !== null) {
      barks.push(...barkFrom.split("|").map((s) => s.trim()).filter((s) => s.length > 0));
      continue;
    }
    const yieldText = stripPrefix(text, "yield ");
    if (yieldText !== null) barks.push(yieldText.trim());
  }
  if (intervalMs === null || barks.length === 0) return null;
  return { id, intervalMs, barks };
}

/** Parse an `on …` clause into a structured trigger. */
export function parseTrigger(event: string): {
  verb: string;
  param: string | null;
  filter: string | null;
} {
  const words = event.trim().split(/\s+/u).filter((w) => w.length > 0);
  const verb = words[0] ?? "";
  let param: string | null = null;
  let filter: string | null = null;
  for (const w of words.slice(1)) {
    if (/^[A-Z]/u.test(w)) filter = w;
    else if (/^[a-z_][A-Za-z0-9_]*$/u.test(w) && param === null) param = w;
  }
  return { verb, param, filter };
}

/** The default first/last typed-slot value (`default ?? rawType`). */
function scalarProp(props: Property[], name: string): string | null {
  const p = props.find((x) => x.name === name);
  if (p === undefined) return null;
  return p.default ?? p.rawType;
}

function defaultValueOf(prop: Property): Value | null {
  if (prop.slotType !== null && prop.slotType.kind === "range" && prop.slotType.default !== null) {
    return vNumber(prop.slotType.default);
  }
  if (prop.default !== null) {
    const d = prop.default.trim();
    if (d === "true") return vBool(true);
    if (d === "false") return vBool(false);
    const n = Number(d);
    if (d.length > 0 && !Number.isNaN(n)) return vNumber(n);
    return vString(d);
  }
  return null;
}
