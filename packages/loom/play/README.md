# loom-play

The **participant app** for live Loom events — the React client guests
and performers use. It's skinned like an **old-school AOL chat room**
(teal Win95 desktop, navy title bars, a sunken white transcript of
Times-New-Roman line messages with colored screen names, chunky 3D
buttons, a "People Here" buddy list), but the structure underneath is
still threaded: every conversation is its own **channel (thread)** in an
inbox, and each running instance streams its participant's slice of the
live `Sim`.

It talks to the [`@loom/core`](../core) event server over Server-Sent
Events (push) + `fetch` POST (actions). Built with Vite + React 18, the
same way the [`editor`](../editor) is.

Messages are **composed server-side** (see `core/server/chat.ts`) and
routed to channels, so the client just renders. That gives three things
for free: full **history on re-login** (the server replays every thread
since the event began), **moderation** (admins hide/show messages), and a
single source of truth for narrative copy.

## Channels

Every message lands in one of three channel kinds:

- **🌐 The Internet** (`lobby`) — ambient story barks, global broadcasts,
  and your own personal beats (*"⛓️ You've been dragged into the
  Internet."*).
- **#faction** (`faction:<Id>`) — a faction-scoped broadcast, members
  only.
- **DM** (`dm:<Character>`) — a character speaking directly to you.

**Decisions dock in a thread.** A recruiter's `<choice>` appears as
quick-reply buttons inside *that recruiter's* DM; world choices (pick a
side, escape) sit in the lobby. Any thread with an unanswered decision
badges and sorts to the top, so a choice left unmade always pulls focus.

## Roles

- **🎟️ Guest** — register with a name + the host's **event code** (or
  arrive via a `?code=` link / QR that pre-fills it) → an inbox of
  conversation threads with a status HUD (faction · score · location ·
  captured) and a profile sheet (QR pass · defect · leave). Re-login
  restores every thread.
- **🎭 Performer** — sign in as a character → the *same* threaded surface,
  recomposed for the booth: a pinned **Scanner** thread (camera via
  `BarcodeDetector`, or manual id) + scan readouts, a read-only
  **broadcast feed**, and one **conversation per guest** to scan into. An
  admin (performer who also enters the moderator passcode) additionally
  gets **hide/show** on every message and capture/release in each guest
  thread.

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
  types.ts      # mirror server/chat.ts (ChatMessage/Channel) + view snapshots
  client.ts     # api() POST + useChatStream (SSE → de-duped message map)
  threads.ts    # group messages → channels; unread + active-thread nav
  chat.tsx      # composable primitives: ChannelList/ChannelView/Message/…
  session.ts    # useGuestSession / usePrimeSession (channels + decisions + actions)
  guest.tsx     # the guest inbox app
  performer.tsx # the performer / booth app
  ui.tsx        # the role chooser
  main.tsx      # entry
  styles.css    # the AOL-chatroom skin (Win95 bevels, navy bars, serif lines)
index.html      # Vite entry
vite.config.ts  # react plugin + dev proxy to LOOM_SERVER (default :7000)
```

The guest and performer apps are thin: both assemble the same role-agnostic
`chat.tsx` primitives over their own channel-shaping (`session.ts`) and a
footer (decision tray / scanner / moderation), so new surfaces compose
without duplication.

> Note: `BarcodeDetector` (camera QR scanning) is Chrome/Android only;
> iOS performers use the always-present manual id field.
