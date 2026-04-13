# kbar/

KBar command palette with focus-depth routing. Actions are registered at one of four depths (`global` / `app` / `plugin` / `cursor`); the palette surfaces actions for the current depth and everything above it.

```ts
import { PrismKBarProvider, usePrismKBar, createActionRegistry } from "@prism/core/kbar";
```

## Key exports

- `createActionRegistry()` — build an `ActionRegistry` that registers KBar `Action`s per `FocusDepth` and subscribes to changes. Returns `{ register, getActions, subscribe }`.
- `PrismKBarProvider` — React provider wrapping `KBarProvider` + a `PrismKBarContext`. Binds the registry, the current depth, and CMD+K.
- `usePrismKBar()` — hook returning `{ registry, currentDepth, setDepth }`.
- Types: `FocusDepth` (`"global" | "app" | "plugin" | "cursor"`), `PrismAction`, `ActionRegistry`, `PrismKBarProviderProps`.

## Usage

```tsx
import { PrismKBarProvider, usePrismKBar } from "@prism/core/kbar";

function AppShell({ children }: { children: React.ReactNode }) {
  return (
    <PrismKBarProvider>
      <ActionRegistrar />
      {children}
    </PrismKBarProvider>
  );
}

function ActionRegistrar() {
  const { registry, setDepth } = usePrismKBar();
  React.useEffect(() => {
    return registry.register("global", [
      { id: "new-file", name: "New File", perform: () => {} },
    ]);
  }, [registry]);
  return null;
}
```
