//! The pluggable channel-type registry.
//!
//! A channel's *visibility* comes from its `kind` (see `canSeeChannel` in
//! `sim.ts`); its *behaviour* comes from a `ChannelRules` bundle resolved
//! here. Rules are keyed by a **type name** in a registry, so a new room
//! behaviour is added once (`registerChannelType`) and authors select it with
//! `type: <name>` (or just inherit their `kind`'s defaults) plus inline
//! overrides (`post:` / `threads:` / `routes:` / `slow:` / `ephemeral:`).

/** Who may post into a channel. */
export type PostPolicy =
  | { kind: "everyone" }
  | { kind: "members" }
  | { kind: "faction" }
  | { kind: "none" } // read-only for participants — only story/system lines land
  | { kind: "role"; role: string };

export interface ChannelRules {
  post: PostPolicy;
  /** Whether messages here can open Slack-style threads. */
  threadable: boolean;
  /** Broadcast cues mirrored into this channel: exact cue names, or `["*"]`. */
  routes: string[];
  /** Reserved behaviours — declared, enforcement is a follow-up. */
  slowModeMs: number | null;
  ephemeralMs: number | null;
}

/** Named type → default rules. Register a new name to add a room behaviour. */
export const CHANNEL_TYPES = new Map<string, ChannelRules>();

export function registerChannelType(name: string, rules: ChannelRules): void {
  CHANNEL_TYPES.set(name, rules);
}

function rules(partial: Partial<ChannelRules>): ChannelRules {
  return {
    post: { kind: "everyone" },
    threadable: true,
    routes: [],
    slowModeMs: null,
    ephemeralMs: null,
    ...partial,
  };
}

export const DEFAULT_RULES: ChannelRules = rules({});

// Built-in types: one per visibility kind, plus a read-only announcement feed
// that mirrors every broadcast. Authors get all of these for free.
registerChannelType("open", rules({ post: { kind: "everyone" } }));
registerChannelType("private", rules({ post: { kind: "members" } }));
registerChannelType("group", rules({ post: { kind: "members" } }));
registerChannelType("dm", rules({ post: { kind: "members" }, threadable: false }));
registerChannelType("faction", rules({ post: { kind: "faction" } }));
registerChannelType("announcement", rules({ post: { kind: "none" }, threadable: false, routes: ["*"] }));

function parseDurationMs(s: string): number | null {
  const m = /^(\d+(?:\.\d+)?)(ms|s|m)?$/u.exec(s.trim());
  if (m === null) return null;
  const n = Number(m[1]);
  const unit = m[2] ?? "s";
  return unit === "ms" ? n : unit === "m" ? n * 60000 : n * 1000;
}

function parsePost(value: string): PostPolicy | null {
  const v = value.trim();
  if (v === "everyone" || v === "all") return { kind: "everyone" };
  if (v === "members") return { kind: "members" };
  if (v === "faction") return { kind: "faction" };
  if (v === "none" || v === "readonly") return { kind: "none" };
  const role = /^role[\s:]+(\S+)$/u.exec(v);
  if (role !== null) return { kind: "role", role: role[1]! };
  return null;
}

export interface RuleOverrides {
  post: string | null;
  threads: string | null;
  routes: string | null;
  slow: string | null;
  ephemeral: string | null;
}

/**
 * Resolve a channel's rules from its `kind`'s defaults (or an explicit
 * `type:` preset), then apply the author's inline overrides.
 */
export function resolveRules(kind: string, type: string | null, o: RuleOverrides): ChannelRules {
  const base = (type !== null ? CHANNEL_TYPES.get(type) : undefined) ?? CHANNEL_TYPES.get(kind) ?? DEFAULT_RULES;
  const r: ChannelRules = { ...base, routes: [...base.routes] };
  if (o.post !== null) {
    const p = parsePost(o.post);
    if (p !== null) r.post = p;
  }
  if (o.threads !== null) r.threadable = !/^(off|no|false)$/iu.test(o.threads.trim());
  if (o.routes !== null) {
    r.routes = o.routes.trim() === "*" ? ["*"] : o.routes.split(",").map((s) => s.trim()).filter((s) => s.length > 0);
  }
  if (o.slow !== null) r.slowModeMs = parseDurationMs(o.slow);
  if (o.ephemeral !== null) r.ephemeralMs = parseDurationMs(o.ephemeral);
  return r;
}

/** Does a channel with these `routes` receive a broadcast of `cue`? */
export function routesCue(r: ChannelRules, cue: string): boolean {
  return r.routes.includes("*") || r.routes.includes(cue);
}
