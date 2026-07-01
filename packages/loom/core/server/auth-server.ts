//! The author-account auth plane, powered by BetterAuth.
//!
//! This is entirely separate from the live-event passcode plane: it gates the
//! **control plane** (projects, files, launching events) for signed-in
//! authors, while party-goers keep using per-event codes with no account.
//!
//! Mounted on the raw `node:http` server via `toNodeHandler` (see
//! `server.ts`), so there's no web framework to adopt. Sessions are read in
//! other handlers with `authUser(req)`.

import type { IncomingMessage } from "node:http";

import { betterAuth } from "better-auth";
import { fromNodeHeaders } from "better-auth/node";
import { getMigrations } from "better-auth/db/migration";

import { pool } from "./db/index.ts";
import { AUTH_SECRET, BASE_URL } from "./config.ts";

export const auth = betterAuth({
  database: pool(),
  secret: AUTH_SECRET,
  baseURL: BASE_URL,
  basePath: "/api/auth",
  emailAndPassword: {
    enabled: true,
    // Registering an author signs them straight in — no email round-trip,
    // matching the "sign up and start building" flow.
    autoSignIn: true,
  },
  // Dev origins: the Vite dev servers for the editor / participant apps, and
  // the same-origin production port. Extend via LOOM_TRUSTED_ORIGINS if needed.
  trustedOrigins: [
    "http://localhost:5173",
    "http://localhost:5174",
    "http://localhost:7000",
    ...(process.env.LOOM_TRUSTED_ORIGINS?.split(",").map((s) => s.trim()).filter(Boolean) ?? []),
  ],
});

/** Create/upgrade the BetterAuth tables (user/session/account/verification). */
export async function migrateAuth(): Promise<void> {
  const { runMigrations } = await getMigrations(auth.options);
  await runMigrations();
}

/** The signed-in author for a request, or null. Reads the session cookie. */
export async function authUser(req: IncomingMessage): Promise<{ id: string; email: string; name: string } | null> {
  const session = await auth.api.getSession({ headers: fromNodeHeaders(req.headers) });
  if (!session?.user) return null;
  return { id: session.user.id, email: session.user.email, name: session.user.name };
}
