//! Postgres access for the control plane (authors, projects, events).
//!
//! The BetterAuth tables (`user` / `session` / `account` / `verification`)
//! are owned + migrated by BetterAuth itself. This module owns the *domain*
//! tables that hang off an author account:
//!
//!   project        one `.loom` app an author develops
//!   project_file   a single `.loom` file inside a project
//!   event          a launched run of a project (live or preview)
//!
//! `owner_id` references a BetterAuth user id but is deliberately *not* a hard
//! foreign key, so the two migration sources stay decoupled and order-free.

import { Pool } from "pg";

import { DATABASE_URL } from "../config.ts";

let _pool: Pool | null = null;

/** The shared connection pool (also handed to BetterAuth). Lazily created. */
export function pool(): Pool {
  if (_pool === null) {
    _pool = new Pool({ connectionString: DATABASE_URL, max: 10 });
  }
  return _pool;
}

/** Domain schema — idempotent, safe to run on every boot. */
const SCHEMA = `
create table if not exists project (
  id          text primary key,
  owner_id    text not null,
  name        text not null,
  slug        text not null,
  created_at  timestamptz not null default now(),
  updated_at  timestamptz not null default now(),
  unique (owner_id, slug)
);
create index if not exists project_owner_idx on project (owner_id);

create table if not exists project_file (
  id          text primary key,
  project_id  text not null references project (id) on delete cascade,
  path        text not null,
  content     text not null default '',
  updated_at  timestamptz not null default now(),
  unique (project_id, path)
);
create index if not exists project_file_project_idx on project_file (project_id);

create table if not exists event (
  id               text primary key,
  project_id       text not null references project (id) on delete cascade,
  mode             text not null check (mode in ('live', 'preview')),
  status           text not null check (status in ('idle', 'open', 'paused', 'ended')),
  event_code       text not null,
  prime_code       text not null,
  mod_code         text not null,
  scenario_name    text not null,
  scenario_source  text not null,
  created_at       timestamptz not null default now(),
  ended_at         timestamptz
);
create index if not exists event_project_idx on event (project_id);
-- Passcodes are unique across live events so a code resolves to one event.
create unique index if not exists event_event_code_uq on event (event_code) where status <> 'ended';
create unique index if not exists event_prime_code_uq on event (prime_code) where status <> 'ended';
create unique index if not exists event_mod_code_uq on event (mod_code) where status <> 'ended';
-- At most one active (non-ended) event per project.
create unique index if not exists event_one_active_per_project on event (project_id) where status <> 'ended';
`;

/** Create the domain tables if they don't exist. Throws if the DB is down. */
export async function initSchema(): Promise<void> {
  await pool().query(SCHEMA);
}

/** Can we reach the database at all? Used to gate the control plane at boot. */
export async function dbReady(): Promise<boolean> {
  try {
    await pool().query("select 1");
    return true;
  } catch {
    return false;
  }
}
