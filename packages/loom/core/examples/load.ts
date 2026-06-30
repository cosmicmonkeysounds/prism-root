//! Loader for multi-file `.loom` example projects.
//!
//! An example "project" is a directory under `examples/` (e.g.
//! `escape-the-internet/`) holding a `main.loom` plus any number of
//! sibling / sub-directory `.loom` files. `main.loom` is always loaded
//! first because the runtime takes `entry:` and the default `ROLE` from
//! the earliest file it sees (`compileModel` in `runtime/sim/model.ts`).
//!
//! Two views of a project are exposed:
//!   - `scenarioFiles(name)` → the per-file `{ path, source }` array, fed
//!     straight to `Sim.fromSources(...files)` to exercise the real
//!     multi-file `Bundle` path (independent parse per file).
//!   - `scenarioSource(name)` → the same files concatenated into one
//!     string. The two compile to an identical `SimModel`, so the event
//!     server keeps persisting a single `scenarioSource` for its
//!     deterministic journal-replay (see `server/store.ts`) while authors
//!     still split the world across many files.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join, relative, sep } from "node:path";

export interface ScenarioFile {
  /** Project-relative path (`main.loom`, `cast/algorithm.loom`). */
  path: string;
  source: string;
}

/** Recursively collect every `.loom` file under `dir`, absolute paths. */
function walkLoom(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...walkLoom(full));
    else if (name.endsWith(".loom")) out.push(full);
  }
  return out;
}

/**
 * `main.loom` ranks first; everything else sorts by path so the
 * concatenation (and therefore the compiled model) is deterministic.
 */
function ordered(root: string): string[] {
  return walkLoom(root).sort((a, b) => {
    const am = a.endsWith(`${sep}main.loom`) ? 0 : 1;
    const bm = b.endsWith(`${sep}main.loom`) ? 0 : 1;
    return am - bm || (a < b ? -1 : a > b ? 1 : 0);
  });
}

/** The directory holding a named example project. */
export function scenarioDir(name: string): string {
  return fileURLToPath(new URL(`./${name}/`, import.meta.url));
}

/** Every `.loom` file in the project as `{ path, source }`, main first. */
export function scenarioFiles(name: string): ScenarioFile[] {
  const root = scenarioDir(name);
  return ordered(root).map((full) => ({
    path: relative(root, full).split(sep).join("/"),
    source: readFileSync(full, "utf8"),
  }));
}

/**
 * The project's files concatenated into one source string (main first).
 * Equivalent to bundling the files separately — the parser tolerates a
 * later file's comment header mid-stream and `entry:` / default `ROLE`
 * still resolve from `main.loom` because it leads.
 */
export function scenarioSource(name: string): string {
  return scenarioFiles(name)
    .map((f) => `# ── ${f.path} ${"─".repeat(Math.max(0, 60 - f.path.length))}\n${f.source}`)
    .join("\n\n");
}
