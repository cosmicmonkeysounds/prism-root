# App Shell

The **App Shell** wraps a multi-page app with persistent navigation chrome. Unlike Page Shell, which frames a single page, App Shell is designed to host **many pages** and keep the user oriented as they move between them.

## Regions

- **header** — top strip with site name, global search, and user menu.
- **side-nav** — persistent vertical navigation between top-level destinations.
- **main** — the current page's content. Typically contains a Page Shell.
- **footer** — site-wide footer (copyright, links, etc).

## App Shell vs Page Shell

| | App Shell | Page Shell |
|-|-|-|
| Scope | Whole app | One page |
| Lives at | Workspace root | Inside a page |
| Contains | Multiple pages | Blocks |
| Use for | Global chrome | Per-page regions |

## Composing them

Put an App Shell at the workspace level and a Page Shell inside its `main` slot. Each page you create slots into the Page Shell, and the App Shell stays mounted across navigation.

```
App Shell
├── header
├── side-nav
├── main
│   └── Page Shell
│       ├── header
│       ├── sidebar
│       └── main ← your blocks
└── footer
```

## Tips

- Put site-level navigation in the App Shell's `side-nav`, not inside a Page Shell.
- Leave App Shell's `main` empty in the builder — Prism fills it with the active page at render time.
- Resize the `side-nav` width by dragging its right edge. The commit happens on release.
