//! The multi-tenant event registry.
//!
//! One process can host many live events at once — one per project that has
//! opened its doors. `EventRegistry` maps a stable event id to its live
//! `EventRuntime`, and resolves a short passcode back to the event (and role)
//! it belongs to — the bootstrap a guest / performer / moderator needs before
//! they know which `/e/:eventId` to talk to.

import { join } from "node:path";

import { passOk, type Passcodes } from "./auth.ts";
import { Store } from "./store.ts";
import { EventRuntime } from "./event-runtime.ts";

/** The role a passcode grants when it resolves to an event. */
export type CodeRole = "guest" | "prime" | "mod";

/** A resolved passcode: which event it belongs to, and as which role. */
export interface ResolvedCode {
  eventId: string;
  role: CodeRole;
}

/** How to build a runtime the registry hasn't seen yet (for `ensure`). */
export interface EventSpec {
  eventId: string;
  codes: Passcodes;
  scenarioName: string;
  scenarioSource: string;
}

export class EventRegistry {
  private readonly byId = new Map<string, EventRuntime>();

  /**
   * @param stateDir base directory; each event journals under `stateDir/<id>`.
   * @param joinBase best base URL for guest-join QRs (per-event codes view).
   */
  constructor(
    private readonly stateDir: string,
    private readonly joinBase: () => string,
  ) {}

  /** Attach an already-constructed runtime (e.g. the default event). */
  register(runtime: EventRuntime): EventRuntime {
    this.byId.set(runtime.eventId, runtime);
    return runtime;
  }

  /** The runtime for an event id, or undefined if it isn't hosted here. */
  get(eventId: string): EventRuntime | undefined {
    return this.byId.get(eventId);
  }

  /** Every hosted runtime. */
  all(): EventRuntime[] {
    return [...this.byId.values()];
  }

  /** Is this event currently hosted? */
  has(eventId: string): boolean {
    return this.byId.has(eventId);
  }

  /**
   * Get the runtime for `spec.eventId`, constructing + registering (and
   * restoring from disk) one if it isn't hosted yet. Idempotent.
   */
  ensure(spec: EventSpec): EventRuntime {
    const existing = this.byId.get(spec.eventId);
    if (existing) return existing;
    const runtime = new EventRuntime({
      eventId: spec.eventId,
      store: new Store(join(this.stateDir, spec.eventId)),
      codes: spec.codes,
      scenarioName: spec.scenarioName,
      scenarioSource: spec.scenarioSource,
      joinBase: this.joinBase,
    });
    runtime.restore();
    return this.register(runtime);
  }

  /** Stop hosting an event: dispose its runtime and drop it. */
  stop(eventId: string): void {
    const runtime = this.byId.get(eventId);
    if (runtime) {
      runtime.dispose();
      this.byId.delete(eventId);
    }
  }

  /**
   * Resolve a short passcode to the event + role it grants. Guests use an
   * event's `event` code, performers its `prime` code, moderators its `mod`
   * code. Codes are globally unique across live events, so the first match
   * wins. Returns null when no hosted event recognises the code.
   */
  resolveCode(code: string): ResolvedCode | null {
    const trimmed = code.trim();
    if (trimmed === "") return null;
    for (const rt of this.byId.values()) {
      if (passOk(trimmed, rt.codes.event)) return { eventId: rt.eventId, role: "guest" };
      if (passOk(trimmed, rt.codes.prime)) return { eventId: rt.eventId, role: "prime" };
      if (passOk(trimmed, rt.codes.mod)) return { eventId: rt.eventId, role: "mod" };
    }
    return null;
  }
}
