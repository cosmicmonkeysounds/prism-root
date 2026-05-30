# Loom Runtime / Simulator

A small PySide6 GUI that drives `loom-play` (the Loom runtime CLI) so
friends can play through any `.loom` project without setting up the
full editor / relay stack.

## Setup

```
# 1. Install the only Python dep (one-time).
pip install pyside6

# 2. Launch via the prism CLI — builds loom-play and execs the GUI:
cargo run -p prism-cli -- loom sim
cargo run -p prism-cli -- loom sim packages/loom/examples/bell-tower-live
cargo run -p prism-cli -- loom sim --ship    # release-profile driver
```

The CLI subcommand is the recommended entry point. Manual path
(skips the build step, useful for hacking on the Python side):

```
cargo build -p loom-runtime --bin loom-play
python packages/loom/simulator/simulator.py [project-dir]
```

With no argument the simulator opens `packages/loom/examples/saltmere`
if it exists; otherwise click **Open…** in the top bar.

## What you see

- **Transcript** (left) — the playable story: scene headings, action,
  dialogue, choices.
- **Choice buttons** — appear under the transcript whenever the
  playhead hits a `*` block. Click to advance.
- **Ledger tab** — every `Event` envelope the runtime emits, tagged
  with its track id `[tN]`, its ledger index `#N`, and a `← #N`
  back-pointer to its cause envelope. Great for debugging directives,
  world writes, knowledge changes, scene spawns, and cross-track
  hook firings.
- **Timeline tab** — multitrack canvas (loom-editor.html §4.2
  Arrangement view). Rows are Mesh tracks (Booth at top, then Main,
  then ROLEs, then PERSONs, then ambient generators). Each ledger
  envelope renders as a small block on its track row, colour-coded
  by event kind. Cross-track cause edges (e.g. a Booth `<cast:>`
  directive firing a `HookFired` on Wren's row) draw as thin red
  arcs. Pan with click-drag; Ctrl+scroll to zoom.
- **World tab** — live key/value snapshot of the `World` scope after
  every `WorldSet` / `KnowledgeChanged` / `LetEvaluated`.
- **Graph tab** — Obsidian-style force-directed entity graph
  (characters / locations / cohorts / beats). Independent of the
  timeline; useful for compose-mode browsing.
- **Diagnostics tab** — parser + project diagnostics from load time.

## Booth controls

- **Restart** — rewinds to the entry beat by relaunching the
  `loom-play` subprocess. Use this to replay from the start with a
  clean ledger and world; `Reload` preserves both.
- **Reload** — re-loads the project directory from disk and
  hot-reloads the running playhead (`Playhead::booth_hot_reload`).
  Ledger + world are preserved.
- **Skip Beat** — skips the current beat (`booth_skip_beat`).
- **force directive** — type something like `sfx: thunder` and press
  **Fire** to inject a directive through `booth_force_directive`.

## Protocol

The Python side never parses `.loom` or evaluates anything — it just
shuttles JSON lines over the `loom-play` subprocess's stdio. See the
docstring at the top of `runtime/src/bin/loom_play.rs` for the message
schema if you want to write your own client.
