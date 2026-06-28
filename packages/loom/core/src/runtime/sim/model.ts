//! Compile a parsed `Bundle` into a `SimModel` — the static shape of
//! the world the runtime drives: factions, locations, roles,
//! characters, their reactive hooks, and the scripted beats.

import type {
  Beat,
  BodyItem,
  CharacterBody,
  FactionBody,
  LocationBody,
  Property,
} from "../../parser/index.ts";
import type { Bundle } from "../bundle.ts";
import { vBool, vNumber, vString, type Value } from "../expr.ts";
import { lowerRawBody } from "./effects.ts";

export type EntityKind = "person" | "character" | "faction" | "location" | "role";

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
  /** Structured effect body (shared executor with beats). */
  body: BodyItem[];
  /** Raw `on …` clause, kept for diagnostics. */
  event: string;
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

export interface SimModel {
  factions: Map<string, FactionDef>;
  locations: Map<string, LocationDef>;
  roles: Map<string, RoleDef>;
  characters: Map<string, CharDef>;
  beats: Map<string, Beat>;
  /** id → kind, for seeding self-identity values + dispatch. */
  entityKind: Map<string, EntityKind>;
  /** The role a fresh Person is cast into (first `ROLE` declared). */
  defaultRole: string | null;
  /** `entry:` beat from a file header, if any. */
  entry: string | null;
}

/** Compile every loaded file in `bundle` into one `SimModel`. */
export function compileModel(bundle: Bundle): SimModel {
  const model: SimModel = {
    factions: new Map(),
    locations: new Map(),
    roles: new Map(),
    characters: new Map(),
    beats: new Map(),
    entityKind: new Map(),
    defaultRole: null,
    entry: null,
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
          }
          break;
        default:
          break;
      }
    }
  }
  return model;
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
      return { ownerId, ownerKind, verb, param, filter, body: lowerRawBody(h.body), event: h.event };
    });
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
