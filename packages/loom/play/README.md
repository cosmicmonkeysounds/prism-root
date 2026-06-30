# loom-play

The **participant app** for live Loom events — the React client guests
and performers use. Each running instance is one node in the distributed
story: it streams its participant's slice of the live `Sim` and renders it
as a **flowing, branching game-dialogue timeline**.

It talks to the [`@loom/core`](../core) event server over Server-Sent
Events (push) + `fetch` POST (actions). Built with Vite + React 18, the
same way the [`editor`](../editor) is.

## Roles

- **🎟️ Guest** — register with a name + the host's **event code** (or
  arrive via a `?code=` link / QR that pre-fills it) → a status HUD
  (faction · score ·
  location · captured) over an animated story feed, with a **choice tray**
  that drives the branch: choose your side, answer a recruiter, escape
  when captured, or defect. Plus a personal QR pass for performers to
  scan. The story is composed from the event stream, with narrative beats
  synthesized from state transitions (e.g. *"⛓️ You've been dragged into
  the Internet."*) and ambient generator barks.
- **🎭 Performer** — sign in as a character → a scanner (camera via
  `BarcodeDetector`, or manual id) + a scan-response readout + a tappable
  list of guests present.

The operator/moderator surface is the server's console at `/console`
(not this app).

## Install

Shared install from the loom root (covers core, play, and editor):

```bash
cd packages/loom && pnpm install
```

## Develop (HMR)

Two processes, like the editor + relay:

```bash
# terminal 1 — the event server (API + SSE)
pnpm --filter @loom/core serve            # → http://localhost:7000

# terminal 2 — this app, with hot reload
pnpm --filter loom-play dev               # → http://localhost:5174
```

Open **:5174**. Vite proxies `/api`, `/events` (SSE) and `/console` to the
event server. `host: true` exposes the dev server on the LAN so phones can
load it too. Point it at a server on another machine with
`LOOM_SERVER=http://<ip>:7000 pnpm --filter loom-play dev`.

## Build (production)

```bash
pnpm --filter loom-play build             # → play/dist
```

The event server serves `play/dist` at `/` automatically (it resolves
`../play/dist`; override with `LOOM_APP_DIST`). So for a real event you
run a single process — `@loom/core serve` — on one origin, no Vite.

## Commands

| Command          | What it does                          |
|------------------|---------------------------------------|
| `pnpm dev`       | Vite dev server (:5174) with HMR      |
| `pnpm build`     | production build → `dist/`            |
| `pnpm preview`   | preview the built app                 |
| `pnpm typecheck` | `tsc --noEmit`                        |

## Layout

```
src/
  types.ts     # client view types (mirror server/views.ts) + Beat model
  session.ts   # useGuestSession / usePrimeSession — SSE → story timeline
  ui.tsx       # all components + the two role apps + the role chooser
  main.tsx     # entry
  styles.css   # the dark, game-like dialogue styling
index.html     # Vite entry
vite.config.ts # react plugin + dev proxy to LOOM_SERVER (default :7000)
```

> Note: `BarcodeDetector` (camera QR scanning) is Chrome/Android only;
> iOS performers use the always-present manual id field.
