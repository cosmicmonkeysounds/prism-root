# input

Keyboard shortcut parsing, scoped input routing, and undo/redo wiring. `KeyboardModel` holds shortcut → action bindings; `InputScope` groups a KeyboardModel with action handlers and an optional `UndoHook`; `InputRouter` maintains a stack of scopes and dispatches key events to the topmost matching handler.

## Import

```ts
import {
  KeyboardModel,
  InputScope,
  InputRouter,
  parseShortcut,
  normaliseKeyEvent,
  keyToShortcut,
} from "@prism/core/input";
```

## Key exports

- `parseShortcut("cmd+shift+p")` — parses a string into `NormalisedKey` (`cmd` covers both Meta and Ctrl).
- `normaliseKeyEvent(event)` — converts a DOM-ish `KeyEventLike` into `NormalisedKey`.
- `keyToShortcut(normalised)` — renders a `NormalisedKey` back to a shortcut string.
- `KeyboardModel` — `bind`/`bindAll`/`unbind`/`applyShortcutMap`/`resolve`/`allBindings`.
- `InputScope` — wraps a `KeyboardModel` with action handlers; ships with `cmd+z`/`cmd+shift+z`/`escape` pre-bound.
- `InputRouter` — `push`/`pop`/`replace` scopes; `handleKeyEvent` routes top-down, async `dispatch(action)` invokes by name.
- Types: `NormalisedKey`, `KeyEventLike`, `UndoHook`, `InputRouterEvent`, `InputRouterListener`.

## Usage

```ts
import { InputScope, InputRouter } from "@prism/core/input";

const scope = new InputScope("editor", "Editor");
scope.keyboard.bind("cmd+s", "save");
scope.on("save", async () => saveDocument());

const router = new InputRouter();
router.push(scope);

await router.handleKeyEvent({ key: "s", ctrlKey: true, metaKey: false, shiftKey: false, altKey: false });
```
