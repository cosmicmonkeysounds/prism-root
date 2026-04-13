# react-shell/

React shell components for Prism apps. Wires the framework-agnostic `@prism/core/lens` primitives (`LensRegistry`, `ShellStore`) to concrete React components and provides ready-made document / form / CSV / report surfaces plus a print renderer.

```ts
import { ShellLayout, LensProvider } from "@prism/core/shell";
```

## Key exports

- `LensProvider` — React context provider binding a `LensRegistry`, a `ShellStore`, and a `LensComponentMap` (lens ID → React component).
- `useLensContext()` / `useShellStore()` — access the context and reactively read the Zustand shell store.
- `ShellLayout` — top-level IDE-style layout: `ActivityBar` + sidebar + `TabBar` + active lens content.
- `ActivityBar`, `TabBar` — composable shell primitives.
- `DocumentSurface` — renders a `PrismFile` via the language registry, with pluggable `CustomSurfaceProps`.
- `FormSurface` — form renderer over `DocumentSchema` / `FormState`.
- `CsvSurface` — CSV/table surface.
- `ReportSurface` — report surface.
- `renderForPrint`, `triggerBrowserPrint`, `buildPageCss`, `resolvePageSize` — print pipeline.
- Types: `LensComponentMap`, `LensContextValue`, `LensProviderProps`, `DocumentSurfaceProps`, `CustomSurfaceProps`, `FormSurfaceProps`, `CsvSurfaceProps`, `ReportSurfaceProps`.

## Usage

```tsx
import { LensProvider, ShellLayout } from "@prism/core/shell";

function App({ registry, store, components }: AppProps) {
  return (
    <LensProvider registry={registry} store={store} components={components}>
      <ShellLayout />
    </LensProvider>
  );
}
```
