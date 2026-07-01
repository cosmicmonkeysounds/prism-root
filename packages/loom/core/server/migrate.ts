//! One-shot migration runner: create/upgrade the BetterAuth tables and the
//! domain schema (project / project_file / event). Idempotent.
//!
//! Run: `pnpm --filter @loom/core migrate` (uses DATABASE_URL, default
//! `postgres://127.0.0.1:5432/loom_dev`).

import { migrateAuth } from "./auth-server.ts";
import { initSchema, pool } from "./db/index.ts";
import { DATABASE_URL } from "./config.ts";

async function main(): Promise<void> {
  process.stdout.write(`Migrating ${DATABASE_URL} …\n`);
  await migrateAuth();
  process.stdout.write("  ✓ BetterAuth tables (user / session / account / verification)\n");
  await initSchema();
  process.stdout.write("  ✓ domain tables (project / project_file / event)\n");
  await pool().end();
  process.stdout.write("Done.\n");
}

main().catch((err) => {
  process.stderr.write(`Migration failed: ${String(err)}\n`);
  process.exit(1);
});
