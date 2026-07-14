# loom-stagehand

The show-control bridge for Loom live events — the daemon that connects
the story brain (the `@loom/core` event server) to the physical house:
TouchDesigner projection machines (OSC), props and sensors (MQTT), and
eventually the vision workers and the Upside Down VR feed. Design:
[`docs/dev/loom-show-control.md`](../../../docs/dev/loom-show-control.md).

Two translations, both declarative in `show.yaml`:

- **story → world (the cue map).** Stagehand joins the event's SSE feed
  as a mod and watches raw sim events. Authored directives the engine
  doesn't handle natively (`<cue: …>`, `<prop: …>`, `<vibe: …>`) arrive
  as `{type: "directive", verb, args}` envelopes — cue rules match them
  (or any raw event, e.g. `beatEntered`) and emit OSC messages and/or
  MQTT publishes.
- **world → story (the sensor map).** Stagehand subscribes to MQTT
  sensor topics; rules gate on a `when:` condition and inject journaled
  story mutations through the mod API: `signal` / `beat` /
  `arrive` (a location move — `POST /api/mod/set {field: "location"}`).

The router is stateless: restart it any time. Retained MQTT re-converges
props; the journal carries the story; debounce state is the only loss.

## Run

```bash
cd packages/loom/stagehand
cp show.example.yaml show.yaml   # edit: server url + passcode, broker, OSC targets
uv run stagehand check --config show.yaml
uv run stagehand run   --config show.yaml   # -v for debug logging
```

Prereqs: a Mosquitto broker (`brew install mosquitto`), the event server
(`pnpm --filter @loom/core serve`), and NTP-synced cue receivers if you
use `t_exec`.

## Wire contracts (core/server/event-runtime.ts)

- SSE feed: `GET {url}[/e/:event]/events?role=mod&id=stagehand` —
  frames `snapshot` / `history` / `message` / `sim`; stagehand consumes
  `sim` only.
- Mod auth: `x-loom-token` header; bootstrapped via
  `POST /api/mod/login {passcode}` when the config gives `mod_passcode`.
- Mutations: `POST /api/mod/signal {name, subject?}`,
  `POST /api/mod/beat {name, subject?}`,
  `POST /api/mod/set {id, field: "location", value}`.

> Note: the SSE stream itself grants the raw mod feed to any client that
> asks for `role=mod` — acceptable on the show's LAN island, but server-
> side hardening (tokened SSE) is an open item alongside the known
> `/api/history` IDOR.

## The `t_exec` contract

A cue with `t_exec: "+200ms"` is sent immediately with the **absolute
execution time appended as a final string argument** (epoch
milliseconds; a string because OSC int32 overflows and float32 is too
coarse). NTP-synced receivers schedule the cue for that instant — this
is how 8 projectors on two machines flip in the same frame. Receivers
that ignore the extra argument just fire ~200 ms early.

## MQTT conventions

| Topic | Direction | Notes |
|---|---|---|
| `sensors/<node>/<sensor>` | prop → story | JSON payloads; non-JSON lands as `{"value": …}` |
| `vision/<cam>/…` | vision → story | software-defined sensors (workers TBD) |
| `props/<prop>/…` | story → prop | publish **retained** so props re-converge |
| `show/scene`, `show/vibe` | story → all | retained global show state |
| `health/<node>` | LWT | stagehand announces on `health/stagehand` (`online`/`offline`/`lost`) |

Sensor topic patterns may capture segments by name —
`vision/{cam}/tamper` subscribes as `vision/+/tamper` and exposes
`{cam}` to `when:` conditions and templates.

## Tests

```bash
uv run pytest
```

Pure units — directive parsing, conditions, topic patterns, both maps,
the SSE parser, config validation. The router's socket wiring is
exercised live against Mosquitto + the event server (see the blinking-
LED milestone in the design doc), not in unit tests.
