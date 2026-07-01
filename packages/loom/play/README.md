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

The server is multi-tenant, so joining is a two-step bootstrap: the code a
guest/performer types is first resolved with `POST /api/resolve-code {code}`
to its `{ eventId, role }`, then every SSE/REST call is scoped to
`/e/:eventId/…` (the `eventId` is stored alongside the session in
`localStorage`). Sessions from before this change fall back to a `default`
event. See `src/session.ts` / `src/client.ts`. Nothing about the guest
experience changed — it's still just name + event code.

Messages are **composed server-side** (see `core/server/chat.ts`) and
routed to channels, so the client just renders. That gives three things
for free: full **history on re-login** (the server replays every thread
since the event began), **moderation** (admins hide/show messages), and a
single source of truth for narrative copy.

## Channels, spaces & threads

Channels are grouped into **spaces** — the Discord-style sidebar sections.
Story-driven channels live in the built-in `internet` space (the performer
also gets `booth` + `guests` sections); **authored `SPACE`/`CHANNEL`
declarations** (see [`core`](../core)) add their own spaces alongside. Each
message lands in one channel kind:

- **🌐 The Internet** (`lobby`) — ambient story barks, global broadcasts,
  your own personal beats, and open-room chat.
- **#faction** (`faction:<Id>`) — a faction-scoped broadcast / room, members
  only.
- **DM** (`dm:<Character>`) — a character speaking directly to you.
- **authored rooms** (`room:<name>`) — an `open` / `private` / `faction` /
  `group` / `dm` channel declared in `.loom`, grouped under its `SPACE`.

**Access control.** Visibility follows the channel kind: `open` rooms show
for everyone, `faction` rooms for that faction's members, and
`private`/`group`/`dm` rooms only for explicit members. The server snapshot
(`GuestView.channels`) lists exactly the rooms you can see — empty rooms
included. Membership changes through **invites** (`/api/*/channel/invite`,
and guests invite each other from the roster picker) and **leave**
(`/api/*/channel/leave`), both journaled so they replay; a join posts a
member-scoped "📥 … joined the room" notice.

**Channel-type rules.** Each authored channel carries `canPost` + `threadable`
in its snapshot (resolved from the `core` channel-type registry). A read-only
room (`post: none`, e.g. `#announcements`) hides the composer; a
non-threadable room hides the reply affordance. Broadcasts mirror into rooms
that subscribe (`routes: *`). A `slow:` room 429s a too-soon post; an
`ephemeral:` room's messages vanish (a `messageExpired` SSE event drops them
from the view). The guest invite picker is fed a **scoped roster** (only
faction-mates + people you share a gated room with), not the whole event.

**Typed chat (hybrid).** Participants type into a channel via a composer
(`/api/guest/say` · `/api/prime/say`); the message is journaled as a sim
`chat` event so it replays deterministically alongside the story. Audience is
the channel's current members (open → everyone).

**Threads (Slack-style).** A line message can be replied to: a reply carries
the root's `seq` as `parentSeq`, the channel shows a *"N replies"*
affordance, and opening it shows the thread panel (root + replies + a reply
composer). Threading is single-level.

**Sender runs (Slack/Discord).** Consecutive `line`s from one sender within
five story-minutes coalesce under a single colored screen-name banner;
continuation lines tuck under it.

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
| `pnpm test`      | vitest (pure threads/grouping helpers)|

## Layout

```
src/
  types.ts      # mirror server/chat.ts (ChatMessage incl. parentSeq / Channel incl. spaceId)
  client.ts     # api() POST + useChatStream (SSE → de-duped message map)
  threads.ts    # group→channels + spaces (groupBySpace) + sender runs (groupRuns) + thread selectors
  chat.tsx      # primitives: SpaceList/ChannelView/MessageGroup/MessageThread/Composer/…
  session.ts    # useGuestSession / usePrimeSession (channels + decisions + actions + say())
  guest.tsx     # the guest inbox app
  performer.tsx # the performer / booth app
  ui.tsx        # the role chooser
  main.tsx      # entry
  styles.css    # the AOL-chatroom skin (Win95 bevels, navy bars, serif lines)
test/           # vitest: grouping (sender runs) · spaces · thread selectors
index.html      # Vite entry
vite.config.ts  # react plugin + dev proxy to LOOM_SERVER (default :7000)
```

The guest and performer apps are thin: both assemble the same role-agnostic
`chat.tsx` primitives over their own channel-shaping (`session.ts`) and a
footer (decision tray / scanner / moderation), so new surfaces compose
without duplication.

> Note: `BarcodeDetector` (camera QR scanning) is Chrome/Android only;
> iOS performers use the always-present manual id field.
