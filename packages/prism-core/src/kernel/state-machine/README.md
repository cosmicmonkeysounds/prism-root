# state-machine

Flat finite state machine primitives. `Machine` is context-free (state is a plain string tag), observable (`on()` subscribers), serializable (`toJSON()`), and supports guards, actions, lifecycle hooks (`onEnter`/`onExit`), wildcard `from: "*"` transitions, and terminal states. Also exports the XState-based `toolMachine` used by interaction layers for tool-mode routing.

Exposed under three aliases for historical/path compatibility: `@prism/core/automaton`, `@prism/core/machines`, `@prism/core/state-machine`.

```ts
import { createMachine } from "@prism/core/automaton";
```

## Key exports

- `Machine<TState, TEvent>` — class-form FSM with `state`, `matches`, `can`, `send`, `on`, `toJSON`, and lifecycle hooks.
- `createMachine(def, options?)` — functional constructor returning a `Machine`.
- `toolMachine` / `createToolActor()` / `getToolMode(actor)` — XState actor for tool-mode routing.
- Types: `StateNode`, `Transition`, `MachineDefinition`, `MachineListener`, `ToolMode`, `ToolEvent`.

## Usage

```ts
import { createMachine } from "@prism/core/automaton";

type State = "idle" | "loading" | "ready";
type Event = "LOAD" | "DONE";

const machine = createMachine<State, Event>({
  initial: "idle",
  states: [{ id: "idle" }, { id: "loading" }, { id: "ready" }],
  transitions: [
    { from: "idle", event: "LOAD", to: "loading" },
    { from: "loading", event: "DONE", to: "ready" },
  ],
});

machine.on((state, event) => console.log(event, "->", state));
machine.send("LOAD");
machine.send("DONE");
```
