//! Control-plane data-layer tests against a real Postgres.
//!
//! These are gated on DB connectivity: with no reachable database (the
//! default in CI) every case skips, so `pnpm test` stays green offline. To
//! run them, point `DATABASE_URL` at a Postgres and re-run — e.g.
//! `DATABASE_URL=postgres://127.0.0.1:5432/loom_dev pnpm --filter @loom/core test db`.

import { randomUUID } from 'node:crypto'

import { afterAll, beforeAll, describe, expect, it } from 'vitest'

import { dbReady, initSchema, pool } from '../server/db/index.ts'
import { createEvent, createProject, projectSource, upsertFile } from '../server/db/queries.ts'

const OWNER = `test-owner-${randomUUID()}`
let hasDb = false

beforeAll(async () => {
  hasDb = await dbReady()
  if (hasDb) await initSchema()
})

afterAll(async () => {
  if (hasDb) {
    // Cascades to project_file + event rows.
    await pool().query('delete from project where owner_id = $1', [OWNER])
    await pool().end()
  }
})

function codes(prefix: string) {
  return { event: `${prefix}E1`, prime: `${prefix}P1`, mod: `${prefix}M1` }
}

describe('control-plane data layer (Postgres)', () => {
  it('projectSource puts main.loom first and headers each file', async (ctx) => {
    if (!hasDb) return ctx.skip()
    const p = await createProject(OWNER, 'Source Test')
    // Insert out of order + with a path that sorts before "main".
    await upsertFile(p.id, 'cast/villain.loom', 'CHARACTER Villain\n')
    await upsertFile(p.id, 'main.loom', 'TITLE: Demo\n')

    const src = await projectSource(p.id)
    expect(src).not.toBeNull()
    const body = src!.source
    // main.loom leads despite "cast/…" sorting first alphabetically.
    expect(body.indexOf('main.loom')).toBeLessThan(body.indexOf('cast/villain.loom'))
    expect(body).toContain('# ── main.loom ')
    expect(body).toContain('TITLE: Demo')
    expect(body).toContain('CHARACTER Villain')
  })

  it('projectSource is null for a project with no files', async (ctx) => {
    if (!hasDb) return ctx.skip()
    const p = await createProject(OWNER, 'Empty')
    expect(await projectSource(p.id)).toBeNull()
  })

  it('enforces at most one active event per project', async (ctx) => {
    if (!hasDb) return ctx.skip()
    const p = await createProject(OWNER, 'One Active')
    const base = {
      project_id: p.id,
      mode: 'live' as const,
      status: 'open' as const,
      scenario_name: 'x',
      scenario_source: 'TITLE: x\n',
    }
    await createEvent({ id: randomUUID(), ...base, ...codeFields('AAA') })
    // A second active event for the same project violates the partial unique index.
    await expect(createEvent({ id: randomUUID(), ...base, ...codeFields('BBB') })).rejects.toThrow()
  })

  it('rejects a duplicate event code across live events', async (ctx) => {
    if (!hasDb) return ctx.skip()
    const p1 = await createProject(OWNER, 'Codes A')
    const p2 = await createProject(OWNER, 'Codes B')
    const common = { mode: 'live' as const, status: 'open' as const, scenario_name: 'x', scenario_source: 'y' }
    await createEvent({ id: randomUUID(), project_id: p1.id, ...common, event_code: 'DUP111', prime_code: 'DUP222', mod_code: 'DUP333' })
    await expect(
      createEvent({ id: randomUUID(), project_id: p2.id, ...common, event_code: 'DUP111', prime_code: 'NEW222', mod_code: 'NEW333' }),
    ).rejects.toThrow()
  })
})

function codeFields(prefix: string) {
  const c = codes(prefix)
  return { event_code: c.event, prime_code: c.prime, mod_code: c.mod }
}
