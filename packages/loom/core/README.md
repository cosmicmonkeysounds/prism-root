# @loom/core

Native **TypeScript** port of the Loom engine — **no WASM, no Prism**.
It is the foundation the participant app ([`loom-play`](../play)) runs on
and the long-term replacement for the wasm bundle the editor consumed.

Loom is a language for **social-ecosystem simulation** — LARPs,
interactive installations, multiplayer games. A `.loom` file looks like
a screenplay (ALL-CAPS speaker cues, indented dialogue) but underneath it
is a reactive program: factions, locations, characters with per-person
relationships, hooks that fire on real-world events, and an autonomous
clock.

## What's here

```
src/
  parser/        # full TS port of the Rust parser — lexer, AST, decl
                 #   bodies, diagnostics, keyword table (see ./parser)
  runtime/
    expr.ts      # the World scope + Value engine + expression parser
    ledger.ts    # the narrative event ledger
    bundle.ts    # compiled project + ITEM/FACTION/CHARACTER `is` merges
    sim/         # the social-ecosystem runtime (see sim/DESIGN.md)
server/
  server.ts      # the LAN event server (SSE + REST, hosts one live Sim)
  views.ts       # pure per-role projections (guest / mod / performer)
  public/        # the vanilla operator console served at /console
examples/
  escape-the-internet.loom   # the reference scenario
test/            # vitest suites, ported 1:1 from the Rust #[cfg(test)]
```

The minimal runtime (`runtime/sim`) is **World state (dotted paths) + an
event ledger + a reactive hook engine drained to a fixpoint + a directive
effect vocabulary + an external input API + an autonomous clock**. Hooks
and beats share one executor. Full design + the "Escape the Internet"
walkthrough: [`src/runtime/sim/DESIGN.md`](./src/runtime/sim/DESIGN.md).

## Install

These packages share one install — run it once from the loom root:

```bash
cd packages/loom && pnpm install
```

## Commands

Run from `packages/loom/core` (or `pnpm --filter @loom/core <script>` from
the loom root):

| Command          | What it does                                            |
|------------------|---------------------------------------------------------|
| `pnpm test`      | vitest — the full suite (parser + runtime/sim + views)  |
| `pnpm typecheck` | `tsc --noEmit`                                           |
| `pnpm serve`     | boot the **event server** (tsx) — see below             |

## The event server

`pnpm serve` hosts a single live `Sim` and serves the participant app +
operator console to clients on the local network. Transport is
Server-Sent Events (push) + `fetch` POST (actions) — no WebSocket
dependency, so it works on any phone browser on the wifi.

```
http://<lan-ip>:7000           → the participant app (built loom-play)
http://<lan-ip>:7000/console   → the vanilla operator console (mods)
```

- **Mods** open/close the doors (`idle → open → paused`); guest and
  performer actions return `409` while not "open". While open the server
  ticks the sim once a second so ambient generators + time-driven hooks
  advance.
- `/` serves the built `../play/dist`. If the app isn't built it falls
  back to the console. Override the dist path with `LOOM_APP_DIST`.

### Passcodes (logging in)

Three independent passcodes gate the three roles:

| Role          | Passcode            | Where it's used                          |
|---------------|---------------------|------------------------------------------|
| 🎟️ Guest      | **event code**      | required to register / join the event    |
| 🎭 Performer  | **performer code**  | sign in as a character (scan guests)     |
| 🛡️ Moderator  | **moderator code**  | the operator console (open/close doors)  |

Each is taken from `LOOM_EVENT_PASS` / `LOOM_PRIME_PASS` / `LOOM_MOD_PASS`
if set; **otherwise a short, speakable code is generated** (six chars, no
`0/O/1/I/L`) and **persisted**, so it stays stable across restarts. All
three are printed in the boot banner — that terminal is the trusted
channel the operator reads them from. After signing in, the moderator's
console shows the event + performer codes (with a join-QR) to hand out to
the room. Guests can also arrive via a `?code=<event-code>` link (what the
QR encodes), which pre-fills the field. Matching is trimmed + case-insensitive.

### Persistence (surviving restarts & drops)

The sim is fully deterministic, so the server **event-sources** every
mutation: each `createPerson` / `join` / `choose` / `tick` / … is journaled
to `LOOM_STATE_DIR` (default `server/.loom-state/`) alongside the scenario,
phase, the passcodes, and live session tokens. On boot it replays the
journal into a fresh sim and rehydrates sessions — so a crash, laptop
sleep, or Ctrl-C is transparent: **nobody re-authenticates and nobody
loses their faction / score / place.** A wifi blip is handled client-side
(the SSE stream auto-reconnects and the next snapshot rehydrates the UI).
`mod load` / `mod reset` start a fresh timeline (clear the journal).

Environment: `LOOM_PORT` (7000), `LOOM_HOST` (0.0.0.0), `LOOM_EVENT_PASS`,
`LOOM_PRIME_PASS`, `LOOM_MOD_PASS` (any unset code is auto-generated),
`LOOM_STATE_DIR`, `LOOM_APP_DIST`.

For the participant app's dev/build flow (HMR), see [`../play`](../play).

## Running a live event

```bash
# build the participant app once, then run the server (one origin)
pnpm --filter loom-play build
pnpm --filter @loom/core serve
# → open http://<lan-ip>:7000/console, sign in as a mod, hit Start
# → guests open http://<lan-ip>:7000 on their phones
```

## Status

The parser is a complete, test-verified 1:1 port of the Rust parser. The
runtime is the first-principles **ecosystem sim** (the playhead/coroutine
1:1 port is deferred in favour of it). `lsp` / `doc` surfaces are not yet
ported.
