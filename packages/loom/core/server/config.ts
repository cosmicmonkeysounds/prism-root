//! Central server configuration, resolved once from the environment.
//!
//! The event plane (live `Sim` hosting) needs nothing here — it runs on any
//! phone-friendly LAN with zero setup. The **control plane** (author
//! accounts, projects, launching events) is what needs a database and an
//! auth secret; those are read here so a missing `DATABASE_URL` degrades to
//! "events still work, authoring is disabled" rather than a boot crash.

/** Where the author account / project / event registry lives. */
export const DATABASE_URL = process.env.DATABASE_URL ?? "postgres://127.0.0.1:5432/loom_dev";

/** Signing secret for BetterAuth sessions. Stable across restarts in prod. */
export const AUTH_SECRET =
  process.env.BETTER_AUTH_SECRET ??
  process.env.LOOM_AUTH_SECRET ??
  "loom-dev-insecure-secret-change-me";

/** Public base URL the app is served from (used by BetterAuth for cookies). */
export const BASE_URL = process.env.LOOM_BASE_URL ?? process.env.BETTER_AUTH_URL ?? `http://localhost:${process.env.LOOM_PORT ?? 7000}`;

/** True when a non-default auth secret is configured (required for prod). */
export const HAS_SECURE_SECRET = AUTH_SECRET !== "loom-dev-insecure-secret-change-me";
