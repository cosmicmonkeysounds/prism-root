//! Passcodes for the live event. Three independent codes gate the three
//! roles: a **guest event code** (so only invited party-goers can join),
//! a **performer passcode**, and a **moderator passcode**. Kept here —
//! pure and dependency-free — so the generation + comparison rules are
//! unit-testable without standing up the HTTP server.
//!
//! Codes are short and *speakable*: six characters from an unambiguous
//! alphabet (no `0/O`, `1/I/L`) so an operator can read one across a noisy
//! room and a guest can thumb it into a phone without squinting. Matching
//! is trimmed and case-insensitive for the same reason.

import { randomBytes } from "node:crypto";

/** Roles that carry a passcode. Guests use `event`. */
export type PassRole = "event" | "mod" | "prime";

export interface Passcodes {
  /** Guests must enter this to register. */
  event: string;
  /** Moderators (operator console) sign in with this. */
  mod: string;
  /** Performers sign in as their character with this. */
  prime: string;
}

/** Unambiguous alphabet — drops `0 O 1 I L` to survive a read-aloud. */
const ALPHABET = "ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const LEN = 6;

/** A fresh, random, speakable passcode (e.g. `K7PQ3M`). */
export function makePass(): string {
  const bytes = randomBytes(LEN);
  let out = "";
  for (let i = 0; i < LEN; i++) out += ALPHABET[bytes[i]! % ALPHABET.length];
  return out;
}

/**
 * Does `input` match `expected`? Trimmed + case-insensitive so trailing
 * spaces and a stray capital don't lock a guest out of their own event.
 */
export function passOk(input: string, expected: string): boolean {
  return input.trim().toLowerCase() === expected.trim().toLowerCase();
}

/**
 * Resolve the three passcodes at boot. Precedence per role:
 *   1. an explicit `LOOM_*_PASS` env var (operator override),
 *   2. the code persisted from a previous run (stable across restarts),
 *   3. a freshly generated one (first run).
 *
 * Persisting the generated codes is what makes a restart transparent: the
 * mod has already shared "the event code is K7PQ3M", so it must not churn
 * just because the process bounced.
 */
export function resolvePasscodes(
  env: Record<string, string | undefined>,
  persisted: Partial<Passcodes> | null,
): Passcodes {
  return {
    event: env.LOOM_EVENT_PASS ?? persisted?.event ?? makePass(),
    mod: env.LOOM_MOD_PASS ?? persisted?.mod ?? makePass(),
    prime: env.LOOM_PRIME_PASS ?? persisted?.prime ?? makePass(),
  };
}
