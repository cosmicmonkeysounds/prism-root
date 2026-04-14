# Current Plan

## Admin Panels for Daemon / Relay / Studio (Complete — 2026-04-13)

All three Prism runtimes now ship live admin dashboards powered by `@prism/admin-kit`.

### What landed

1. **`@prism/admin-kit/html`** — `renderAdminHtml(options)` generates a self-contained HTML page with inline CSS/JS that polls a JSON endpoint and renders the admin dashboard. Used by Relay and Daemon. Exports individual widget renderers (`renderHealthBadge`, `renderMetricCard`, `renderUptimeCard`, `renderServiceList`, `renderActivityTail`).
2. **`createDaemonDataSource({ url })`** — new data source in admin-kit that talks to the daemon's HTTP transport. Tries `POST /invoke/daemon.admin` first, falls back to `/healthz` + `/capabilities`.
3. **Relay admin routes** (`/admin`, `/admin/api/snapshot`) — mounted in `relay-server.ts`, outside `/api/*` (no CSRF). SSR seed data for instant first paint. Shows health, uptime, modules, peers, federation, collections, portals, memory.
4. **Daemon admin module** (`admin_module.rs`) — `daemon.admin` command returning `AdminSnapshot` JSON. No feature gate. Installed last in `with_defaults()` to capture all modules. Builder exposes `.with_admin()`.
5. **Daemon HTTP admin routes** (`/admin`, `/admin/api/snapshot`) — added to the axum HTTP transport (`transport-http` feature). HTML served via `include_str!` templates.
6. **Studio admin panel** — already existed (Shift+A lens), documented in Studio CLAUDE.md.
7. **Unit tests** — 26 HTML renderer tests, 4 daemon data source tests, 5 relay admin route tests, 3 Rust admin module tests, 2 Rust HTTP admin tests = 40 new tests, all passing.
8. **E2E tests** — `packages/prism-relay/e2e/admin.spec.ts` (4 tests: HTML page, JSON snapshot, browser render, auto-refresh), `packages/prism-studio/e2e/admin.spec.ts` (4 tests: activity bar switch, header/source picker, Puck widgets, tab bar).

### Test status

- Admin-kit: 89 Vitest tests across 9 files
- Relay admin routes: 5 Vitest tests
- Daemon: 5 Rust tests (3 admin module + 2 HTTP transport)
- Clippy clean under `--features transport-http`

## Puck component registry + RecordList primitive (In progress — 2026-04-13)

ADR-004 introduces a DI seam for Puck builder components so new widgets can flow through a registered `PuckComponentProvider` instead of a hand-wired `if (def.type === …)` block in `layout-panel.tsx`'s 3000-line config `useMemo`. The first consumer is `RecordList` — a parametric list over kernel records driven by `ViewConfig` (filter/sort/limit) plus a lightweight row template — which is intended to eventually subsume the ten dynamic record widgets plus `list-widget` / `table-widget` / `card-grid-widget` / `report-widget`.

### What landed

1. **`@prism/core/bindings/puck/component-registry.ts`** — `PuckComponentProvider<TKernel>` interface + `PuckComponentRegistry<TKernel>` class with `register` / `registerAll` / `unregister` / `has` / `get` / `types` / `buildComponents`. Generic over kernel type so core stays decoupled from studio. Emits PascalCase Puck keys from kebab-case entity types.
2. **`record-list` entity** in `packages/prism-studio/src/kernel/entities.ts` — `component` category, `childOnly: true`, fields for `recordType`, `titleField`, `subtitleField`, `metaFields`, `filterExpression`, `sortField`, `sortDir`, `limit`, `emptyMessage`.
3. **`RecordListRenderer`** (`packages/prism-studio/src/components/record-list-renderer.tsx`) — pure renderer taking pre-resolved `GraphObject[]` + `ViewConfig` + `RecordListTemplate`, applying `@prism/core/view`'s existing `applyViewConfig` pipeline, and rendering title/subtitle/meta chips. `readRecordField` helper handles all shell fields + data payload.
4. **`recordListProvider`** (`packages/prism-studio/src/panels/puck-providers/record-list-provider.tsx`) — parses the compact `filterExpression` grammar (`status eq open; priority in high,urgent`) into `FilterConfig[]`, the `metaFields` string into `TemplateField[]`, and queries the kernel for objects matching the selected `recordType`. Uses the existing 12 `FilterOp` values from `@prism/core/view` — no parallel spec.
5. **`createStudioPuckRegistry()`** — factory in `panels/puck-providers/index.ts` seeding a fresh registry with built-in providers. `layout-panel.tsx` calls it inside the existing config `useMemo` and merges its `buildComponents()` output over the hand-wired components (registry wins). Additive only — no existing widgets were migrated.
6. **Tests — 47 new, all passing**: 12 registry tests, 12 renderer tests (resolveTemplateField / applyRecordListView), 23 provider tests (parseFilterExpression, parseMetaFields, buildTemplate, buildViewConfig).
7. **ADR-004 drafted** — `docs/adr/004-puck-component-registry-and-dynamic-content.md`. Proposed status. Captures the DI seam design, RecordList as the first parametric primitive, reuse of existing `ViewConfig`, and the migration shape for hand-wired widgets (deferred to incremental follow-up PRs).

### Status

- `pnpm --filter @prism/core typecheck` clean
- Full `pnpm vitest run packages/prism-core packages/prism-studio` — **3553 tests across 162 files passing** (47 new)
- Pre-existing `@prism/admin-kit/src/data-sources/daemon-data-source.ts` typecheck error is in an untracked in-progress file and unrelated

### Deferred

- Migrating the ~50 existing hand-wired `if (def.type === …)` blocks into providers (one-per-category PRs).
- `FilterBuilder` visual field for composing filter expressions without typing the string grammar.
- `Repeater` over array props (non-record content like nav items / pricing tiers).
- Wiring `createSavedViewRegistry` as a RecordList props source.
- Collapsing the ten dynamic record widgets into RecordList templates.

## Dynamic record widgets for Puck builder — Tier 1 (Complete — 2026-04-13)

The Puck layout panel offered only generic data widgets (list/table/kanban/card-grid/report) that worked over any record type. Dynamic "apps-flavoured" affordances — tasks, reminders, contacts, events, notes, goals, habits, bookmarks, timer, capture inbox — were missing. Built a full set of ten specialised widgets that all query `kernel.store.allObjects()` for records of a specific `type`, rendering the appropriate UX for each domain. Designed so Tier 2 (external provider streams) can drop in without touching widget code (see ADR-003).

### What landed

1. **New `record` category in `entities.ts`** — `{ category: "record", canParent: [], canBeRoot: true }`. Free-standing data records (tasks, reminders, contacts, events, notes, goals, habits, bookmarks, timer-sessions, captures) live at the workspace root alongside pages and folders.
2. **Ten record entity defs** — `taskDef`, `reminderDef`, `contactDef`, `eventDef`, `noteDef`, `goalDef`, `habitDef`, `bookmarkDef`, `timerSessionDef`, `captureDef`. Each declares only domain-specific fields; GraphObject top-level fields (name/description/status/date/tags/pinned/color) come from the inspector's Shell section automatically.
3. **Ten widget entity defs** (`component` category, `childOnly: true`) — `tasksWidgetDef`, `remindersWidgetDef`, `contactsWidgetDef`, `eventsWidgetDef`, `notesWidgetDef`, `goalsWidgetDef`, `habitTrackerWidgetDef`, `bookmarksWidgetDef`, `timerWidgetDef`, `captureInboxWidgetDef`. Each exposes widget-level config (filter, title, maxItems, etc.) but not per-row data — the row data is pulled live from the kernel by the renderer.
4. **`dynamic-widget-renderers.tsx`** — single file housing all ten React renderers and their pure helpers. Shared `DateChip`, `cardBase`, `headingRow` styles; per-widget filter logic that short-circuits on empty inputs. Tasks widget has inline done-toggle that writes `status` back via `onToggleDone`. Reminders widget uses circular round checkbox. Contacts widget supports cards-or-list display with avatar fallback via `contactInitials()` + mailto:/tel: action chips. Events widget buckets by range (today/week/month/all) and renders relative-date chip + hh:mm time. Notes widget reads `data.body` first, description second, collapses whitespace, truncates with ellipsis. Goals widget shows `currentValue / targetValue + unit` and a progress bar via `goalRatio()`. Habit tracker uses `habitWeeklyRatio()` against `targetPerWeek`, shows streak with fire emoji. Bookmarks widget supports grid-of-favicons or list display, uses `bookmarkHost()` to parse clean hostnames. Timer widget is a local `useState` focus-timer that logs a `timer-session` via `onCreateSession`. Capture inbox widget has an inline text input that creates a `capture` via `onCaptureSubmit`, and a "Done" button per row that marks `processedAt` via `onMarkProcessed`.
5. **Pure helpers (exported for vitest)** — `parseObjectDate`, `formatRelativeDate`, `filterTasks`, `orderTasks`, `priorityColor`, `filterReminders`, `filterContacts`, `contactInitials`, `filterEvents`, `formatEventTime`, `filterNotes`, `notePreview`, `filterGoals`, `goalRatio`, `formatDuration`, `filterBookmarks`, `bookmarkHost`, `habitWeeklyRatio`, `filterCaptures`. All deterministic — `nowMs()` is injected as a parameter with `Date.now()` as default so tests can freeze time.
6. **`layout-panel.tsx` wiring** — ten new `if (def.type === "…-widget")` branches right after the `map-widget` case. Each branch (a) declares typed fields via the same `as unknown as Fields[string]` cast pattern the other widgets use, (b) filters kernel objects by the right record type, (c) forwards `selectedId` + `onSelectObject` + (for interactive widgets) `onToggleDone` / `onCreateSession` / `onCaptureSubmit` / `onMarkProcessed` callbacks that call `kernel.updateObject` / `kernel.createObject`. Capture inbox's mark-processed handler reads the existing `data` object and spreads it before writing to preserve other fields.
7. **`dynamicDataInitializer`** (`builtin-initializers.ts`) — guarded on empty store for those record types, seeds ~25 sample records (5 tasks with mixed priorities + due dates, 3 reminders, 3 contacts (2 pinned), 4 events spread across the week, 3 notes (1 pinned, all tagged), 3 goals with progress, 3 habits with streaks, 4 bookmarks, 2 captures) so first-run users see the widgets populated. Added to the `createBuiltinInitializers()` list after `demoWorkspaceInitializer`.
8. **Tests** — `dynamic-widget-renderers.test.ts` covers 59 assertions across 19 describe blocks, all pure-helper coverage: date parsing (valid/invalid/empty), relative labelling (overdue/today/tomorrow/in-Nd/far-future), task filtering (all/open/done/today/overdue + project cross-filter), task ordering (date asc with nulls last, priority tie-break), reminder filtering, contact filtering + initials, event range windowing (today/week/month/all), note filtering + preview truncation/whitespace collapse, goal ratio clamping, duration formatting (seconds/minutes/h:m), bookmark folder filtering + hostname extraction, habit weekly ratio, capture processed/pending partitioning.
9. **ADR-003** — `docs/adr/003-dynamic-data-providers.md` scopes Tier 2 (external streams). Defines a `ProviderDefinition` contract in `@prism/core/providers`, provider records tagged with `data.source: { providerId, accountId, externalId, syncedAt }`, a daemon-owned sync worker materialising into the same Loro collection, credentials via existing VFS+vault+daemon stack, and `provider-binding` entity for wiring a collection to a provider. Widgets stay dumb — they query by type and never see provenance. Priority order: Google Calendar → Gmail → filesystem inbox → RSS → Weather → iCal → GitHub → Linear. Write-back, push notifications, and Apple Calendar/Reminders/Contacts are explicit non-goals for Phase 1.

### Status

- `pnpm typecheck` clean across the monorepo
- `pnpm test` — **198 files / 3869 tests passing** (59 new tests)
- `dynamicDataInitializer` guarded on empty store so re-running won't duplicate records

### Files touched

- `packages/prism-studio/src/kernel/entities.ts` — new `record` category rule, 20 new entity defs (10 records + 10 widgets), all wired into `createPageBuilderRegistry()`
- `packages/prism-studio/src/components/dynamic-widget-renderers.tsx` (new)
- `packages/prism-studio/src/components/dynamic-widget-renderers.test.ts` (new)
- `packages/prism-studio/src/panels/layout-panel.tsx` — 10 new widget branches + imports
- `packages/prism-studio/src/kernel/builtin-initializers.ts` — new `dynamicDataInitializer`
- `docs/adr/003-dynamic-data-providers.md` (new)
- `docs/adr/README.md` — ADR-003 index entry
- `packages/prism-studio/CLAUDE.md` — documents the new renderer file

---

## Real Button & Card renderers for Puck builder (Complete — 2026-04-13)

The Puck layout panel was rendering `button` and `card` blocks as generic dashed-border preview chips (the `entityToPuckComponent` fallback at `layout-panel.tsx:312`), so authors couldn't tell what a button would actually look like on their page. Promoted both to real interactive previews with an extended field vocabulary aimed at developers.

### What landed

1. **`button-renderer.tsx`** — real `<button>` / `<a>` preview. Variants: primary/secondary/outline/ghost/danger/success/gradient. Sizes: xs/sm/md/lg/xl. Leading or trailing icon (any glyph/emoji). Full-width, disabled, loading (with spinner via `prism-button-spin` keyframes). Rounded preset (none→full), shadow preset, hover effect (lift/glow/scale). Link target + auto-filled `noopener noreferrer` on `_blank`. Button `type` (button/submit/reset), aria label. Pure helpers `resolveVariant`/`resolveSize`/`resolveRadius`/`resolveShadow`/`resolveTransform`/`buildButtonStyles`/`resolveRel` exported for test.
2. **`card-renderer.tsx`** — real card preview. Variants: elevated/outlined/filled/ghost. Layouts: vertical, horizontal (40% media basis), overlay (gradient wash over full-bleed media). Eyebrow, title, body, optional CTA that reuses `ButtonRenderer`. Media fit (cover/contain), aspect ratio, clamped overlay opacity, hover effect (lift/glow). Pure helpers `resolveCardVariant`/`resolveCardLayout`/`clampOverlayOpacity`/`buildCardStyles` exported.
3. **`entities.ts`** — `buttonDef` extended with icon/iconPosition/fullWidth/disabled/loading/rounded/shadow/hoverEffect/target/rel/buttonType/ariaLabel plus three new variants and two new sizes. `cardDef` extended with eyebrow/mediaFit/mediaAspectRatio/variant/layout/hoverEffect/overlayOpacity/ctaLabel/ctaVariant. Fields are grouped via `ui.group` so the inspector stays organised.
4. **`layout-panel.tsx`** — new `def.type === "button"` and `def.type === "card"` branches right after the `image` case. Each reuses `entityToPuckComponent` for automatic field generation, then replaces the `render` with the real renderer. Card's `imageUrl` field is swapped for the VFS-aware `mediaUploadField`. The generic `mediaFieldOverrides` map loses its old `card` entry (dedicated branch owns it now).
5. **Tests** — `button-renderer.test.ts` (23 assertions across 7 helper groups — variant palettes, size scaling, radius map, shadow layering, hover transform, disabled override, rel defaulting) and `card-renderer.test.ts` (17 assertions — variant defaults, layout direction, overlay opacity clamping, hover transform, elevated base shadow).

### Status

- `pnpm typecheck` clean across the monorepo
- `pnpm exec vitest run --exclude 'e2e/**'` in prism-studio — **31 files / 405 tests passing** (40 new tests)
- Pre-existing playwright-vs-vitest config collision leaves e2e specs failing under vitest; unrelated to this change

### Files touched

- `packages/prism-studio/src/components/button-renderer.tsx` (new)
- `packages/prism-studio/src/components/button-renderer.test.ts` (new)
- `packages/prism-studio/src/components/card-renderer.tsx` (new)
- `packages/prism-studio/src/components/card-renderer.test.ts` (new)
- `packages/prism-studio/src/kernel/entities.ts`
- `packages/prism-studio/src/panels/layout-panel.tsx`

---

## Resizable 4-bar PageShell (Complete — 2026-04-13)

Layout panel's page shell was rigid: one of three hard-coded grids (`sidebar-left`/`sidebar-right`/`stacked`), single `sidebarWidth` int field, no drag handles. Replaced with a 3×3 grid of four independently-resizable bars wrapping a central main canvas.

### What landed

1. **`PageShellRenderer` rewrite** (`packages/prism-studio/src/components/layout-shell-renderers.tsx`) — 3×3 CSS Grid driven by `topBarHeight` / `leftBarWidth` / `rightBarWidth` / `bottomBarHeight`. Each populated bar renders a `<ResizeHandle>` on its inner edge (`col-resize` / `row-resize` cursor, `pointerdown`→`pointermove`→`pointerup` with `pointercancel` fallback). Drag updates local state live for smooth feedback; `onCommit(key, value)` fires once on pointerup for kernel persistence. Empty bars collapse to 0 via `hasContent(node)`. Pure `computeShellGrid()` helper exported for test.
2. **`SideBarRenderer` resizable** — standalone `SideBar` widget reuses the same `useResizeHandle` hook and now supports four positions (`left`/`right`/`top`/`bottom`) with per-position handle placement and cursor.
3. **Slot rename** (`packages/prism-studio/src/panels/layout-panel-data.ts`) — `PAGE_SLOTS = ["topBar", "leftBar", "rightBar", "bottomBar"]`; `SHELL_SLOTS["page-shell"] = ["topBar", "leftBar", "main", "rightBar", "bottomBar"]`. Per CLAUDE.md "rename, move, break, fix" — no back-compat for `header`/`sidebar`/`footer` slot names.
4. **Entity defs** (`packages/prism-studio/src/kernel/entities.ts`) — `pageDef` and `pageShellDef` replaced old `layout`/`sidebarWidth`/`stickyHeader` fields with `topBarHeight`/`leftBarWidth`/`rightBarWidth`/`bottomBarHeight`/`stickyTopBar`. Page `layout` enum simplified to `flow` | `shell`.
5. **`rootRender` wiring** (`layout-panel.tsx`) — in `shell` mode, passes the four slot functions + dimension props into `PageShellRenderer` and supplies an `onCommit` closure that calls `kernel.updateObject(pageId, { data: { ...currentData, [key]: value } })` on pointerup. `flow` mode unchanged (free-scrolling single column).
6. **Seeds** — `playground-seed.ts` shell page now seeds `topBar`/`leftBar`/`rightBar`/`bottomBar` content with non-zero dimensions so every bar renders on first load. `builtin-initializers.ts` About page switched from `layout: "sidebar"` to `layout: "shell"`.
7. **Test updates**
   - `layout-panel.test.ts` — `seedShellPage` + `kernelToPuckData` projection test + `walkTree` round-trip + `splitRootProps` + new `computeShellGrid` suite (4 cases: full grid, absent bars collapsing, clamp negative/huge, fractional rounding).
   - `loro-puck-bridge.test.ts` — slot-shaped round-trip data updated to the new PageShell shape.

### Status

- `pnpm typecheck` clean across `@prism/core`, `@prism/studio`, `@prism/puck-playground`
- `pnpm lint` clean
- `pnpm test` — **196 files / 3793 tests passing**

### Files touched

- `packages/prism-studio/src/components/layout-shell-renderers.tsx`
- `packages/prism-studio/src/panels/layout-panel-data.ts`
- `packages/prism-studio/src/panels/layout-panel.tsx`
- `packages/prism-studio/src/panels/layout-panel.test.ts`
- `packages/prism-studio/src/kernel/entities.ts`
- `packages/prism-studio/src/kernel/builtin-initializers.ts`
- `packages/prism-puck-playground/src/playground-seed.ts`
- `packages/prism-core/src/bindings/puck/loro-puck-bridge.test.ts`

---

## Universal Admin Kit — Puck-native admin panels for every runtime (Complete — 2026-04-13)

Daemon, Relay, and Studio each had their own ad-hoc "admin" views. Unified them behind a single Puck-native package (`@prism/admin-kit`) so one editable dashboard can reflect any Prism runtime — in-process kernel, or remote HTTP relay — through a normalised `AdminSnapshot`.

### What landed

1. **New package `@prism/admin-kit`** — Puck widget library + data source abstraction.
   - `types.ts`: `AdminDataSource` interface, `AdminSnapshot`/`HealthStatus`/`Metric`/`Service`/`ActivityItem`/`HealthLevel`, `emptySnapshot()` initial value.
   - `admin-helpers.ts`: pure `formatUptime`/`formatBytes`/`formatMetricValue`/`formatRelativeTime`/`rollupHealth` + `HEALTH_COLORS` palette.
   - `admin-context.tsx`: `AdminProvider` — wires a data source to a reactive snapshot via subscribe-path (if supported) or `setInterval` fallback. Exposes `useAdminContext()` + `useAdminSnapshot()`.
   - `widgets/`: `SourceHeader`, `HealthBadge`, `MetricCard`, `MetricChart` (Recharts line/bar), `ServiceList`, `ActivityTail`, `UptimeCard` — each consumes `useAdminSnapshot()` and renders framework-free dark-theme CSS.
   - `puck-config.tsx`: `createAdminPuckConfig()` registers every widget with `fields`/`defaultProps`/`render`, categorised into `summary`/`metrics`/`lists`. Mirrors layout-panel's `as unknown as Fields[string]` casting pattern.
   - `default-layout.ts`: `createDefaultAdminLayout()` seeds a drag-ready dashboard (header + health + uptime + 5 metric cards + trend chart + service list + activity tail).
   - Package exports: `.`, `./widgets`, `./data-sources`, `./puck`.

2. **Two canonical data sources.**
   - **`createKernelDataSource(kernel, opts)`** — projects a StudioKernel-shaped target (`store`/`notifications`/`relay`/`presence`/`bus`/optional `listFiles`) into an `AdminSnapshot`. Uses a structural `KernelAdminTarget` interface to avoid a studio→admin-kit cycle. `subscribe()` lazily attaches `PrismBus` listeners (`ObjectCreated`/`Updated`/`Deleted`/`EdgeCreated`/`EdgeDeleted`) + `NotificationStore.subscribe` + `RelayManager.subscribe`, pushing fresh snapshots; detaches on last listener leave. Ring-buffer activity feed (default 50), seeded from `NotificationStore.getAll()`.
   - **`createRelayDataSource({url, fetch?, now?})`** — HTTP client for a running `@prism/relay` server. Parallel fetches `/api/health` + `/api/modules` + `/metrics`. Unreachable → `error` health + diagnostic detail; reachable → module list mapped to services, numeric metrics rolled up. Prometheus samples back-fill uptime/peers/connections when the JSON health endpoint omits them. Injectable `fetch` / `now` for tests.

3. **Prometheus parser on top of `@prism/core/syntax`'s Scanner.**
   - Per user directive: "any parsing should be done via Prism Syntax / Codegen". Rewrote `data-sources/prometheus-parse.ts` to use `Scanner`/`ScanError`/`isDigit`/`isIdentChar`/`isIdentStart`.
   - Custom `isPromNameStart`/`isPromNameChar` predicates allow `:` in metric names. `scanSample` handles unlabelled + labelled samples, `scanLabels` state-machine handles `{k="v",...}` with quoted values, `scanNumberLiteral` special-cases Prometheus `+Inf`/`-Inf`/`NaN` tokens then delegates to `Scanner.scanNumber`. Line-level error recovery: a malformed line swallows its `ScanError` and continues at the next newline.
   - `findSample(samples, name, filter?)` — helper for name + optional label filtering.

4. **Studio integration — `adminLensBundle`.**
   - New `panels/admin-panel.tsx`: `AdminPanel` component with a source picker dropdown (Kernel + each configured relay), `<AdminProvider>` wrapping `<Puck config={createAdminPuckConfig()} data={…} onChange={setData} />`. Data source is memoized per source id; cleanup via `dispose?.()` in effect cleanup; auto-reverts to Kernel when the selected relay disappears.
   - Lens registered as `ADMIN_LENS_ID` with `Shift+A` keybinding, wired into `createBuiltinLensBundles()`.
   - `@prism/admin-kit` added to `prism-studio/package.json` dependencies; tsconfig path alias + root `tsconfig.base.json` path alias + Vite regex aliases added for both `@prism/admin-kit` and `@prism/admin-kit/*`.

### Test coverage

- `packages/prism-admin-kit/src/admin-helpers.test.ts` (21 tests) — every formatter + `rollupHealth` + `HEALTH_COLORS`.
- `packages/prism-admin-kit/src/data-sources/prometheus-parse.test.ts` (12 tests) — empty input, comments, unlabelled + labelled samples, colon metric names, signed/fractional values, `Inf`/`NaN`, malformed-line recovery, `findSample` name + label filtering.
- `packages/prism-admin-kit/src/data-sources/relay-data-source.test.ts` (6 tests) — default + custom id/label, full snapshot composition, unreachable fallback, Prometheus back-fill, thrown-fetch graceful handling.
- `packages/prism-admin-kit/src/data-sources/kernel-data-source.test.ts` (8 tests) — projection, service health inference, unread notification warn, bus-event activity capture, subscription cleanup, injected clock.
- `packages/prism-admin-kit/src/puck-config.test.ts` (6 tests) — component registration, category grouping, defaultProps/render presence, default-layout shape + unique ids, only references registered components.
- `packages/prism-studio/src/panels/admin-panel.test.ts` (5 tests) — lens manifest shape, bundle install into LensRegistry, real StudioKernel projection, streaming snapshots on mutation.

### Status

- `pnpm typecheck` — clean across all 7 packages (added admin-kit alongside existing 6)
- `pnpm test` — **195 files / 3766 tests passing** (was 3708; +58 new)
- `pnpm build` — Studio production bundle succeeds (includes admin-kit via Vite aliases)

### Files touched

- **New:**
  - `packages/prism-admin-kit/package.json`, `tsconfig.json`
  - `packages/prism-admin-kit/src/index.ts`, `types.ts`, `admin-helpers.ts`, `admin-helpers.test.ts`, `admin-context.tsx`, `puck-config.tsx`, `puck-config.test.ts`, `default-layout.ts`
  - `packages/prism-admin-kit/src/widgets/{index.ts,styles.ts,source-header.tsx,health-badge.tsx,uptime-card.tsx,metric-card.tsx,metric-chart.tsx,service-list.tsx,activity-tail.tsx}`
  - `packages/prism-admin-kit/src/data-sources/{index.ts,kernel-data-source.ts,kernel-data-source.test.ts,relay-data-source.ts,relay-data-source.test.ts,prometheus-parse.ts,prometheus-parse.test.ts}`
  - `packages/prism-studio/src/panels/admin-panel.tsx`, `admin-panel.test.ts`
- **Modified:**
  - `packages/prism-studio/package.json` — add `@prism/admin-kit: workspace:*`
  - `packages/prism-studio/tsconfig.json`, `vite.config.ts` — path + Vite aliases
  - `packages/prism-studio/src/lenses/index.tsx` — register `adminLensBundle`
  - `tsconfig.base.json` — workspace-wide `@prism/admin-kit` path alias

## Puck builder — fonts, universal styling, page-native slots (Complete — 2026-04-13)

Three gaps closed on the Layout Panel: font family was a free-form string, most widgets couldn't access the style-field spread (align/color/size/font), and sidebar/header/footer layout required dropping a `PageShell` block explicitly. Pages are now layout-aware as a first-class concept — no implicit wrapper.

### What landed

1. **Font picker with Google Fonts preview** — `@prism/core/page-builder` now re-exports a new `fonts.ts` module (`FONT_OPTIONS`, `findFontOption`, `isGoogleFontValue`, `googleFontsHref`, `collectFontFamilies`) covering 3 system stacks + 15 Google families with declared weights.
   - `components/puck-custom-fields.tsx` adds `fontPickerField({ label? })`: a native grouped `<select>` where each `<option>` renders in its own font face, plus a dashed preview swatch below. At construction time it injects `<link rel="stylesheet">` tags into `document.head` for every Google family (idempotent via a module-level `loadedFontHrefs` set, guarded by `typeof document`).
   - `layout-panel.tsx` `entityToPuckComponent()` routes any field with `id === "fontFamily"` to `fontPickerField`, so every text-bearing entity inherits the picker automatically.
   - HTML export (`@prism/core/page-builder → page-export.ts`) walks the exported node tree with `collectFontFamilies()` and emits `<link rel="preconnect">` + `<link rel="stylesheet">` for every Google family used, wired in the `<head>` next to `<title>`.

2. **Universal style fields on every Puck widget** — `layout-panel.tsx` adds `attachStyleFieldsInPlace(components)`, a single post-processing pass that merges `STYLE_FIELD_DEFS` into every component config that doesn't already expose `fontFamily` (i.e. every hand-rolled widget — kanban/list/table/chart/map/stat/badge/alert/progress/markdown/iframe/code/video/audio/tabs/popover/slide/form inputs/layout primitives).
   - Each wrapped render funnel the original output through a `<div style={computeBlockStyle(extractBlockStyle(props))} className={props.className}>`. No-op when no style props are set (falls back to a fragment).
   - Idempotent because entity-def-flow components (heading, text-block, shells, card, etc.) already include `fontFamily` via their `...STYLE_FIELD_DEFS` spread and are skipped.
   - Cached style-field schema in `STYLE_PUCK_FIELDS_CACHE` so the Puck field construction doesn't rebuild per-component.

3. **Page-native slots — no implicit PageShell** — the `page` entity now owns `layout`/`sidebarWidth`/`stickyHeader` directly and projects its `__slot`-tagged children into Puck's `root.props` via `PAGE_SLOTS = ["header", "sidebar", "footer"]`.
   - `panels/layout-panel-data.ts`: `SHELL_SLOTS["page"] = PAGE_SLOTS`; `kernelToPuckData()` now copies non-`__slot` keys from `page.data` into `root.props` and fills each `PAGE_SLOTS` key with `buildPuckContent(pageId, slot, ...)`. `splitRootProps()` is the reverse partition used by `syncPuckToKernel`.
   - `panels/layout-panel.tsx`: the Puck `root` config is built from `entityToPuckComponent(pageDef)` fields + `{type: "slot"}` fields for each `PAGE_SLOTS` entry. The root render reads `layout` off `root.props` and wraps the main flow in `PageShellRenderer` for `sidebar-left`/`sidebar-right`/`stacked`, falls through to the original flow container for `flow` (default).
   - `kernel/entities.ts`: `pageDef.layout` enum is now `flow|stacked|sidebar-left|sidebar-right` (was `single|sidebar|full`), default `"flow"`; added `sidebarWidth` (int, default 240) and `stickyHeader` (bool, default true).
   - `syncPuckToKernel` rewrites root handling: splits `rootProps` via `splitRootProps`, updates the page entity's `data` with the scalar keys (preserving any existing `__slot` tag), then diffs each slot's content array against existing `__slot`-tagged children. Added `kernel.store.getObject` to the `KernelSync` type.
   - `@prism/puck-playground/playground-seed.ts`: `buildShellPage()` drops header/sidebar/footer children directly on the page with `__slot` tags (no `PageShell` wrapper); `pageRoot()` accepts an `extraData` bag for `{layout, sidebarWidth, stickyHeader}`.

### Test coverage

- `packages/prism-core/src/interaction/page-builder/fonts.test.ts` (new, 15 tests) — `FONT_OPTIONS` structure, `findFontOption` (exact stack / leading family / unknown / empty), `isGoogleFontValue`, `googleFontsHref` (empty / single / multi-dedup / `+` encoding for "Playfair Display"), `collectFontFamilies` (nested walk + dedup, ignores empty).
- `packages/prism-studio/src/panels/layout-panel.test.ts` — added a `kernelToPuckData — page-level slot projection` describe block and a `splitRootProps` describe block covering scalar/slot partition, default empty arrays for missing slots, non-array coercion, and undefined input handling.

### Status

- `pnpm typecheck` — clean across all 6 packages
- `pnpm test` — **189 files / 3708 tests passing** (was 3688)
- No explicit `PageShell` block is needed to get a sidebar; the palette still offers `PageShell` for nested shells (site-header, hero, etc.)

### Files touched
- **New:** `packages/prism-core/src/interaction/page-builder/fonts.ts`, `fonts.test.ts`
- **Modified:** `packages/prism-core/src/interaction/page-builder/index.ts`, `page-export.ts`; `packages/prism-studio/src/components/puck-custom-fields.tsx`; `packages/prism-studio/src/kernel/entities.ts`; `packages/prism-studio/src/panels/layout-panel.tsx`, `layout-panel-data.ts`, `layout-panel.test.ts`; `packages/prism-puck-playground/src/playground-seed.ts`

## Puck builder — functional design pass (Complete — 2026-04-13)

Functional design pass on the Layout Panel (`@prism/studio` → `panels/layout-panel.tsx`) to make the Puck visual builder productive end-to-end instead of just "technically wired up". Four concrete gaps closed: facets couldn't be created from inside the builder, media had no upload path, component palette was one flat list, and there was no at-a-glance context about which page you were editing.

### What landed

1. **VFS media URL scheme + resolver** — new `components/vfs-media-url.ts` bridges the pre-existing `vfs://<hash>` convention (written by `assets-panel.tsx` but never read) to DOM-consumable blob URLs.
   - Exports `VFS_SCHEME`, `isVfsMediaUrl`, `parseVfsHash`, `buildVfsMediaUrl`, `isBrowserMediaUrl`, `resolveVfsMediaUrl`, `useResolvedMediaUrl`.
   - Module-scoped `blobUrlCache: Map<hash, blobUrl>` — content-addressed hashes are immutable so the cache entry is safe forever. `__resetVfsMediaCache` exported for tests.
   - `useResolvedMediaUrl` hook initializes synchronously for http/https/data/blob (no flash), only awaits for `vfs://`.
   - Test file `vfs-media-url.test.ts` exercises the pure helpers + the async resolver against a real `createVfsManager` (stat cache hit on 2nd call verified).

2. **`mediaUploadField` — VFS-integrated Puck custom field** — new `components/vfs-media-field.tsx`.
   - Factory `mediaUploadField(kernel, { label?, accept? })` returns a Puck `Field<string>` that closes over the kernel. Render delegates to `<MediaUploadFieldInner>` where hooks are legal (Puck invokes render as a React component).
   - Three input modes: **upload** (drag-drop or file picker → `kernel.importFile` → `vfs://<hash>`), **pick from vault** (filtered by MIME prefix via `kernel.listFiles` — same BinaryRef index the Assets panel uses), **paste URL** (http/https fallback for external assets).
   - Success/failure feedback via `kernel.notifications.add()` — same toast queue every Studio action uses.
   - `accept: "image" | "video" | "audio" | ""` drives both the filter and the native `<input accept="image/*">` attribute.
   - Preview box resolves `vfs://` hashes via `useResolvedMediaUrl`, shows filename/MIME/size from `BinaryRef` for vault files.

3. **`facetPickerField` — inline facet creator Puck custom field** — new `components/facet-picker-field.tsx`.
   - Dropdown of `kernel.listFacetDefinitions()` + **New…** button that reveals an inline form capturing name / objectType / layout.
   - Object-type suggestions come from `kernel.registry.allDefs()` filtered to non-workspace/non-section — authors can't target types that don't exist.
   - Creates via `createFacetDefinition(id, objectType, layout)` from `@prism/core/facet`; collisions resolved by `uniqueFacetId`; success emits a notification.
   - Pure helpers `facetIdFromName(name, objectType)` and `uniqueFacetId(base, existing)` split into `components/facet-picker-helpers.ts` so vitest can exercise them in the node env without loading Puck.

4. **Media renderers accept `vfs://` URLs** — `components/media-renderers.tsx` (`VideoWidgetRenderer`, `AudioWidgetRenderer`).
   - `isSafeMediaUrl` allow-list extended: accepts `http://`, `https://`, and `vfs://<hash>`.
   - Both renderers call `useResolvedMediaUrl(src, kernel.vfs)` and emit loading / missing placeholder states.
   - Poster images on the video widget also resolve through the same pipeline.
   - `media-renderers.test.ts` now covers the vfs case.

5. **Image block** — `PuckImageBlockRender` React component added to `layout-panel.tsx`. Uses `useResolvedMediaUrl` so `vfs://` hashes produce real `<img>` tags with width/height/alt/caption. The image entity bypasses the generic Puck component factory (`continue` after building its ComponentConfig) so it can opt into the dedicated renderer + `mediaUploadField` for `src`.

6. **Component categories** — layout panel now passes `config.categories` to Puck instead of dumping every entity into one sidebar list.
   - `COMPONENT_CATEGORY_MAP`, `CATEGORY_TITLES`, `buildPuckCategories()`, and `PuckCategoryBucket` moved into `panels/layout-panel-data.ts` so they're importable from vitest without loading the full UI graph (leaflet, recharts, Puck runtime).
   - Eight buckets: Layout, Content, Media, Data Views, Forms, Navigation, Display, Dynamic; plus Puck's built-in `other` catch-all for unmapped types. Layout + Content are `defaultExpanded`.
   - Insertion order is driven by `CATEGORY_TITLES`, so sidebar ordering is stable regardless of `Object.keys(components)` order upstream. Empty non-`other` buckets are pruned via a rebuild (not `delete`, to satisfy `no-dynamic-delete` lint).
   - New video/audio/image/card/hero blocks wire in `mediaUploadField` via a `mediaFieldOverrides` map (card.imageUrl, hero.backgroundImage) on the generic path.

7. **Facet picker wired into facet-view + spatial-canvas blocks** — `facetId` fields on those blocks use `facetPickerField(kernel, { label: "Facet" })` so an author can create the bound FacetDefinition without leaving the layout builder.

8. **Header strip above the Puck canvas** — wraps `<Puck>` in a flex column with a top row that shows the current page title (falls back to `page.name`), slug, and a published/draft badge. Test IDs: `layout-panel`, `layout-panel-header`, `layout-panel-page-title`, `layout-panel-status-badge`.

### Test coverage

`packages/prism-studio/src/panels/layout-panel.test.ts` (14 tests total):
- **Existing shell projection tests** — unchanged (2 tests covering `kernelToPuckData` + Puck `walkTree` round-trip).
- **`buildPuckCategories` (5 tests)** — drops empty non-other buckets, sends unknown types to `other`, marks layout+content as `defaultExpanded`, preserves canonical insertion order, and verifies **every** `component`/`section` entity registered by `createStudioKernel()` lands in an expected bucket (nothing silently goes missing).
- **`pascalToKebab` / `kebabToPascal`** — round-trip for every key in `COMPONENT_CATEGORY_MAP`.
- **`facetIdFromName` (3 tests)** — slugifies human names, collapses runs of non-alphanumerics, falls back to `<objectType>-facet` on empty input.
- **`uniqueFacetId` (3 tests)** — returns base id when no collision, suffixes `-2` on first collision, walks past contiguous collisions.

`packages/prism-studio/src/components/vfs-media-url.test.ts` — new file, covers the URL-scheme helpers and the async resolver (including the content-addressed cache hit).

`packages/prism-studio/src/components/media-renderers.test.ts` — extended with a `vfs://` case on `isSafeMediaUrl`.

### Status

- `pnpm typecheck` — clean across all 6 packages
- `pnpm test` — 188 files / 3688 tests passing (was 3006 tests at the start of the pass)
- `pnpm lint` — clean

### Files touched
- **New:** `packages/prism-studio/src/components/vfs-media-url.ts`, `vfs-media-url.test.ts`, `vfs-media-field.tsx`, `facet-picker-field.tsx`, `facet-picker-helpers.ts`
- **Modified:** `packages/prism-studio/src/panels/layout-panel.tsx`, `layout-panel-data.ts`, `layout-panel.test.ts`; `components/media-renderers.tsx`, `media-renderers.test.ts`

## Puck builders — real chart/map renderers + standalone playground harness (Complete — 2026-04-12)

Replaced the placeholder chart and map widget renderers in `@prism/studio` with real `recharts` + `react-leaflet` implementations and stood up a new `@prism/puck-playground` package — a single-file Vite SPA that boots a real `StudioKernel` with seeded demo data so the layout panel and every data-aware Puck widget can be exercised in isolation, away from the full Studio shell.

### What landed

1. **`@prism/studio` widget renderers — real implementations**
   - **`components/chart-widget-renderer.tsx`** rewritten to use **recharts ^2.15.0** (`<ResponsiveContainer>` wrapping `BarChart`/`LineChart`/`PieChart`/`AreaChart`). Dark theme: `#0f172a` background, `#a855f7` accent, `#94a3b8` axes. Pie uses `<Cell>` per slice with `CHART_PALETTE`; bar/line/area share `CartesianGrid`/`XAxis`/`YAxis`/`Tooltip`.
   - **`components/map-widget-renderer.tsx`** rewritten to use **react-leaflet ^5.0.0 + leaflet ^1.9.4** with `MapContainer`/`TileLayer` (OSM)/`Marker`/`Popup`. Patches Leaflet's default icon URLs by importing `leaflet/dist/images/marker-icon{,-2x,-shadow}.png`. A custom inline `FitBounds` component uses `useMap()` to refit on bounds change. Click handlers use a conditional spread (`{...(onSelectObject ? { eventHandlers: ... } : {})}`) so `exactOptionalPropertyTypes` doesn't reject `undefined` for `LeafletEventHandlerFnMap`.
2. **Pure-logic split for vitest compatibility** — both renderers had to keep working under vitest's node env where recharts/leaflet can't evaluate. New sibling files isolate the pure helpers:
   - **`components/chart-data.ts`** — `aggregateObjects`, `CHART_PALETTE`, types `ChartType`/`ChartAggregation`/`ChartDataPoint`. The renderer re-exports it for back-compat.
   - **`components/map-data.ts`** — `extractMarkers`, `computeBounds`, types `MapMarker`/`MapBounds`.
   - Test files updated to import from `./chart-data.js` and `./map-data.js` instead of the renderers. Neither renderer imports `leaflet/dist/leaflet.css` — host apps do that from their own `main.tsx` so vitest stays DOM-free.
3. **`src/vite-env.d.ts` (new)** — `*.png`/`*.svg`/`*.css` ambient module declarations so the leaflet PNG imports type-check under TS strict.
4. **`@prism/studio` deps** — added `leaflet ^1.9.4`, `react-leaflet ^5.0.0`, `recharts ^2.15.0`, devDep `@types/leaflet ^1.9.12`. `main.tsx` gains `import "leaflet/dist/leaflet.css"`.
5. **New package: `@prism/puck-playground`** — standalone Vite SPA that bundles to a single self-contained `dist/index.html` (~10 MB, ~3 MB gzipped) via [`vite-plugin-singlefile`](https://github.com/richardtallent/vite-plugin-singlefile). Contents:
   - **`vite.config.ts`** — reuses `buildCoreAliases()` (one alias per `@prism/core/*` subpath, sorted longest-first) plus a regex alias `@prism/studio/*` → `../prism-studio/src/$1`. Mirrored in `tsconfig.json` `paths`. The playground always sees latest studio source — no build step, no `exports` field, no publish.
   - **`src/playground-seed.ts`** (562 lines) — a custom `StudioInitializer` that seeds five sample collections (15 `demo-task`, 8 `demo-contact`, 16 `demo-sale`, 7 `demo-place` with real lat/lng for Anthropic HQ / Berlin / Tokyo / etc., 8 `demo-event` with dates relative to today) and seven demo pages, each calling a `build*Page()` function that pre-populates the page with curated Puck blocks: Welcome (hero + stats), Data Widgets (kanban/list/table/card-grid), Charts & Reports (bar/line/pie/area + report widget), Map & Calendar, Forms (text/email/textarea/select/number/date/checkbox), Display & Content (alert/badge/progress-bar/markdown/code-block), Layout Primitives (columns/divider/spacer/tab-container). Guarded by `kernel.store.objectCount() > 0` so it's idempotent. Ends with `kernel.undo.clear()` and `kernel.select(welcome.id)`.
   - **`src/playground-app.tsx`** — `createStudioKernel({ lensBundles: [layoutLensBundle], initializers: [playgroundSeedInitializer] })`, `KernelProvider` wrapping `LayoutPanel`, left sidebar listing every `page` object with active-page tracking via `parentId` walk, header with **Reset workspace** button that disposes the kernel and re-mounts via `key` swap.
   - **`src/main.tsx`** — React root mount, imports `leaflet/dist/leaflet.css` for the host app.
   - Plugins: `react()`, `wasm()`, `topLevelAwait()`, `viteSingleFile()` (the last two are mandatory because the kernel pulls in `loro-crdt`).
   - `cssCodeSplit: false`, `assetsInlineLimit: 100_000_000` so the build is truly single-file.
   - Dev server on `:4179` (strictPort).
6. **Docs** — `packages/prism-puck-playground/README.md` and `packages/prism-puck-playground/CLAUDE.md` cover the harness's purpose, build commands, source-aliasing trick, demo workspace contents, and reset behaviour. `packages/prism-studio/CLAUDE.md` updated to reflect the recharts/leaflet swap and the chart-data/map-data pure-logic split.

### Status

- `pnpm --filter @prism/puck-playground typecheck` — clean
- `pnpm --filter @prism/puck-playground build` — `dist/index.html` 10,349,992 bytes (3.0 MB gzipped), built in ~7.5 s
- `pnpm --filter @prism/puck-playground dev` — boots on `http://localhost:4179` in ~150 ms, serves transformed modules cleanly
- `pnpm --filter @prism/studio vitest run --exclude 'e2e/**'` — **327 / 327 unit tests passing** (the 31 collection failures in the unfiltered run are pre-existing Playwright-file collisions unrelated to this work)

### Notes

- The playground deliberately installs only `layoutLensBundle`, not `createBuiltinLensBundles()`. Adding another lens to repro something is a one-line import (`import { canvasLensBundle } from "@prism/studio/panels/canvas-panel.js"`).
- The demo pages can be edited inside the playground freely — those edits live in the kernel's in-memory `CollectionStore` only and disappear on **Reset workspace**, which is the entire point of the harness.
- Future work: a `pnpm --filter @prism/puck-playground preview-static` script that opens `dist/index.html` directly via `file://` to verify the single-file output works without a server. Right now this is a manual step.

## ADR 002 Phases 2–4 — Luau fold, Studio→Core extraction, Registry collapse (Complete — 2026-04-12)

Completed the remaining three phases of ADR-002, bringing the unified language/document model to its final shape and extracting shared kernel infrastructure from Studio into Core so future apps (Flux, Lattice, Musica) can reuse it.

### Phase 2 — Fold Luau into `language/luau/`

Merged the two disjoint Luau directories (`syntax/luau/` for parsing and standalone `luau/` for runtime+debugger) into a single `language/luau/` home. `createLuauContribution()` is now the canonical entry point, replacing the old `createLuauLanguageDefinition()` + ad-hoc `DocumentSurfaceRegistry` pair.

### Phase 3 — Extract Studio kernel modules to Core

Moved 5 pure support modules from `packages/prism-studio/src/kernel/` to their final `@prism/core/*` homes via `git mv`:

- `design-tokens.ts` → `@prism/core/design-tokens` (DesignTokenRegistry, tokensToCss, DEFAULT_TOKENS)
- `block-style.ts` → `@prism/core/page-builder` (BlockStyleData, STYLE_FIELD_DEFS, computeBlockStyle — de-React-ified: `CssStyle = Record<string, string | number>` replaces React `CSSProperties`)
- `page-export.ts` → `@prism/core/page-builder` (exportPageToJson, exportPageToHtml, renderNodeHtml)
- `relay-manager.ts` → `@prism/core/relay-manager` (createRelayManager, RelayEntry)
- `builder-manager.ts` → `@prism/core/builder` (merged into existing builder/, circular `@prism/core/builder` self-import fixed with relative imports)

Created `@prism/core/initializer` — generic `KernelInitializer<TKernel>` post-boot hook pattern. Studio's `StudioInitializer` is now a thin alias (`KernelInitializer<StudioKernel>`).

All Studio imports updated to consume from `@prism/core/*`. Studio kernel directory now contains only wiring + entity definitions.

### Phase 4 — Unify codegen + collapse registries

Unified `LanguageRegistry` replaces the old split between `LanguageDefinition` + `DocumentSurfaceRegistry`. `LanguageContribution` is the single registration type. Compat bridge (`compat.ts`) retired. Markdown contribution (`createMarkdownContribution()`) created so there is exactly one markdown tokenizer.

### Status

- `pnpm -r typecheck` — all 5 workspace packages clean
- `pnpm test` — **186 files, 3655 tests passing**
- ADR-002 status updated to **Accepted**
- `packages/prism-core/CLAUDE.md` and `packages/prism-studio/CLAUDE.md` updated to reflect final locations
- No `layer1`/`layer2` references remain in source code (only in historical ADR/plan docs)

## prism-daemon — Whisper + Conferencing (Tracks, Rooms, Streaming STT) (Complete — 2026-04-12)

Closed every gap between the existing data-channel/batch-STT primitives and real chatroom functionality: audio/video tracks, room management (P2P mesh + Relay SFU), and streaming Whisper sessions. The daemon can now transport pre-encoded media frames, group peers into multi-party rooms, and incrementally transcribe audio — the three building blocks the SPEC's "Self-Dictation" pattern needs.

### What landed

1. **Audio/video track support** (conferencing module, 5 new commands) — `conferencing.{add_track,write_sample,recv_track_data,list_tracks,remove_track}`. Local tracks are `TrackLocalStaticSample` backed by `RTCRtpCodecCapability` (Opus @ 48 kHz stereo for audio, VP8 @ 90 kHz for video). `write_sample` accepts hex-encoded pre-encoded media frames — the host encodes (Opus/VP8), the daemon transports. Remote tracks are received via `on_track` callback which spawns a tokio task that continuously reads RTP payloads into a `TrackInbox`. `recv_track_data` drains buffered samples as hex.
2. **Room management** (conferencing module, 6 new commands) — `conferencing.{create_room,join_room,leave_room,room_info,list_rooms,broadcast_data}`. Rooms group peer connections for multi-party calls. Full-mesh P2P for small groups (2–4), Relay SFU for larger ones. `broadcast_data` fans a data-channel message to every room member. `join_room` validates peer exists; `leave_room` is idempotent.
3. **Streaming Whisper sessions** (whisper module, 4 new commands) — `whisper.{create_session,push_audio,poll_segments,close_session}`. Sessions accumulate PCM via `push_audio`. `poll_segments` transcribes the entire buffer and returns ALL segments plus `audio_duration_ms` (whisper.cpp `full()` resets state per call — host diffs results). For the SPEC's Self-Dictation pattern the host forks raw PCM to both whisper and Opus encode → track write.
4. **Conferencing module total: 24 commands** (13 original + 5 track + 6 room). **Whisper module total: 9 commands** (5 batch + 4 streaming). Added `webrtc-media = "0.9"` dep for the `Sample` type.
5. **Tests** — `cargo test --features conferencing` is **87 lib + 9 integration + 2 stdio = 98 passing**. New: track add/list/remove, room create/join/leave/info/list, unknown peer/room/kind rejection. Whisper gains 4 streaming tests. Clippy clean, formatted.

### Status

- `cargo test --lib` (default) — **72 lib tests passing** (unchanged)
- `cargo test --features conferencing` — **87 lib + 9 integration + 2 stdio = 98 tests passing**
- `cargo clippy --features conferencing --all-targets -- -D warnings` — clean
- `cargo check --features whisper` — still requires `cmake` on PATH

### Notes / deferred

- **No Opus/VP8 encode/decode in the daemon** — tracks transport pre-encoded frames; host handles codecs.
- **Whisper ↔ conferencing bridge is host-orchestrated** — the host forks mic PCM to whisper and Opus encode pipelines. Keeps modules independent and composable.
- **Room signaling is local** — cross-daemon peer discovery goes through the Relay's signaling channel.
- Future: video-codec feature (libvpx for daemon-side VP8), Opus codec feature (for daemon-side encode/decode), SFU relay integration.

## prism-daemon — VFS + Crypto + Cross-Surface E2E (Complete — 2026-04-11)

Made the Rust `prism-daemon` crate implement every SPEC-critical primitive that can run on every target we ship (desktop, mobile, browser, embedded), and wired up end-to-end tests that exercise each surface through its real transport rather than a host simulator.

### What landed

1. **VFS module** (`src/modules/vfs_module.rs`, 13 unit tests) — content-addressed blob store behind `vfs.{put,get,has,delete,list,stats}`. SHA-256 hex keys, atomic `write-temp + rename` writes, lock-free reads. Hosts inject a `VfsManager` rooted at the app data directory via `builder.set_vfs_manager(...)`; the module lazily creates one under the OS temp dir if nothing is plugged in. This is the local implementation of the SPEC's `object_store` adapter.
2. **Crypto module** (`src/modules/crypto_module.rs`, 14 unit tests) — libsodium-equivalent primitives (X25519 ECDH + XChaCha20-Poly1305 AEAD + CSPRNG) behind `crypto.{keypair,derive_public,shared_secret,encrypt,decrypt,random_bytes}`. Pure-Rust RustCrypto crates (`x25519-dalek`, `chacha20poly1305`, `rand_core`) rather than `libsodium-sys` so iOS/Android/emscripten all compile without a C toolchain dep. Every byte field on the wire is lowercase hex.
3. **Builder + kernel wiring** — `DaemonBuilder` gains `set_vfs_manager/vfs_manager_slot/with_vfs/with_crypto`; `DaemonKernel` gains a `vfs_manager()` accessor; `with_defaults()` now installs vfs + crypto alongside crdt/luau/build/watcher when their features are active.
4. **Feature matrix** — `Cargo.toml` adds `vfs = ["dep:sha2", "dep:hex"]` and `crypto = ["dep:chacha20poly1305", "dep:x25519-dalek", "dep:rand_core", "dep:hex"]`. `full` and `wasm` pull them both in; `mobile` pulls them both in (E2EE + blob store are the whole point of mobile sync); `embedded` stays at crdt-only so a future no_std-leaning target remains viable.
5. **Stdio-binary E2E test** (`tests/stdio_bin.rs`, 2 tests) — spawns `prism-daemond` as a subprocess and drives crdt/luau/vfs/crypto commands through real stdin/stdout JSON. The CLI-transport analogue of the Playwright browser suite.
6. **Playwright suite extended** (`e2e/wasm.spec.ts`) — seven new tests covering vfs put/get/delete/list/stats and crypto keypair/shared_secret/encrypt/decrypt/AEAD-tamper/random_bytes through the real emscripten C ABI inside Chromium. Total: 19 tests × 2 profiles (dev/prod) = 38 browser runs.
7. **Kernel integration tests extended** (`tests/kernel_integration.rs`) — `with_defaults_installs_every_feature_module` now covers vfs/crypto command registration; `installed_modules_reports_install_order` rewritten to walk only the shortcuts compiled into the current feature set, so mobile/embedded/wasm runs exercise the real builder path; added `vfs_blob_store_roundtrips_through_kernel_invoke` and `crypto_keypair_ecdh_and_aead_flow_through_kernel` for end-to-end coverage through `kernel.invoke()`.
8. **Full-matrix runner** (`scripts/test-all.sh`) — orchestrates host cargo tests for all four feature combos, clippy under each combo, `cargo fmt --check`, WASM dev+prod cross-compile, Playwright dev+prod, iOS xcframework build + C ABI symbol check (`_prism_daemon_{create,destroy,invoke,free_string}`), Android per-ABI cdylib build + matching symbol check. `--skip-mobile/--skip-e2e/--skip-wasm` let you trim the run when iterating.
9. **Docs refreshed** — `packages/prism-daemon/CLAUDE.md` now lists the new modules, the expanded feature matrix, the full test inventory, the matrix runner, and the mobile FFI sanity check procedure.

### Status

- `cargo test` (default) — **60 lib + 9 integration + 2 stdio_bin = 71 tests passing**
- `cargo test --no-default-features --features mobile` — **50 lib + 9 integration = 59 passing**
- `cargo test --no-default-features --features embedded` — **11 lib + 7 integration = 18 passing**
- `cargo test --no-default-features --features wasm --lib` — **50 tests passing**
- `cargo clippy --all-targets -- -D warnings` — clean across every feature combo
- `cargo fmt --check` — clean
- `scripts/build-wasm.sh {dev,prod}` — prod build is 3.4 MB wasm + 67 KB JS glue
- `scripts/build-ios.sh` — xcframework assembled, device slice nm-verified for the C ABI
- `scripts/build-android.sh debug` — libprism_daemon.so + libc++_shared.so staged into jniLibs for arm64-v8a / armeabi-v7a / x86_64, all nm-verified
- Playwright — **19 dev + 19 prod = 38 passing**, exercising every command (crdt/luau/vfs/crypto) through the real emscripten C ABI inside Chromium
- Mobile scripts each build under `--features mobile` and ship the `prism_daemon_{create,destroy,invoke,free_string}` C ABI symbols.

### Notes / deferred

- Hardware protocol bridges (Art-Net/sACN, VISCA, DMX, OSC, MIDI) and the actor/process queue (Whisper, Python sidecars) from SPEC §2 are still on the roadmap — they need desktop-only system libraries that would break the mobile/embedded/wasm surfaces we just finished certifying. They'll land as feature-gated desktop-only modules (`hardware`, `actors`) when the first consumer needs them.
- No ESP32 simulator was exercised. `embedded = ["crdt"]` is still the placeholder for that path; `cargo test --features embedded` runs on the host toolchain and validates the minimal kernel compiles without any desktop-only dep leaking in.
- VFS currently uses `std::fs` under a configurable root. A future S3 / GCS / content-delivery adapter would plug in via `builder.set_vfs_manager(...)` exposing the same command surface.

## ADR 002 Phase 1 — PrismFile + LanguageContribution (Complete — 2026-04-11)

Phase 1 of ADR 002 introduced the two new abstractions (§A1 and §A2). Phase 4 (see entry above) collapsed the legacy registries and retired the compat bridge.

### What landed

1. **`packages/prism-core/src/language/document/prism-file.ts`** — `PrismFile` + `FileBody` discriminated union (`text` | `graph` | `binary`) plus `createTextFile`/`createGraphFile`/`createBinaryFile` constructors and `isTextBody`/`isGraphBody`/`isBinaryBody` narrowing guards. Imports `LoroText`, `GraphObject`, `BinaryRef`, and `DocumentSchema` — the first type to formally tie all of them together.
2. **`packages/prism-core/src/language/registry/language-contribution.ts`** — `LanguageContribution<TRenderer, TEditorExtension>` interface covering id/extensions/displayName/mimeType, optional parse/serialize/syntaxProvider/codemirrorExtensions, a `LanguageSurface<TRenderer>` (defaultMode/availableModes/inlineTokens/renderers) and optional `LanguageCodegen`. Generic over renderer + editor extension types so `@prism/core/language` stays React- and CodeMirror-free; Studio specializes later.
3. **`packages/prism-core/src/language/registry/compat.ts`** — the compatibility bridge. `contributionFromLegacy(language, surface)` adapts any existing `LanguageDefinition` + `DocumentContributionDef` pair (either half may be null) into a `LanguageContribution`. `resolveContribution({ languages, surfaces, filename, documentType, languageId })` resolves both registries from common keys and returns a unified view. Drops diagnostics on parse (Phase 4 reconnects them) and falls back to a code-only surface when the language has no registered `DocumentContributionDef`.
4. **Subpath exports** — `@prism/core/document` and `@prism/core/language-registry` added to `packages/prism-core/package.json`. Tsconfig paths already cover `src/language/*`, so no tsconfig change was needed.
5. **Tests** — 20 new tests across `prism-file.test.ts` (10) and `compat.test.ts` (10). Covers: body constructors, narrowing guards, exhaustive switch over `FileBody`, schema + metadata carry-through, round-trip parse/serialize through the bridge, missing-surface fallback, missing-language pass-through, explicit-documentType override, unknown-filename null return, and the bridge throwing when both inputs are null.
6. **Docs** — `packages/prism-core/CLAUDE.md` + `README.md` list `document/` and `registry/` under the `language/` category with ADR-002 §A1/§A2 cross-references.

### Status

- `pnpm --filter @prism/core typecheck` — green.
- `pnpm test` — **187 test files, 3663 tests passing** (+20 from 185/3643 pre-Phase-1).
- `PrismFile` and `LanguageContribution` are now consumed by Phase 2 (Luau fold) and Phase 4 (registry collapse) — see ADR 002 Phases 2–4 entry above.

## ADR 002 Structural Reorganization (Complete — 2026-04-11)

Moved `@prism/core` from its old `src/layer1/` + `src/layer2/` binary split into **8 domain categories** under `src/`:

```
foundation → language/identity → kernel/network → interaction/domain → bindings
```

- `foundation/` — pure data: object-model, persistence, vfs, crdt-stores, batch, clipboard, template, undo, loro-bridge
- `language/` — expression, forms, syntax, luau, facet
- `kernel/` — actor, automation, builder, config, plugin, plugin-bundles, state-machine
- `interaction/` — atom, layout, lens, input, activity, notification, search, view (React-free)
- `identity/` — did, encryption, trust, manifest
- `network/` — relay, presence, session, discovery, server
- `domain/` — flux, graph-analysis, timeline
- `bindings/` — codemirror, puck, kbar, xyflow, react-shell, viewport3d, audio (the only layer allowed to import React / DOM / WebGL)

### What landed

1. **8 category directories** created under `packages/prism-core/src/`; every subsystem moved with `git mv` so history is preserved.
2. **Path aliases extended** in `packages/prism-core/tsconfig.json` so `@prism/core/<subsystem>` resolves inside the package as well as from consumers. `rootDir: ".."` fixes TS2209 now that `package.json` no longer has a `"."` entry.
3. **Vitest uses `vite-tsconfig-paths`** (`vitest.config.ts`) instead of a 60-line hand-maintained alias list.
4. **Cross-category relative imports eliminated.** Two scripts under `/tmp/` rewrote 115/383 files to `@prism/core/<subsystem>` form, then corrected folder-name → public-export mismatches (`did→identity`, `crdt-stores→stores`, `react-shell→shell`, `xyflow→graph`). Intra-category sibling imports stay relative.
5. **`@prism/core/layer1` barrel retired.** Studio kernel, panels, and tests split their old catch-all imports into specific subsystem imports (`@prism/core/plugin-bundles`, `@prism/core/facet`, `@prism/core/view`, `@prism/core/manifest`, `@prism/core/luau`, `@prism/core/flux`). The `layer1` and `layer2` subpath exports no longer exist.
6. **Docs refreshed** — `packages/prism-core/CLAUDE.md`, `packages/prism-core/README.md`, `SPEC.md` §1, root `README.md`, `packages/prism-studio/CLAUDE.md`/`README.md`, `packages/prism-relay/CLAUDE.md` now describe the 8-category structure instead of the Layer 1 / Layer 2 framing.

### Status

- `pnpm typecheck` — green across all 6 packages.
- `pnpm test` — **185 test files, 3643 tests passing**.
- All four ADR-002 phases (1–4) are now complete on top of this structure.

### Note on historical phase entries below

The phase entries below this section were written when the codebase was still split into `layer1/` and `layer2/`. Paths like `layer1/syntax/...`, `layer2/viewport3d/...`, etc. are **historical**. The canonical current paths live under the 8 domain categories and are tracked in the subpath export table in `packages/prism-core/README.md`.

## Luau full-moon AST Integration (Complete — 2026-04-10)

Ripped regex-based Luau parsing out of panels + debugger, replaced with a
lossless AST via Kampfkarren/full-moon compiled to WASM. Plugged into the
Helm-inherited syntax/codegen system as a `LanguageDefinition` +
`SyntaxProvider` rather than a standalone module.

### What landed

1. **Rust crate `packages/prism-core/native/luau-parser/`** wraps `full_moon`
   (v1.2, `luau` feature) via `wasm-bindgen` + `serde-wasm-bindgen`. Exposes
   four functions: `parse`, `findUiCalls`, `findStatementLines`, `validate`.
   Serializable `JsSyntaxNode`/`UiCall`/`UiArg`/`Diagnostic` types mirror the
   shapes Prism's normalizers expect. 8 native Rust tests cover statement line
   detection (including if/while/for recursion and multi-line strings),
   `ui.*(...)` extraction with nested children, and parse error reporting.
2. **WASM build** via `wasm-pack build --target web` — 248KB optimized
   `prism_luau_parser_bg.wasm` + glue committed under
   `src/layer1/syntax/luau/pkg/` so Vitest/Vite don't need the Rust toolchain.
3. **TS wrappers at `packages/prism-core/src/layer1/syntax/luau/`**:
   - `wasm-loader.ts` — idempotent async init with Node/browser environment
     detection (Node uses `fs/promises.readFile`, browser uses `fetch`);
     `ensureLuauParserLoaded` / `getLuauParserSync` / `isLuauParserReady`
   - `luau-ast.ts` — async + sync helpers (`parseLuau`, `findUiCalls`,
     `findStatementLines`, `validateLuau`) with defensive normalizers for the
     untyped wasm-bindgen output
   - `luau-language.ts` — `createLuauLanguageDefinition()` implements the sync
     `LanguageDefinition` interface; reports init-not-ready or parser errors
     into `ProcessorContext.diagnostics` and returns an empty root so the
     pipeline continues
   - `luau-provider.ts` — `createLuauSyntaxProvider()` implements the
     `SyntaxProvider` interface with AST-backed `diagnose()` and a 9-item
     `ui.*` completion list surfaced after `ui.`
   - `index.ts` — convenience `initLuauSyntax()` + public re-exports
4. **Re-exports** from `@prism/core/syntax` covering every public Luau symbol
   so consumers never import `./luau/...` paths directly.
5. **26 Vitest tests** in `luau-ast.test.ts` covering loader idempotency,
   `findUiCalls` (flat / nested children / empty source / parser errors /
   sync), `findStatementLines` (flat / if-else recursion / multiline string
   resilience / sync), `validateLuau` (clean + error), `parseLuau`, the
   `LanguageDefinition` (id/extensions, clean parse, error reporting), and
   the `SyntaxProvider` (name, diagnose, ui.* completions, hover=null).
6. **`luau-facet-panel.tsx` rewrite** — deleted ~270 lines of hand-rolled
   parser (`parseNodeList` / `parseCall` / `parseLeafCall` /
   `parseSectionCall` / `parseContainerCall` / `parseVoidCall` /
   `parseString` / `skipWhitespaceAndComments` / `skipToClosingParen`).
   `parseLuauUi(source)` is now a thin sync adapter: `findUiCallsSync` →
   `uiCallToNode` → `UINode` tree. Positional args are unpacked onto named
   `props` based on kind (label/button→text, badge→text+color,
   input→placeholder+value, section→title, etc). The `UINode` /
   `ParseResult` / `renderUINode` exports are preserved so `canvas-panel`
   and `layout-panel` continue to work unchanged.
7. **Async-init React hook** — the module kicks off `initLuauSyntax()` at
   load time, and a new `useLuauParserReady()` hook (built on
   `useSyncExternalStore`) flips from `false` to `true` once the WASM
   parser is ready. `LuauFacetPanel`, `canvas-panel`'s `LuauBlockRenderer`,
   and a new `PuckLuauBlockRender` component (extracted from
   `layout-panel` so the Puck render callback is a real component) all
   subscribe to it and re-render once the parser is live.
8. **12 Vitest tests** in `packages/prism-studio/src/panels/luau-facet-panel.test.ts`
   cover the `parseLuauUi` adapter: empty source, every element kind with
   its positional-arg → named-prop mapping (label/button/badge/input/
   section/row/column/spacer/divider), nested children, parser errors,
   and comments.
9. **`luau-debugger.ts` instrumentation rewrite** — `instrumentSource` is
   now async and built on `findStatementLines` from `@prism/core/syntax`.
   It only injects `__prism_trace(n)` on lines that begin a Luau statement
   per the full-moon AST, so (a) multi-line string literals no longer
   receive spurious trace calls inside their continuation lines, and
   (b) multi-line statements are traced once at their first line instead
   of on every continuation. `buildScript` cascades async. 3 regression
   tests in `luau-debugger.test.ts` cover multi-line strings, multi-line
   function calls, and nested if/then/else statements.

Full suite: **3643 tests** passing (up from 3602).

## Lua → Luau Migration (Complete — 2026-04-10)

Full codebase migration from Lua 5.4 (wasmoon) to Luau (luau-web / mlua+luau):

- **Browser runtime**: replaced `wasmoon` with `luau-web` (`LuauState.createAsync`)
- **Daemon runtime**: mlua feature flag `lua54` → `luau`; added `Value::Integer` handling; switched to `into_function().call()` for reliable multi-return capture
- **IPC**: `lua_exec` → `luau_exec`; `prism.lua` → `prism.luau`; `lua.exec` → `luau.exec`
- **Debugger**: source instrumentation approach (`__prism_trace`) unchanged; guarded `debug.getlocal` for environments where it's unavailable (luau-web sandbox)
- **Types**: `LuaResult` → `LuauResult`, `LuaExecRequest` → `LuauExecRequest`, all `Lua*` public types renamed to `Luau*`
- **Exports**: `@prism/core/lua` → `@prism/core/luau`; `.d.lua` → `.d.luau`
- **Tests**: 3602 TS + 33 Rust unit + 7 integration + 12 wasm E2E — all green

## Prism Daemon: Cross-Platform Kernel + DI Builder (Complete — 2026-04-08)

Goal: port Studio's self-replicating kernel paradigm to `prism-daemon` so the
same Rust engine can run on any device — desktop (Tauri), mobile
(Capacitor/FFI), headless (CLI) — with modules plugged in via a fluent
builder instead of being hardcoded.

### What landed

1. **`CommandRegistry` (`src/registry.rs`)** — the transport-agnostic IPC
   layer. Maps name → `Arc<dyn Fn(JsonValue) -> Result<JsonValue, CommandError>>`.
   Every transport adapter (Tauri `#[command]`, UniFFI, stdio CLI, future
   HTTP) funnels through `kernel.invoke(name, payload)`. Mirrors Studio's
   `LensRegistry` role.
2. **`DaemonModule` trait (`src/module.rs`)** — Rust analogue of
   `LensBundle` / `PluginBundle`. `install(&self, builder: &mut DaemonBuilder)`
   self-registers the module's commands + stashes any shared service on the
   builder.
3. **`DaemonInitializer` trait + `InitializerHandle` (`src/initializer.rs`)**
    — post-boot side-effect hooks, equivalent of `StudioInitializer`. Run
   in install order after the kernel exists, torn down in reverse on
   `dispose()`.
4. **`DaemonBuilder` (`src/builder.rs`)** — fluent builder:
   `DaemonBuilder::new().with_crdt().with_luau().with_build().with_watcher()
   .with_module(custom).with_initializer(init).build()`. `with_defaults()`
   installs every module the current feature flags allow. Tauri/CLI/mobile
   all use the identical shape.
5. **`DaemonKernel` (`src/kernel.rs`)** — cheaply-cloneable runtime
   (everything behind `Arc`). Exposes `invoke`, `capabilities`,
   `installed_modules`, `doc_manager()`, `watcher_manager()`, `dispose()`.
   Hot paths can skip JSON round-trips by grabbing the `Arc<DocManager>`
   directly — same idea as Studio's direct `kernel.store` access.
6. **Feature-gated built-in modules (`src/modules/*`)**:
   - `crdt_module.rs` → `prism.crdt` → `crdt.{write,read,export,import}`
   - `luau_module.rs` → `prism.luau` → `luau.exec`
   - `build_module.rs` → `prism.build` → `build.run_step` (emit-file /
     run-command / invoke-ipc)
   - `watcher_module.rs` → `prism.watcher` → `watcher.{watch,poll,stop}`
     backed by a new `WatcherManager` that multiplexes `notify`
     subscriptions by ID.
7. **Feature matrix** (`Cargo.toml`):
   - `full` (default) — every capability, enables the CLI bin
   - `mobile` — `crdt + luau` only (iOS bans process spawning, no notify)
   - `embedded` — `crdt` only (minimum kernel)
   Individual flags: `crdt`, `luau`, `build`, `watcher`, `cli`. Mobile /
   embedded builds don't contain the code they can't run.
8. **`DocManager` extracted to `src/doc_manager.rs`** behind the `crdt`
   feature. Injectable via `builder.set_doc_manager(Arc<DocManager>)` so
   hosts can preload docs from disk before booting the kernel.
9. **Standalone `prism-daemond` bin (`src/bin/prism_daemond.rs`)** —
   minimal stdio-JSON loop proving the kernel runs detached from Tauri.
   Emits a capabilities banner on startup; reads one JSON request per
   line; exposes `daemon.capabilities` + `daemon.modules` alongside
   module-contributed commands.
10. **Studio Tauri shell migrated** (`packages/prism-studio/src-tauri/
    src/{main,commands}.rs`). `main.rs` constructs the kernel via the
    builder and `.manage()`s `Arc<DaemonKernel>`. Tauri commands forward
    to `kernel.invoke(...)` for generic paths and to
    `kernel.doc_manager()` for the CRDT hot path. New
    `daemon_capabilities` command exposes kernel introspection to the
    frontend. Old `prism_daemon::commands::*` legacy shim was removed
    entirely — call sites now import from `prism_daemon::modules::*`.
11. **Tests**: 33 unit tests across `registry` + `modules` + 7 integration
    tests in `tests/kernel_integration.rs` covering builder composition,
    custom modules, initializer ordering, kernel clone/share semantics,
    and empty-kernel behavior. `cargo clippy --all-targets -- -D warnings`
    passes. `cargo build --no-default-features --features mobile` and
    `--features embedded` both compile.

### Studio ↔ Daemon mapping

| Studio (TS)              | Daemon (Rust)            |
|--------------------------|--------------------------|
| `createStudioKernel`     | `DaemonBuilder::build`   |
| `LensBundle`             | `DaemonModule`           |
| `StudioInitializer`      | `DaemonInitializer`      |
| `LensRegistry`           | `CommandRegistry`        |
| `installLensBundles`     | `DaemonBuilder::with_module` |
| `installInitializers`    | `DaemonBuilder::with_initializer` |
| `kernel.dispose()`       | `kernel.dispose()`       |
| `kernel.shellStore`      | `kernel.doc_manager()` + `kernel.watcher_manager()` |

The self-replicating paradigm now spans the entire stack: Studio composes
apps + kernels with bundles + initializers; the Daemon composes
capabilities with modules + initializers; both emit builds via the same
`BuildStep` wire format flowing through the build module.

---

## Kernel Composition & Self-Registering Bundles (Complete — 2026-04-08)

Goal: make layers flow strictly bottom-up. Apps don't know about lenses,
lenses don't know about apps, templates/seed data don't live in `App.tsx`.
Closes every DI / registration gap between the host and its subsystems.

### What landed

1. **`LensBundle` in `@prism/core/lens`**. New `lens-install.ts` adds
   `LensBundle<TComponent>`, `LensInstallContext<TComponent>`,
   `installLensBundles`, and `defineLensBundle`. Generic over component
   type so Layer 1 stays React-free. Covered by `lens-install.test.ts`
   (5 cases). Re-exported from `@prism/core/lens/index.ts`.
2. **Studio React specialization** (`src/lenses/bundle.ts`). A thin
   `ComponentType`-pinned re-export of the generic primitive so panels
   get a `LensBundle` / `defineLensBundle` with the correct component
   type without leaking React into Layer 1.
3. **40 self-registering panels**. Every file in `src/panels/*.tsx` now
   exports both its React component and its `xxxLensBundle` right next
   to the manifest. `src/lenses/index.tsx` collapsed from a manifest +
   component-map aggregator into a 40-line bundle aggregator with a
   single `createBuiltinLensBundles()` export.
4. **Kernel owns the lens lifecycle**. `createStudioKernel()` now takes
   `{ lensBundles?, initializers? }`. It creates its own `LensRegistry`,
   `lensComponents: Map<LensId, ComponentType>`, and `shellStore`, runs
   `installLensBundles`, and exposes all three as kernel fields.
   `dispose()` unwinds bundle disposers in reverse order.
5. **`StudioInitializer` pipeline**. New `kernel/initializer.ts`
   (`install({ kernel }) => uninstall` interface + `installInitializers`
   helper). Initializers run AFTER the kernel's return object exists, so
   they can freely call `kernel.registerTemplate`, `kernel.createObject`,
   etc. Symmetric with `PluginBundle` / `LensBundle` but scoped to
   post-boot side effects.
6. **`kernel/builtin-initializers.ts`**. Moves the old top-of-`App.tsx`
   seeders into three self-installing bundles:
   - `pageTemplatesInitializer` — blog + landing page `ObjectTemplate`s
   - `sectionTemplatesInitializer` — delegates to
     `registerSectionTemplates(kernel)`
   - `demoWorkspaceInitializer` — seeds Home + About pages into an empty
     store, clears undo history, selects the home page
   Exposed via `createBuiltinInitializers()`.
7. **`App.tsx` collapsed from ~400 → ~140 lines**. No more
   `seedDemoData`, `registerSeedTemplates`, or parallel lens-registry
   wiring. Just:
   ```ts
   const kernel = createStudioKernel({
     lensBundles: createBuiltinLensBundles(),
     initializers: createBuiltinInitializers(),
   });
   const { lensRegistry, lensComponents, shellStore } = kernel;
   ```
8. **Export surface**. `src/kernel/index.ts` re-exports
   `StudioKernelOptions`, `StudioInitializer`, `StudioInitializerContext`,
   `installInitializers`, `createBuiltinInitializers`, and the three
   individual initializers so profile-aware hosts can cherry-pick.

### Verification

- `pnpm --filter @prism/core typecheck` clean
- `pnpm --filter @prism/studio typecheck` clean
- `pnpm --filter @prism/studio exec vitest run src/kernel src/lenses` —
  254 tests green (including 113 `studio-kernel.test.ts` cases against
  the new options shape)
- `pnpm exec vitest run packages/prism-core/src/layer1/lens` —
  33 tests green across `lens-install.test.ts`, `lens-registry.test.ts`,
  `shell-store.test.ts`
- The only remaining red in `pnpm --filter @prism/studio test` is
  Vitest accidentally picking up `e2e/*.spec.ts` Playwright files —
  pre-existing glob config issue, unrelated to this work.

### Docs updated

- `SPEC.md` — new `Kernel Composition & Self-Registering Bundles`
  subsection inside "Studio as a Self-Replicating Meta-Builder";
  bundle-kind table; App Profile filter bullet now lists `LensBundle`s
  and `StudioInitializer`s alongside `PluginBundle`s.
- `README.md` — Layer 1 Lens Shell row mentions
  `LensBundle`/`installLensBundles`/`defineLensBundle`; new Philosophy
  bullet: "Layers Flow Bottom-Up".
- `packages/prism-core/CLAUDE.md` — `@prism/core/lens` description
  covers the new bundle primitives and the Layer-1 React-free rationale.
- `packages/prism-studio/CLAUDE.md` — kernel description rewritten
  around `createStudioKernel({ lensBundles, initializers })`; new
  `initializer.ts` + `builtin-initializers.ts` kernel entries; Lenses
  section rewritten around the self-registering bundle pattern with a
  2-step "adding a new lens" recipe.
- `docs/dev/current-plan.md` — this section.

---

## Studio Checklist — Full Closeout (Complete — 2026-04-08)

Every tier in `docs/dev/studio-checklist.md` is now implemented, wired into a
registered lens, and exercised by both vitest unit tests and Playwright E2E
specs. Totals: 3513 unit tests across 174 files, `tsc --noEmit` clean
workspace-wide.

### New in this sprint

- **3E Design Tokens** — `design-tokens-panel.tsx` + `kernel/design-tokens.ts`
  (shift+T). CSS variables for colors/spacing/fonts.
- **4A/4B/4C/4D Expressions + Bindings** — `inspector-panel.tsx`
  `ComputedFieldDisplay` runs `EntityFieldDef.expression` via
  `@prism/core/expression`; the Expression Bar drives a
  `createSyntaxEngine()` completion dropdown; `kernel/data-binding.ts`
  handles `[obj:pageTitle]` resolution and `visibleWhen` gating on the canvas.
- **5B/5D Section Templates + Save-as-Template** —
  `kernel/section-templates.ts` registers six blueprints; Inspector exposes
  "Save as Template" via new `studio-kernel.templateFromObject()`.
- **7A Rich Text Toolbar** — `editor-panel.tsx` Markdown toolbar, backed by
  the pure `computeMarkdownEdit()` helper (7 vitest cases).
- **7D Media / VFS Upload** — `assets-panel.tsx` `handleImportBinary()` reads
  `File` → VFS → auto-creates image blocks when the parent is a
  section/page.
- **8B Form Builder** — `form-builder-panel.tsx` (shift+G). Composes form
  inputs under the nearest container ancestor; walks parent chain.
- **8D Multi-Page Nav** — `site-nav-panel.tsx` + `siteNavDef` /
  `breadcrumbsDef` in `kernel/entities.ts`. Pure `buildSiteNav()` helper
  covered by `site-nav-panel.test.ts`.
- **8E Peer Cursors** — `components/peer-cursors-overlay.tsx` renders
  `PeerCursorsBar` at the top of the canvas and exports
  `PeerSelectionBadge` + a pure `groupPeerSelections()` helper. Driven
  entirely by `usePresence()`. Tests: `peer-cursors-overlay.test.ts`.
- **9A Entity Builder** — `entity-builder-panel.tsx` (shift+E). UI for
  authoring `EntityDef`s at runtime and registering them into
  `kernel.registry`.
- **9B Relationship Builder** — `relationship-builder-panel.tsx` (shift+R).
  UI for authoring `EdgeTypeDef`s (behavior / color / source-target type
  restrictions). Uses conditional assignment for `exactOptionalPropertyTypes`.

### E2E coverage

`e2e/new-panels.spec.ts` (new) adds Playwright coverage for design-tokens,
form-builder, site-nav, entity-builder, relationship-builder, publish, and
the canvas peer-cursors bar. Pattern: open the lens via its activity icon,
assert the panel `data-testid` is visible, then interact with at least one
key control (e.g. add a draft field in the Entity Builder).

### Docs updated

- `docs/dev/studio-checklist.md` — every Tier 3-9 item flipped to `[x]`
  with file references; added Verification section.
- `packages/prism-studio/CLAUDE.md` — new panel list entries.
- `docs/dev/current-plan.md` — this section.

---

## Styling System + Publish Workflow + Rich Media (Complete — 2026-04-08)

Three more checklist tiers land together because they all share the same
block-style foundation: a per-block `BlockStyleData` bag that every renderer
and the HTML exporter apply uniformly.

### Tier 3 — Block styling + typography (`block-style.ts`)

- [x] `BlockStyleData` shape — background, text color, padding/margin
  (X/Y), border (width/color/radius), shadow (preset or raw), font
  (family/size/weight/line-height/letter-spacing), text-align, flex
  (display/direction/gap/align/justify)
- [x] `STYLE_FIELD_DEFS` — single shared array spread into the existing
  `section`, `heading`, `text-block`, `button`, and `card` entity defs
  so the inspector now shows `Style` and `Typography` groups for every
  core block. Replaces the old section-only `padding`/`background` enums.
- [x] `computeBlockStyle()` — pure bag → `CSSProperties` (shadow preset
  resolver, stringy-number coercion, 0-padding tolerance)
- [x] `extractBlockStyle()` / `mergeCss()` — extractor + merger used by
  both `BlockWrapper` and `SectionBlock` in `canvas-panel.tsx` so every
  block picks up the author's style overrides on top of the base CSS

### Tier 6 — Publish & Export (`page-export.ts`, `publish-panel.tsx`)

- [x] **6A HTML Export** — `exportPageToHtml()` walks the page tree
  into dependency-free HTML + default inline CSS, with per-node
  `blockStyleAttr()` emitting sanitized inline styles, safe for offline
  viewing. Escapes all text + attributes (`escapeHtml`/`escapeAttr`).
  Supports `fragmentOnly` output and CSS overrides.
- [x] **6B JSON Export** — `exportPageToJson()` emits a deterministic
  `prism-page/v1` snapshot (`ExportedNode` tree with inlined children in
  position order, deleted objects skipped, data cloned).
- [x] **6C Publish Workflow** — `nextStatus()` / `statusColor()` pure
  helpers drive a `draft → review → published` transition on the page's
  `status` field. Advance / Back-to-Draft buttons in the Publish panel
  update the kernel object, with colored status pills.
- [x] **6D Preview Mode** — Publish panel toggles an inline preview that
  runs the same `renderNodeHtml()` pipeline as the exporter (so authors
  see exactly what the exported HTML contains).
- [x] **Publish lens** — Lens #29, `Shift+U`, rocket icon 🚀, registered
  in `lenses/index.tsx`. Resolves the current page from selection by
  walking parentId, drops a placeholder when nothing is selected.

### Tier 7C / 7D — Code + Media blocks

- [x] **code-block** — `CodeBlockRenderer` with language label, caption,
  line-number gutter (auto-width), wrap toggle, dark theme, dependency-
  free pre/code. Pure helpers `splitCodeLines()` / `gutterWidth()`.
- [x] **video-widget** — `VideoWidgetRenderer` backed by native HTML5
  video, poster + caption + width/height (clamped), controls/autoplay/
  loop/muted flags. Refuses non-http(s) URLs via `isSafeMediaUrl`.
- [x] **audio-widget** — `AudioWidgetRenderer` backed by native HTML5
  audio, same URL allow-list + captions + control flags.
- [x] Three new defs registered in `entities.ts`, wired into Puck
  (`layout-panel.tsx`) and the canvas switch (`canvas-panel.tsx`), and
  covered by the widget-registration check in `studio-kernel.test.ts`.
- [x] HTML exporter knows about `code-block`, `video-widget`,
  `audio-widget`, `iframe-widget`, `markdown-widget`, `divider`,
  `spacer` so exported pages render all the same content as the canvas.

### Files touched

- `packages/prism-studio/src/kernel/block-style.ts` (new)
- `packages/prism-studio/src/kernel/block-style.test.ts` (new)
- `packages/prism-studio/src/kernel/page-export.ts` (new)
- `packages/prism-studio/src/kernel/page-export.test.ts` (new)
- `packages/prism-studio/src/panels/publish-panel.tsx` (new)
- `packages/prism-studio/src/panels/publish-panel.test.ts` (new)
- `packages/prism-studio/src/components/code-block-renderer.tsx` (new)
- `packages/prism-studio/src/components/code-block-renderer.test.ts` (new)
- `packages/prism-studio/src/components/media-renderers.tsx` (new)
- `packages/prism-studio/src/components/media-renderers.test.ts` (new)
- `packages/prism-studio/src/kernel/entities.ts` — spread
  `STYLE_FIELD_DEFS` into section/heading/text-block/button/card; add
  `code-block`, `video-widget`, `audio-widget` defs
- `packages/prism-studio/src/panels/canvas-panel.tsx` — drop legacy
  `PADDING_MAP`, apply `computeBlockStyle()` + `mergeCss()` in
  `BlockWrapper` and `SectionBlock`, add block components + switch
  cases for the three new rich-content types
- `packages/prism-studio/src/panels/layout-panel.tsx` — Puck config
  entries for the three new rich-content types
- `packages/prism-studio/src/lenses/index.tsx` — register Publish lens
- `packages/prism-studio/src/kernel/studio-kernel.test.ts` — extend
  registration check to cover code-block/video-widget/audio-widget

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (`block-style`) | 15 | Pass |
| Vitest (`page-export`) | 16 | Pass |
| Vitest (`publish-panel`) | 6 | Pass |
| Vitest (`code-block-renderer`) | 6 | Pass |
| Vitest (`media-renderers`) | 8 | Pass |
| **Vitest (full repo)** | **3461** | **Pass (168 files)** |
| `tsc --noEmit` (studio) | — | Clean |

## App Builder Widgets — Form / Layout / Display / Content (Complete — 2026-04-08)

Prism Studio now ships 15 additional drag-and-drop Puck widgets so that
non-programmers can assemble real apps (forms, dashboards, docs) without
reaching for code. All widgets follow the established pattern: entity
def in `entities.ts` → renderer in `components/` → Puck config in
`layout-panel.tsx` → canvas block + switch case in `canvas-panel.tsx`.

### Form Inputs (6)

- [x] **text-input** — label, placeholder, default value, input type
  (text/email/url/tel/password), required flag, help text
- [x] **textarea-input** — multi-line with configurable rows
- [x] **select-input** — dropdown; options accept either `a,b,c`,
  `value:Label` pairs, or a JSON array
- [x] **checkbox-input** — labeled boolean
- [x] **number-input** — min/max/step/default
- [x] **date-input** — date / datetime-local / time kinds

### Layout Primitives (3)

- [x] **columns** — 1-6 column grid with gap + cross-axis alignment,
  empty-state placeholders so the widget is visible on first drop
- [x] **divider** — solid/dashed/dotted with thickness, color,
  spacing, optional centered label
- [x] **spacer** — vertical or horizontal gap, clamped to 0-512px

### Data Display (4)

- [x] **stat-widget** — KPI card computing count/sum/avg/min/max over
  `kernel.store.allObjects()` filtered by `collectionType`, with
  prefix/suffix, decimals, and thousands separator
- [x] **badge** — neutral/info/success/warning/danger tones, optional
  emoji icon, solid or outline
- [x] **alert** — callout box with title + message, the same tone
  palette, auto-chosen icon per tone
- [x] **progress-bar** — labeled bar with percent display and tone
  color, value/max clamped to [0,1]

### Content (2)

- [x] **markdown-widget** — dependency-free markdown → HTML with
  headings, lists, blockquotes, fenced code blocks, horizontal rules,
  inline bold/italic/code/links (escaped)
- [x] **iframe-widget** — embed http(s) URL with sandbox attrs
  (`allow-scripts allow-same-origin allow-forms allow-popups`),
  javascript/data/file schemes rejected

### Files touched

- `packages/prism-studio/src/components/form-input-renderers.tsx` (new)
- `packages/prism-studio/src/components/layout-primitive-renderers.tsx` (new)
- `packages/prism-studio/src/components/data-display-renderers.tsx` (new)
- `packages/prism-studio/src/components/content-renderers.tsx` (new)
- `packages/prism-studio/src/components/*.test.ts` (4 new test files)
- `packages/prism-studio/src/kernel/entities.ts` — 15 new EntityDefs
  registered
- `packages/prism-studio/src/panels/layout-panel.tsx` — 15 new Puck
  component configs with live-bound renderers
- `packages/prism-studio/src/panels/canvas-panel.tsx` — 15 new canvas
  block components + switch cases in `ComponentBlock`
- `packages/prism-studio/src/kernel/studio-kernel.test.ts` — registry
  coverage extended

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (form-input-renderers) | 7 | Pass |
| Vitest (layout-primitive-renderers) | 6 | Pass |
| Vitest (data-display-renderers) | 14 | Pass |
| Vitest (content-renderers) | 15 | Pass |
| Vitest (studio-kernel — new widget registration) | +1 | Pass |
| **Vitest (full repo)** | **3402** | **Pass** |
| `tsc --noEmit` (studio) | — | Clean |

## Foundation Widgets + Core Enhancements (Complete — 2026-04-08)

Expansion of Prism's foundation: expression/field primitives, document surface
completion, a suite of composable Puck widgets, and relay-side template/email
endpoints. All new views land as drag-and-drop Puck widgets in Layout/Canvas —
the legacy ViewMode registry is dead.

### Phase A — Layer 1 primitives

- [x] **A1** Expression builtins — string (`len`, `lower`, `upper`, `trim`, `concat`, `left`, `right`, `mid`, `substitute`), date (`today`, `now`, `year`, `month`, `day`, `datediff`), aggregate (`sum`, `avg`, `count`) wired into `evaluator.ts` + `syntax-engine.ts` autocomplete
- [x] **A2** `"formula"` field type with `expression` body in `FieldSchema`
- [x] **A3** `"lookup" | "rollup"` entity field types + `field-resolver.ts` dispatcher for formula/lookup/rollup computation across edge relations
- [x] **A4** `FacetSlot` extended with `tab` / `popover` / `slide` container kinds; `FacetDefinitionBuilder.addTabContainer()` / `addPopoverContainer()` / `addSlideContainer()`
- [x] **A5** `EmailAction` (`email:send`) with `{{field}}` interpolation alongside existing automation actions
- [x] **A6** `"stream"` added to `EdgeBehavior` union

### Phase B — Document Surface completion

- [x] **B1** `FormSurface` — YAML/JSON source → auto-derived field schema → round-trip form
- [x] **B2** `CsvSurface` — quoted fields, TSV autodetect, contentEditable table, add/delete row/column
- [x] **B3** `ReportSurface` — grouped reports with count/sum/avg/min/max summaries, print-ready
- [x] **B4** `print-renderer.ts` — `@page`/`@media print` CSS from `PrintConfig`, hidden-iframe browser print trigger
- [x] **B5** `luau-markdown-plugin.ts` — inline ```luau fenced blocks executed and rendered into markdown previews

### Phase C — Puck widgets

All seven widgets follow the existing `facet-view` / `spatial-canvas` / `data-portal`
pattern: entity def in `entities.ts`, renderer component under `components/`, wired
into both `layout-panel.tsx` (Puck builder) and `canvas-panel.tsx` (canvas preview).

- [x] **C1** `kanban-widget` — HTML5 drag-drop groups cards by a field; drop reassigns the group value via kernel.updateObject (no `@dnd-kit` dep)
- [x] **C2** `calendar-widget` — CSS grid month view with event dots, prev/next/today navigation, click-to-create
- [x] **C3** `chart-widget` — pure SVG bar/line/pie/area with count/sum/avg/min/max aggregations (no `recharts` dep)
- [x] **C4** `map-widget` — SVG lat/lng scatter with auto-bounds projection (swap in `react-leaflet` later for tile layers)
- [x] **C5** `tab-container` — horizontal tab bar, JSON-array or CSV label parsing
- [x] **C6** `popover-widget` + `slide-panel` — trigger-button popover and collapsible accordion
- [x] **C7** `FacetViewRenderer` — renders nested `tab`/`popover`/`slide` slots from FacetDefinitions
- [x] **C8** `list-widget` / `table-widget` / `card-grid-widget` / `report-widget` — data-driven data view widgets bound to a `collectionType`; renderers live in `components/*-widget-renderer.tsx` with exported pure helpers (`readListField`, `parseTableColumns`/`readCellValue`/`sortObjects`, `clampColumnWidth`, `buildReportGroups`/`computeAggregate`/`formatAggregate`). Replaces the legacy `record-browser-panel.tsx` (deleted) and the object-explorer view-mode switcher — all "views" are now composable Puck widgets.

### Phase D — Independent items

- [x] **D1** Import Panel (Lens #28, Shift+Y) — CSV/TSV/JSON file drop, column→field mapping table, 10-row preview, bulk `kernel.createObject` (pure helpers exported for tests)
- [x] **D2** `GET /api/portals/:id/export` — bundles portal manifest + backing collection snapshot into a downloadable JSON template
- [x] **D3** `POST /api/email/send` + `GET /api/email/status` — pluggable `EmailTransport` interface with `createMemoryEmailTransport` for tests; `{{field}}` subject/body interpolation; 503 unconfigured, 502 on delivery failure
- [x] **D4** `StreamEdgeComponent` — animated dashed bezier (`@keyframes prism-stream-dash`) registered in `prismEdgeTypes`

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (full repo) | 3359 | Pass (159 files) |
| New colocated tests this plan | 143 | Pass |

Pre-existing TS errors in `kernel/builder-manager.ts` + `panels/app-builder-panel.tsx` (staged WIP, not part of this plan) remain; full-repo typecheck is otherwise clean.

## Self-Replicating Studio (Complete)

Prism Studio is now a meta-builder: it can produce focused Prism apps (Flux, Lattice, Cadence, Grip), Studio itself, and Relay deployments as build targets — while remaining the universal host. See `SPEC.md` § "Studio as a Self-Replicating Meta-Builder".

### Completed

- [x] SPEC.md — documented App Profiles, BuildTargets, BuildPlan execution model, App Builder Lens
- [x] Layer 1 builder primitives (`@prism/core/builder`) — `AppProfile`, `BuildTarget`, `BuildStep` (emit-file/run-command/invoke-ipc), `BuildPlan`, `BuildRun`, `serializeAppProfile`/`parseAppProfile`, `createBuildPlan`, `serializeBuildPlan`
- [x] Six built-in profiles — `studio` (universal host, no plugin filter), `flux` (work/finance/crm), `lattice` (assets/platform), `cadence` (life/platform), `grip` (work/assets/platform), `relay` (no plugins, 6 relay modules, glass flip disabled)
- [x] Six build targets — `web`, `tauri`, `capacitor-ios`, `capacitor-android`, `relay-node`, `relay-docker` — each with deterministic step list and artifact descriptors
- [x] Studio `BuilderManager` — profile registry, active-profile pin (null = universal host), planBuild/planBuilds, runPlan with executor injection, run history, subscriptions
- [x] Two executors — `createDryRunExecutor` (default; emit-file→success, run-command→skipped, no daemon required) and `createTauriExecutor({ invoke })` (dispatches via `invoke('run_build_step', ...)`, stops on first failure)
- [x] App Builder Lens (#28, Shift+B, 🏭) — profile grid, target pills, preview plan, dry-run build, run history, raw BuildPlan JSON
- [x] `useBuilder` kernel hook — version key tracks profiles/active/runs/last status
- [x] `@prism/core/builder` wired into root `vitest.config.ts` alias map and `package.json` exports

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (`@prism/core/builder`) | 20 | Pass |
| Vitest (Studio `builder-manager`) | 29 | Pass |
| Vitest (full repo) | 3218 | Pass |
| Playwright (`app-builder.spec.ts`) | 11 | Pass |

## Unified Builder System (Complete)

Puck/Luau/Canvas/Facet builder system unified and fully working.

### Completed

- [x] `luau-block` entity type — component that stores Luau source, renders via Luau UI parser
- [x] Layout Panel (Puck) — live onChange sync (debounced 300ms), Luau Block component with inline preview
- [x] Canvas Panel — renders `luau-block` objects inline with parsed Luau UI tree
- [x] Luau Facet Panel — bound to kernel objects: selecting a luau-block auto-loads its source, edits auto-save (debounced 400ms)
- [x] Component Palette — wired into sidebar (below ObjectExplorer), includes luau-block type, search, drag-to-add
- [x] Facet Designer Panel — visual FacetDefinition builder with parts, field slots, portal slots, summaries, sort/group, hooks
- [x] Record Browser — superseded: list/table/card-grid/report are now composable Puck widgets (`components/*-widget-renderer.tsx`), the standalone panel was removed
- [x] Cross-panel integration — Canvas reflects inspector edits, palette→canvas, delete→undo, graph renders all types
- [x] Seed data includes luau-block "Status Widget" demo on Home page

## Free-Form Spatial Layout (Complete)

FileMaker Pro-style absolute positioning as nestable Puck components. See `docs/dev/filemaker-gap-analysis.md`.

### Completed

- [x] Schema extensions — SpatialRect, TextSlot, DrawingSlot, ConditionalFormat, FacetLayoutMode on facet-schema.ts
- [x] Builder API — `.addText()`, `.addDrawing()`, `.layoutMode()`, `.canvasSize()` on FacetDefinitionBuilder
- [x] Pure spatial functions — `spatial-layout.ts` (computePartBands, snapToGrid, alignSlots, distributeSlots, detectOverlaps, slotHitTest, partForY, clampToBand, sortByZIndex)
- [x] 3 new Puck component types — `facet-view`, `spatial-canvas`, `data-portal` entities + custom renderers
- [x] SpatialCanvasRenderer — react-moveable + react-selecto for drag/resize/snap/multi-select
- [x] FacetViewRenderer — renders FacetDefinition in form/list/table/report/card modes
- [x] DataPortalRenderer — related records inline via edge relationships
- [x] Spatial Canvas Panel — dedicated editor lens (#23, Shift+X) with field palette, slot inspector, grid/snap
- [x] FacetDesigner updated — handles text/drawing slot variants with clone/count
- [x] Gap analysis doc — `docs/dev/filemaker-gap-analysis.md` with P0-P4 status tracking
- [x] 45 Vitest tests for spatial-layout pure functions
- [x] 14 Playwright E2E tests for spatial canvas

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (studio-kernel) | 112 | Pass |
| Vitest (all) | 3081 | Pass |
| Playwright (builder) | 49 | Pass |
| Playwright (spatial-canvas) | 14 | Pass |
| Playwright (filemaker-panels) | 60 | Pass |
| Playwright (relay deployment) | 34 | Pass |
| **Playwright (total)** | **371** | **Pass** |

## FileMaker Pro Core Schemas (Complete)

Five new core type systems inspired by FileMaker Pro's 40-year-old patterns, implemented as
Layer 1 agnostic TypeScript. See `docs/dev/filemaker-gap-analysis.md` for full gap tracker.

### Completed

- [x] **SavedView** (Found Sets) — `saved-view.ts` in `@prism/core/view`
  - Persistable named ViewConfig with mode, filters, sorts, groups
  - SavedViewRegistry with add/update/remove/pin/search/serialize/load
  - 28 Vitest tests
- [x] **ValueList** — `value-list.ts` in `@prism/core/facet`
  - Static (hardcoded items) + dynamic (relationship-sourced) value lists
  - ValueListResolver interface for CollectionStore integration
  - ValueListRegistry with register/resolve/search/serialize/load
  - 25 Vitest tests
- [x] **ContainerSlot** — added to `facet-schema.ts`
  - New slot kind for VFS BinaryRef fields (images, PDFs, audio, video)
  - MIME type filtering, max size, render mode (preview/icon), thumbnail dims
  - `addContainer()` builder method
  - 4 Vitest tests
- [x] **PrintConfig** — added to `facet-schema.ts`
  - Page size (letter/legal/a4/a3/custom), orientation, margins
  - Page numbers, headers/footers, page breaks per group
  - `printConfig()` builder method, `createPrintConfig()` factory
  - 3 Vitest tests
- [x] **PrivilegeSet** — `privilege-set.ts` in `@prism/core/manifest`
  - Collection-level (full/read/create/none), field-level (readwrite/readonly/hidden)
  - Layout visibility, script execution permissions
  - Row-level security via ExpressionEngine filter
  - RoleAssignment maps DID → PrivilegeSet
  - Helper functions: getCollectionPermission, getFieldPermission, canWrite, canRead
  - Added `privilegeSets` and `roleAssignments` to PrismManifest
  - 21 Vitest tests
- [x] **FacetDefinition extensions** — valueListBindings, requiredPrivilegeSet on FacetDefinition
  - `bindValueList()` and `requiredPrivilegeSet()` builder methods
  - 4 Vitest tests

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| facet-schema | 103 | Pass |
| value-list | 25 | Pass |
| saved-view | 28 | Pass |
| privilege-set | 21 | Pass |
| **Vitest (all)** | **2915** | **Pass** |

### Runtime + UI (Complete)

- [x] **PrivilegeEnforcer** — `privilege-enforcer.ts`: filterObjects with row-level security, redactObject for hidden fields, visibleFields/canEditField/canSeeLayout
- [x] **Conditional formatting runtime** — `facet-runtime.ts`: evaluateConditionalFormats, computeFieldStyle with expression parsing
- [x] **Merge field interpolation** — `facet-runtime.ts`: interpolateMergeFields `{{fieldName}}`, renderTextSlot, dot-notation path resolution
- [x] **CollectionValueListResolver** — `facet-runtime.ts`: resolves dynamic value lists from CollectionStore data
- [x] **Visual Scripting** — `script-steps.ts`: 31 step types across 7 categories, emitStepsLuau Luau codegen with proper indentation, validateSteps block matching, getStepCategories palette builder
- [x] **FacetStore** — `facet-store.ts`: persistent registry for FacetDefinitions + VisualScripts + ValueLists with serialize/load
- [x] **Studio panels** — 4 new lenses registered (Shift+S/V/L/P):
  - Visual Script Editor (step palette, parameter inputs, live Luau preview, block validation)
  - Saved Views (create/delete/pin/search, filter summary, active view highlighting)
  - Value Lists (static inline editor, dynamic source config)
  - Privilege Sets (permission matrix, row-level security, role assignments)
- [x] **ContainerFieldRenderer** — MIME-aware inline preview (image/audio/video/PDF/file icon) with drag-drop upload

### Test Summary (Updated)

| Suite | Tests | Status |
|-------|-------|--------|
| facet-schema | 103 | Pass |
| value-list | 25 | Pass |
| saved-view | 28 | Pass |
| privilege-set | 21 | Pass |
| facet-runtime | 24 | Pass |
| script-steps | 37 | Pass |
| facet-store | 14 | Pass |
| privilege-enforcer | 16 | Pass |
| studio-kernel (new) | 15 | Pass |
| **Vitest (all)** | **3081** | **Pass** |
| E2E filemaker-panels | 60 | Written |

### Kernel Integration

- [x] **Kernel wiring** — facetStore, savedViews, valueLists, privilegeSets on StudioKernel
- [x] **Reactive hooks** — useFacetStore, useSavedViews, useValueLists, usePrivilegeSets in kernel-context
- [x] **E2E tests** — 60 Playwright tests across Visual Script, Saved Views, Value Lists, Privilege Sets, Container Field, Kernel Integration
- [x] **Kernel unit tests** — 15 new tests for facetStore, savedViews, valueLists, privilegeSets CRUD + listener notification

### Remaining FileMaker Gaps (Phase 3+)

- [ ] Tab controls / slide panels / popovers (P1)
- [ ] Layout picker dropdown UI (P1)
- [ ] Per-field calculation binding (P2)
- [ ] Themes / custom styles (P3)
- [ ] PDF export via PrintConfig (P3 — schema done, renderer TODO)
- [ ] Starter Manifest gallery UI (P5)
- [ ] Schema Designer write mode in Graph Panel (P5)
  - [ ] `graph-panel.tsx` mode toggle (`view` / `design`)
  - [ ] Persist node x/y via new `schemaLayout` LoroMap on kernel
  - [ ] Double-click-blank → new EntityDef (reuse entity-builder logic)
  - [ ] Double-click-node → field CRUD popover (add/remove/rename/type)
  - [ ] Port-drag between nodes → registerEdge dialog (reuse relationship-builder logic)
  - [ ] Playwright E2E: draw edge + add field round-trip
- [ ] Luau step-through debugger / DAP (P5)
  - [ ] `layer1/luau/luau-debugger.ts` — source-instrumentation stepper (`__prism_trace`)
  - [ ] Breakpoint gutter in `visual-script-panel.tsx` Luau preview
  - [ ] Paused-frame UI: locals table + step/continue/stop controls
  - [ ] Breakpoint gutter in `editor-panel.tsx` for `luau-block` objects
  - [ ] Vitest unit suite + Playwright E2E (breakpoint → pause → inspect)

**Already landed (no action):** Value Lists, Container Fields, Found Sets /
SavedView are all shipped — see the P5 table above. Any future FileMaker
feedback that references these should update the existing panels rather than
introducing new ones.

## Phase 1: The Heartbeat (Complete)

Loro CRDT round-trips between browser and Rust daemon via Tauri IPC. All tests passing.

## Phase 2: The Eyes (Complete)

Visual editing of CRDT state.

### Completed

- [x] CodeMirror 6 editing `LoroText` with real-time bidirectional sync
  - `loroSync()` extension: CM edits -> Loro, Loro changes -> CM update
  - `createLoroTextDoc()` helper for creating editor documents
  - `useCodemirror()` React hook with lifecycle management
  - `prismEditorSetup()` shared base configuration
- [x] Puck: drag -> Loro, Loro -> Puck `data` prop
  - `createPuckLoroBridge()` stores layout data in Loro root map
  - `usePuckLoro()` React hook for reactive Puck data
  - CRDT merge support (peer sync through Loro export/import)
- [x] KBar command palette with focus-depth routing
  - `createActionRegistry()` with Global -> App -> Plugin -> Cursor depths
  - `PrismKBarProvider` wrapping kbar with depth-aware filtering
  - `usePrismKBar()` hook for action registration
- [x] `prism-studio`: multi-panel IDE layout
  - Editor tab: CodeMirror + CRDT Inspector (resizable panels)
  - Layout tab: Puck visual builder with Heading/Text/Card components
  - CRDT tab: full-width state inspector
  - KBar palette (CMD+K) with navigation actions
- [x] `notify` file watcher in prism-daemon
  - `watch_directory()` with create/modify/remove event detection
  - Non-blocking `poll_events()` and blocking `wait_event()`
  - Rust tests: file create, modify, remove detection

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (loro-bridge) | 9 | Pass |
| Vitest (use-crdt-store) | 7 | Pass |
| Vitest (luau-runtime) | 8 | Pass |
| Vitest (loro-sync CM) | 6 | Pass |
| Vitest (puck-loro-bridge) | 7 | Pass |
| Vitest (focus-depth) | 9 | Pass |
| Rust (crdt commands) | 3 | Pass |
| Rust (luau commands) | 6 | Pass |
| Rust (file watcher) | 3 | Pass |
| **Phase 1+2 Total** | **58** | **All Pass** |

## Phase 0: The Object Model (Complete)

Ported from legacy Helm codebase with Helm→Prism rename. Foundation for the Object-Graph.

### Completed

- [x] `GraphObject` — universal graph node with shell + payload pattern
- [x] `ObjectEdge` — typed edges between objects with behavior semantics
- [x] `EntityDef` — schema-typed entity blueprints with category, tabs, fields
- [x] `ObjectRegistry` — runtime registry of entity/edge types, category rules, slot system
- [x] `TreeModel` — stateful in-memory tree with add/remove/move/reorder/duplicate/update
- [x] `EdgeModel` — stateful in-memory edge store with hooks and events
- [x] `WeakRefEngine` — automatic content-derived cross-object edges via providers
- [x] `NSID` — namespaced identifiers for cross-Node type interoperability
- [x] `PrismAddress` — `prism://did:web:node/objects/id` addressing scheme
- [x] `NSIDRegistry` — NSID↔local type bidirectional mapping
- [x] `ObjectQuery` — typed query descriptor with filtering, sorting, serialization
- [x] Branded ID types (`ObjectId`, `EdgeId`) with zero-cost type safety
- [x] Slot system for Lens extensions (tabs + fields contributed without modifying base EntityDef)

### Axed from Legacy (at Phase 0 — some later restored)

- `interfaces.ts` — premature abstraction; concrete classes serve as the interface
- `api-config.ts` — replaced by `layer1/server/route-gen.ts` in Phase 10
- `command-palette.ts` — KBar already handles this
- `tree-clipboard.ts` + `cascade.ts` — restored as `layer1/clipboard/` in Phase 17
- `context-engine.ts` — restored as `layer1/object-model/context-engine.ts` in Phase 6
- `lua-bridge.ts` — Prism has its own Luau integration
- `presets/` — domain-specific; Lenses define their own

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (registry) | 19 | Pass |
| Vitest (tree-model) | 22 | Pass |
| Vitest (edge-model) | 13 | Pass |
| Vitest (nsid) | 15 | Pass |
| Vitest (query) | 15 | Pass |
| **Phase 0 Total** | **84** | **All Pass** |

## Phase 4: The Shell (Complete)

Registry-driven workspace shell replacing hardcoded Studio UI.

### Completed

- [x] `LensManifest` — typed lens definition (id, name, icon, category, contributes)
- [x] `LensRegistry` — register/unregister/query/subscribe with events
- [x] `WorkspaceStore` — Zustand store for tabs, activeTab, panelLayout
  - [x] Singleton tab behavior (dedup by lensId, pinned tabs opt out)
  - [x] Tab CRUD: openTab, closeTab, pinTab, reorderTab, setActiveTab
  - [x] Panel layout: toggleSidebar, toggleInspector, width management
- [x] `LensProvider` + `useLensContext()` — React context for registries
- [x] `ActivityBar` — vertical icon bar from LensRegistry
- [x] `TabBar` — horizontal tab bar with close/pin controls
- [x] `WorkspaceShell` — top-level layout composing all shell components
- [x] 4 built-in lenses: Editor, Graph, Layout, CRDT (manifests + component map)
- [x] KBar actions derived from LensRegistry (not hardcoded)
- [x] `data-testid` attributes throughout for Playwright

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (lens-registry) | 12 | Pass |
| Vitest (workspace-store) | 15 | Pass |
| Playwright (shell) | 8 | Pass |
| Playwright (heartbeat, updated) | 6 | Pass |
| Playwright (graph, updated) | 5 | Pass |
| **Phase 4 Total** | **27 Vitest + 19 E2E** | **All Pass** |

## Phase 5: Input, Forms, Layout, Expression (Complete)

Four foundational Layer 1 systems ported from legacy Helm. All pure TypeScript, zero React.

### Completed

- [x] **Input System** (`layer1/input/`) — KeyboardModel, InputScope, InputRouter (LIFO scope stack)
  - Shortcut format: `cmd+k`, `cmd+shift+z`, `escape`. `cmd` = Ctrl OR Meta (cross-platform)
  - InputScope: named context with KeyboardModel + action handlers, fluent `.on()`, UndoHook
  - InputRouter: push/pop/replace scopes, handleKeyEvent walks stack top-down, async dispatch
- [x] **Forms & Validation** (`layer1/forms/`) — schema-driven forms + text parsing
  - FieldSchema: 17 field types (text, number, currency, rating, slider, boolean, date, select, etc.)
  - DocumentSchema: fields + sections (TextSection | FieldGroupSection)
  - FormSchema: extends DocumentSchema with validation rules + conditional visibility
  - FormState: immutable pure-function state management (create, set, validate, reset)
  - Wiki-link parser: `parseWikiLinks()`, `extractLinkedIds()`, `renderWikiLinks()`, `detectInlineLink()`
  - Markdown parser: `parseMarkdown()` → BlockToken[], `parseInline()` → InlineToken[]
- [x] **Layout System** (`layer1/layout/`) — multi-pane navigation with history
  - SelectionModel: select/toggle/selectRange/selectAll/clear with events
  - PageModel<TTarget>: viewMode, activeTab, selection, inputScopeId, persist/fromSerialized
  - PageRegistry<TTarget>: maps target.kind → defaults, createPage factory
  - WorkspaceSlot: inline back/forward history (no external NavigationController), LRU page cache
  - WorkspaceManager: multiple slots, active tracking, open/close/focus
- [x] **Expression Engine** (`layer1/expression/`) — formula evaluation
  - Scanner/Tokenizer: operand syntax `[type:id.subfield]`, numbers, strings, booleans, operators
  - Recursive descent parser: standard operator precedence, bare identifiers as field operands
  - Evaluator: arithmetic, comparison, boolean logic (short-circuit), string concat, builtins (abs, ceil, floor, round, sqrt, pow, min, max, clamp)
  - `evaluateExpression()` convenience: auto-wraps bare identifiers as `[field:name]`

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (keyboard-model) | 25 | Pass |
| Vitest (input-router + scope) | 23 | Pass |
| Vitest (form-state) | 20 | Pass |
| Vitest (wiki-link) | 17 | Pass |
| Vitest (markdown) | 27 | Pass |
| Vitest (selection-model) | 12 | Pass |
| Vitest (page-model) | 12 | Pass |
| Vitest (page-registry) | 8 | Pass |
| Vitest (workspace-slot) | 14 | Pass |
| Vitest (workspace-manager) | 14 | Pass |
| Vitest (scanner) | 21 | Pass |
| Vitest (expression) | 68 | Pass |
| **Phase 5 Total** | **261** | **All Pass** |

## Phase 6: Context Engine, Plugin System, Reactive Atoms (Complete)

Three systems that complete the registry-driven architecture. All pure TypeScript, zero React.

### Completed

- [x] **Context Engine** (`layer1/object-model/context-engine.ts`) — context-aware suggestion engine
  - `getEdgeOptions(sourceType, targetType?)` — valid edge types between objects
  - `getChildOptions(parentType)` — object types that can be children
  - `getAutocompleteSuggestions(sourceType)` — inline [[...]] link types + defaultRelation
  - `getContextMenu(objectType, targetType?)` — structured right-click menu (create/connect/object sections)
  - `getInlineLinkTypes(sourceType)` / `getInlineEdgeTypes()` — suggestInline edge types
  - All answers derived from ObjectRegistry — nothing hardcoded
- [x] **Plugin System** (`layer1/plugin/`) — universal extension unit
  - `ContributionRegistry<T>` — generic typed registry (register/unregister/query/byPlugin)
  - `PrismPlugin` — universal plugin interface with `contributes` (views, commands, keybindings, contextMenus, activityBar, settings, toolbar, statusBar, weakRefProviders)
  - `PluginRegistry` — manages plugins, auto-registers contributions into typed registries, events on register/unregister
  - Contribution types: ViewContributionDef, CommandContributionDef, KeybindingContributionDef, ContextMenuContributionDef, ActivityBarContributionDef, SettingsContributionDef, ToolbarContributionDef, StatusBarContributionDef
- [x] **Reactive Atoms** (`layer1/atom/`) — Zustand-based reactive state layer
  - `PrismBus` — lightweight typed event bus (on/once/emit/off, createPrismBus factory)
  - `PrismEvents` — well-known event type constants (objects/edges/navigation/selection/search)
  - `AtomStore` — UI state atoms (selectedId, selectionIds, editingObjectId, activePanel, searchQuery, navigationTarget)
  - `ObjectAtomStore` — in-memory object/edge cache with selectors (selectObject, selectQuery, selectChildren, selectEdgesFrom, selectEdgesTo)
  - `connectBusToAtoms(bus, atomStore)` — wire navigation/selection/search events to UI atoms
  - `connectBusToObjectAtoms(bus, objectStore)` — wire object/edge CRUD events to cache

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (context-engine) | 17 | Pass |
| Vitest (contribution-registry) | 15 | Pass |
| Vitest (plugin-registry) | 17 | Pass |
| Vitest (event-bus) | 13 | Pass |
| Vitest (atoms) | 15 | Pass |
| Vitest (object-atoms) | 19 | Pass |
| Vitest (connect) | 15 | Pass |
| **Phase 6 Total** | **111** | **All Pass** |

## Phase 7: State Machines, Graph Analysis, Planning Engine (Complete)

Three systems for workflow orchestration and dependency analysis. All pure TypeScript, zero React.

### Completed

- [x] **State Machines** (`layer1/automaton/`) — flat FSM ported from legacy `@core/automaton`
  - `Machine<TState, TEvent>` — context-free FSM with guards, actions, lifecycle hooks (onEnter/onExit)
  - `createMachine(def)` factory with `.start()` and `.restore()` (no onEnter on restore)
  - Wildcard `from: '*'` matches any state, array `from` for multi-source transitions
  - Terminal states block all outgoing transitions
  - Observable via `.on()` listener, serializable via `.toJSON()`
- [x] **Dependency Graph** (`layer1/graph-analysis/dependency-graph.ts`) — ported from legacy `@core/tasks`
  - `buildDependencyGraph(objects)` — forward "blocks" graph from `data.dependsOn`/`data.blockedBy`
  - `buildPredecessorGraph(objects)` — inverse "blocked-by" graph
  - `topologicalSort(objects)` — Kahn's algorithm, cyclic nodes appended at end
  - `detectCycles(objects)` — DFS cycle detection, returns cycle paths
  - `findBlockingChain(objectId, objects)` — transitive upstream blockers (BFS)
  - `findImpactedObjects(objectId, objects)` — transitive downstream dependants (BFS)
  - `computeSlipImpact(objectId, slipDays, objects)` — BFS wave propagation of slip
- [x] **Planning Engine** (`layer1/graph-analysis/planning-engine.ts`) — ported from legacy `@core/planning`
  - `computePlan(objects)` — generic CPM on any GraphObject with `data.dependsOn`
  - Forward pass: earlyStart, earlyFinish
  - Backward pass: lateStart, lateFinish, totalFloat
  - Critical path extraction (zero-float nodes)
  - Duration priority: `data.durationDays` > `data.estimateMs` > date span > default 1 day

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (machine) | 19 | Pass |
| Vitest (dependency-graph) | 20 | Pass |
| Vitest (planning-engine) | 10 | Pass |
| **Phase 7 Total** | **49** | **All Pass** |

### E2E Tests (All Phases)

All 19 Playwright tests pass covering Phases 1-4 rendered UI:

| Suite | Tests | Status |
|-------|-------|--------|
| Playwright (heartbeat) | 6 | Pass |
| Playwright (graph) | 5 | Pass |
| Playwright (shell) | 8 | Pass |
| **E2E Total** | **19** | **All Pass** |

Note: Phases 0, 5, 6, 7, 8 are pure Layer 1 TypeScript with no React UI. E2E tests apply to rendered UI features (Phases 1-4). All Layer 1 systems are tested via Vitest unit tests.

## Phase 8: Automation Engine, Prism Manifest (Complete)

Two systems for workflow automation and manifest-driven workspace definition. All pure TypeScript, zero React.

### Terminology (from SPEC.md)

| Term | Definition |
|------|-----------|
| **Vault** | Encrypted local directory — the physical security boundary. Contains Collections and Manifests. |
| **Collection** | Typed CRDT array (e.g. `Contacts`, `Tasks`). Holds the actual data. |
| **Manifest** | JSON file with weak references to Collections. A "workspace" is just a Manifest pointing to data nodes. Multiple manifests can reference the same collection with different filters. |
| **Shell** | The IDE chrome that renders whatever a Manifest references. No fixed layout. |

### Completed

- [x] **Automation Engine** (`layer1/automation/`) — trigger/condition/action rule engine
  - `AutomationTrigger` — ObjectTrigger (created/updated/deleted with type/tag/field filters), CronTrigger, ManualTrigger
  - `AutomationCondition` — FieldCondition (10 comparison operators), TypeCondition, TagCondition, And/Or/Not combinators
  - `AutomationAction` — CreateObject, UpdateObject, DeleteObject, Notification, Delay, RunAutomation
  - `evaluateCondition()` — recursive condition tree evaluator with dot-path field access
  - `interpolate()` — `{{path}}` template replacement from AutomationContext
  - `matchesObjectTrigger()` — object event filtering by type/tag/field match
  - `AutomationEngine` — orchestrator with start/stop lifecycle, cron scheduling, handleObjectEvent(), run()
  - `AutomationStore` interface — synchronous list/get/save/saveRun for pluggable persistence
  - Action dispatch via `ActionHandlerMap` — app layer provides handlers, engine orchestrates
  - Execution tracking: AutomationRun with per-action results, status (success/failed/skipped/partial)
- [x] **Prism Manifest** (`layer1/manifest/`) — workspace definition file
  - `PrismManifest` — on-disk `.prism.json` containing weak references to Collections in a Vault
  - `CollectionRef` — a manifest's pointer to a typed CRDT collection, optionally with type/tag/sort filters
  - `StorageConfig` — Loro CRDT (default), memory, fs backends (adapted from legacy sqlite/http/postgres)
  - `SchemaConfig` — ordered schema module references (`@prism/core`, relative paths)
  - `SyncConfig` — off/manual/auto modes with peer addresses for CRDT sync
  - `defaultManifest()`, `parseManifest()`, `serialiseManifest()`, `validateManifest()`
  - Collection ref CRUD: `addCollection()`, `removeCollection()`, `updateCollection()`, `getCollection()`
  - Full glossary (Vault/Collection/Manifest/Shell) in `manifest-types.ts` doc comment

### Axed from Legacy

- `WebhookTrigger` / `IntegrationEventTrigger` — Prism uses Tauri IPC, not HTTP endpoints
- `WebhookAction` / `IntegrationAction` — no raw HTTP in Prism architecture
- `FeatureFlagCondition` / `ConfigCondition` — deferred until config system exists
- `IAutomationStore` (async) — simplified to synchronous `AutomationStore` (Loro CRDT is sync)
- `SqliteStorageConfig` / `HttpStorageConfig` / `PostgresStorageConfig` / `IndexedDBStorageConfig` — Prism uses Loro CRDT, not SQL
- `SyncProviderKind` (http/git/dropbox/onedrive/s3) — simplified to peer-based CRDT sync
- `WorkspaceRoster` — deferred; vault discovery is a daemon concern

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (condition-evaluator) | 27 | Pass |
| Vitest (automation-engine) | 17 | Pass |
| Vitest (manifest) | 27 | Pass |
| **Phase 8 Total** | **71** | **All Pass** |

## Phase 9: Config System, Undo/Redo System (Complete)

Two infrastructure systems for settings management and interaction history. All pure TypeScript, zero React.

### Completed

- [x] **Config System** (`layer1/config/`) — layered settings with scope resolution
  - `ConfigRegistry` — instance-based catalog of SettingDefinitions + FeatureFlagDefinitions
  - `ConfigModel` — live runtime config with layered scope resolution (default → workspace → user)
  - `SettingDefinition` — typed settings with validation, scope restrictions, secret masking, tags
  - `ConfigStore` interface — synchronous persistence (Loro CRDT is sync)
  - `MemoryConfigStore` — in-process store with `simulateExternalChange()` for testing
  - `attachStore(scope, store)` — auto-load + subscribe to external changes
  - `watch(key, cb)` — observe specific key changes (immediate call + on change)
  - `on('change', cb)` — wildcard listener for all config mutations
  - `toJSON(scope)` — serialization with secret masking ('***')
  - `validateConfig()` — lightweight JSON Schema subset (string/number/boolean/array/object)
  - `coerceConfigValue()` — env var string → typed value coercion
  - `schemaToValidator()` — bridge from declarative schema to SettingDefinition.validate
  - `FeatureFlags` — boolean toggles with config key delegation and condition evaluation
  - Built-in settings: ui (theme, density, language, sidebar, activityBar), editor (fontSize, lineNumbers, spellCheck, indentSize, autosaveMs), sync (enabled, intervalSeconds), ai (enabled, provider, modelId, apiKey), notifications
  - Built-in flags: ai-features (→ ai.enabled), sync (→ sync.enabled)
- [x] **Undo/Redo System** (`layer1/undo/`) — snapshot-based undo stack
  - `UndoRedoManager` — framework-agnostic undo/redo with configurable max history
  - `ObjectSnapshot` — before/after diffs for GraphObject and ObjectEdge
  - `push(description, snapshots)` — record undoable entry, clears redo stack
  - `merge(snapshots)` — coalesce rapid edits into last entry
  - `undo()` / `redo()` — calls applier with snapshot direction
  - `canUndo` / `canRedo` / `undoLabel` / `redoLabel` — UI state queries
  - `subscribe(cb)` — observe stack changes for reactive UI updates
  - Synchronous applier (not async — Loro CRDT operations are sync)

### Axed from Legacy

- `SettingScope: 'app' | 'team'` — Prism is local-first; no server-level or team-level scopes
- `IConfigStore` (async) — simplified to synchronous `ConfigStore` (Loro CRDT is sync)
- `LocalStorageConfigStore` — browser-specific; Prism uses Tauri IPC
- `FeatureFlagCondition: 'user-role' | 'team-plan' | 'env'` — Prism has no team plans or server env vars
- Server/SaaS settings (session timeout, CORS, 2FA, allowed origins) — Relay concerns, not core
- `loadFromModule()` — deferred until schema loader exists

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (config-registry) | 13 | Pass |
| Vitest (config-model) | 28 | Pass |
| Vitest (config-schema) | 29 | Pass |
| Vitest (feature-flags) | 11 | Pass |
| Vitest (undo-manager) | 22 | Pass |
| **Phase 9 Total** | **103** | **All Pass** |

## Phase 10: Server Factory, Undo Bridge (Complete)

Two systems for REST API generation and undo/redo integration. All pure TypeScript, zero React.

### Completed

- [x] **Server Factory** (`layer1/server/`) — framework-agnostic REST route generation
  - `ApiOperation` + `ObjectTypeApiConfig` — types added to EntityDef for declarative API config
  - `generateRouteSpecs(registry)` — reads ObjectRegistry.allDefs() and generates RouteSpec[] for types with `api` config
  - Per-type CRUD routes: list (GET), get (GET /:id), create (POST), update (PUT /:id), delete (DELETE /:id), restore (POST /:id/restore), move (POST /:id/move), duplicate (POST /:id/duplicate)
  - Edge routes: GET/POST/PUT/DELETE /api/edges[/:id], GET /api/objects/:id/related
  - Global object search: GET /api/objects, GET /api/objects/:id
  - `RouteAdapter` interface + `registerRoutes()` — framework-agnostic handler registration
  - `groupByType()`, `printRouteTable()` — utilities for introspection
  - `buildOpenApiDocument()` — OpenAPI 3.1.0 document from RouteSpec[] + ObjectRegistry
  - Per-type component schemas from EntityFieldDef (enum, date, datetime, url, object_ref, bool, int, float, text)
  - GraphObject/ObjectEdge/ResolvedEdge base schemas in components
  - Proper operationIds (listTasks, getTask, createTask, etc.) and tags
  - `generateOpenApiJson()` — serialized OpenAPI document
  - String helpers: `pascal()`, `camel()`, `singular()` (in object-model/str.ts)
- [x] **Undo Bridge** (`layer1/undo/undo-bridge.ts`) — auto-recording TreeModel/EdgeModel mutations
  - `createUndoBridge(manager)` — returns TreeModelHooks + EdgeModelHooks
  - afterAdd: records create snapshot (before=null, after=object)
  - afterRemove: records delete snapshots for object + all descendants
  - afterMove: records move snapshot
  - afterDuplicate: records create snapshots for all copies
  - afterUpdate: records before/after snapshot
  - Edge hooks: afterAdd, afterRemove, afterUpdate — same pattern for ObjectEdge
  - All snapshots are deep copies via structuredClone (mutation-safe)

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (route-gen) | 19 | Pass |
| Vitest (openapi) | 16 | Pass |
| Vitest (undo-bridge) | 11 | Pass |
| **Phase 10 Total** | **46** | **All Pass** |

## Phase 11: CRDT Persistence Layer (Complete)

Two systems for durable CRDT-backed storage and vault lifecycle management. All pure TypeScript, zero React.

### Completed

- [x] **Collection Store** (`layer1/persistence/collection-store.ts`) — Loro CRDT-backed object/edge storage
  - `createCollectionStore(options?)` — wraps a LoroDoc with "objects" + "edges" top-level maps
  - Object CRUD: `putObject()`, `getObject()`, `removeObject()`, `listObjects()`, `objectCount()`
  - Edge CRUD: `putEdge()`, `getEdge()`, `removeEdge()`, `listEdges()`, `edgeCount()`
  - `ObjectFilter` — query by types, tags, statuses, parentId, excludeDeleted
  - Edge filtering by sourceId, targetId, relation
  - Snapshot: `exportSnapshot()`, `exportUpdate(since?)`, `import(data)` — full Loro CRDT sync
  - `onChange(handler)` — subscribe to object/edge mutations via `CollectionChange` events
  - `allObjects()`, `allEdges()`, `toJSON()` — bulk access and debugging
  - Multi-peer sync via peerId option and Loro merge semantics
- [x] **Vault Persistence** (`layer1/persistence/vault-persistence.ts`) — manifest-driven collection lifecycle
  - `PersistenceAdapter` interface — pluggable I/O: `load()`, `save()`, `delete()`, `exists()`, `list()`
  - `createMemoryAdapter()` — in-memory adapter for testing and ephemeral workspaces
  - `createVaultManager(manifest, adapter, options?)` — orchestrates collection stores against persistence
  - Lazy loading: `openCollection(id)` creates + hydrates from disk on first access
  - Dirty tracking: mutations auto-mark collections dirty via `onChange` subscription
  - `saveCollection(id)` / `saveAll()` — persist dirty collections as Loro snapshots
  - `closeCollection(id)` — save + evict from cache
  - `isDirty(id)`, `openCollections()` — introspection
  - Collection paths: `data/collections/{collectionId}.loro`

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (collection-store) | 31 | Pass |
| Vitest (vault-persistence) | 25 | Pass |
| **Phase 11 Total** | **56** | **All Pass** |

## Phase 12: Search Engine (Complete)

Full-text search with TF-IDF scoring and cross-collection structured queries. All pure TypeScript, zero React.

### Completed

- [x] **Search Index** (`layer1/search/search-index.ts`) — in-memory inverted index with TF-IDF scoring
  - `tokenize()` — lowercase tokenizer splitting on whitespace/punctuation, configurable min length
  - `createSearchIndex(options?)` — inverted index mapping tokens to document references
  - Field-weighted scoring: name (3x), type (2x), tags (2x), status (1x), description (1x), data (0.5x)
  - IDF with smoothing: `log(1 + N/df)` — avoids zero scores with single documents
  - Multi-field extraction: indexes name, description, type, tags, status, and string data payload values
  - Add/remove/update/clear per document, removeCollection for bulk eviction
  - Case-insensitive matching, multi-token query support
- [x] **Search Engine** (`layer1/search/search-engine.ts`) — cross-collection search orchestrator
  - `createSearchEngine(options?)` — composes SearchIndex with structured filters
  - Full-text query with TF-IDF relevance scoring across all indexed collections
  - Structured filters: types, tags (AND), statuses, collectionIds, dateAfter/dateBefore, includeDeleted
  - Faceted results: counts by type, collection, and tag (computed from full result set, not just page)
  - Pagination: configurable limit/offset with default page size (50)
  - Sort by relevance (default when query present), name (default otherwise), date, createdAt, updatedAt
  - Auto-indexing: `indexCollection()` subscribes to CollectionStore.onChange for live index updates
  - Live subscriptions: `subscribe(options, handler)` re-runs search on every index change
  - Reindex/remove collection lifecycle management

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (search-index) | 30 | Pass |
| Vitest (search-engine) | 34 | Pass |
| **Phase 12 Total** | **64** | **All Pass** |

## Phase 13: Vault Discovery (Complete)

Vault/manifest registry and filesystem discovery for workspace management. All pure TypeScript, zero React.

### Completed

- [x] **Vault Roster** (`layer1/discovery/vault-roster.ts`) — persistent registry of known vaults
  - `createVaultRoster(store?)` — in-memory registry with optional backing store
  - CRUD: `add()`, `remove()`, `get()`, `getByPath()`, `update()`
  - `touch(id)` — bump `lastOpenedAt` to now on workspace open
  - `pin(id, pinned)` — pin/unpin entries for quick access
  - `list(options?)` — sort by lastOpenedAt/name/addedAt, filter by pinned/tags/search text, limit
  - Pinned entries always float to top within any sort order
  - Path-based deduplication (same vault path → single entry)
  - Change events: `onChange(handler)` with add/remove/update types
  - `RosterStore` interface for pluggable persistence + `createMemoryRosterStore()`
  - `save()` / `reload()` for explicit persistence lifecycle
  - Hydrates from store on creation
- [x] **Vault Discovery** (`layer1/discovery/vault-discovery.ts`) — filesystem scanning for manifests
  - `createVaultDiscovery(adapter, roster?)` — scan + merge orchestrator
  - `DiscoveryAdapter` interface: `listDirectories()`, `readFile()`, `exists()`, `joinPath()`
  - `createMemoryDiscoveryAdapter()` — in-memory adapter for testing
  - `scan(options)` — scan search paths for `.prism.json` files, parse manifests
  - Configurable `maxDepth` (default 1), `mergeToRoster` toggle
  - Also checks search path itself for a manifest (not just children)
  - Automatic roster merge: adds new vaults, updates existing entries on rescan
  - Discovery events: `scan-start`, `scan-complete`, `vault-found`, `scan-error`
  - Scan state tracking: `scanning`, `lastScanAt`, `lastScanCount`

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (vault-roster) | 32 | Pass |
| Vitest (vault-discovery) | 22 | Pass |
| **Phase 13 Total** | **54** | **All Pass** |

## Phase 14: Derived Views (Complete)

View mode definitions, configurable filter/sort/group pipeline, and live materialized projections from CollectionStores. All pure TypeScript, zero React.

### Completed

- [x] **View Definitions** (`layer1/view/view-def.ts`) — view mode registry with capability descriptors
  - `ViewMode` — 7 standard modes: list, kanban, grid, table, timeline, calendar, graph
  - `ViewDef` — per-mode capabilities: supportsSort, supportsFilter, supportsGrouping, supportsColumns, supportsInlineEdit, supportsBulkSelect, supportsHierarchy, requiresDate, requiresStatus
  - `createViewRegistry()` — pre-loaded with 7 built-in defs, extensible via `register()`
  - `supports(mode, capability)` — single capability query
  - `modesWithCapability(capability)` — find all modes with a feature
- [x] **View Config** (`layer1/view/view-config.ts`) — filter/sort/group pure transform pipeline
  - `FilterConfig` — 12 operators: eq, neq, contains, starts, gt, gte, lt, lte, in, nin, empty, notempty
  - `SortConfig` — field + direction, multi-level sort support
  - `GroupConfig` — field-based grouping with collapse state
  - `getFieldValue()` — resolves shell fields first, then data payload
  - `applyFilters()` — AND-combined filter evaluation
  - `applySorts()` — multi-level sort (immutable, returns new array)
  - `applyGroups()` — single-level grouping with insertion-order preservation, __none__ for null/undefined
  - `applyViewConfig()` — full pipeline: excludeDeleted → filters → sorts → limit
- [x] **Live View** (`layer1/view/live-view.ts`) — auto-updating materialized projection
  - `createLiveView(store, options?)` — wraps CollectionStore + ViewConfig
  - `snapshot` — materialized objects, grouped results, total count, type/tag facets
  - Config mutations: `setFilters()`, `setSorts()`, `setGroups()`, `setColumns()`, `setLimit()`, `setMode()`, `setConfig()`
  - `toggleGroupCollapsed(key)` — per-group collapse state management
  - `includes(objectId)` — fast membership check via internal ID set
  - Auto-updates on CollectionStore changes (add/update/remove)
  - `subscribe(listener)` — immediate callback + reactive updates
  - `refresh()` — force re-materialization
  - `dispose()` — detach from store, stop auto-updates

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (view-def) | 17 | Pass |
| Vitest (view-config) | 38 | Pass |
| Vitest (live-view) | 34 | Pass |
| **Phase 14 Total** | **89** | **All Pass** |

## Phase 15: Notification System (Complete)

In-app notification registry with debounced batching and deduplication. All pure TypeScript, zero React.

### Completed

- [x] **Notification Store** (`layer1/notification/notification-store.ts`) — in-memory notification registry
  - `NotificationKind` — 8 kinds: system, mention, activity, reminder, info, success, warning, error
  - `Notification` — id, kind, title, body?, objectId?, objectType?, actorId?, read, pinned, createdAt, readAt?, dismissedAt?, expiresAt?, data?
  - `createNotificationStore(options?)` — full CRUD with eviction policy
  - `add()` — create notification with auto-generated ID and timestamp
  - `markRead()` / `markAllRead(filter?)` — read state management
  - `dismiss()` / `dismissAll(filter?)` — soft-delete with timestamp
  - `pin()` / `unpin()` — pin important notifications
  - `getAll(filter?)` — newest-first, excludes dismissed/expired, filters by kind/read/objectId/since
  - `getUnreadCount(filter?)` — unread count excluding dismissed
  - `subscribe(handler)` — change events (add/update/dismiss)
  - `hydrate(items)` — bulk load from persistence
  - `clear()` — remove dismissed unpinned items, preserve pinned
  - Eviction policy: dismissed unpinned (oldest) → read unpinned (oldest); pinned never evicted
- [x] **Notification Queue** (`layer1/notification/notification-queue.ts`) — debounced batching with dedup
  - `createNotificationQueue(store, options?)` — enqueue → debounce → flush to store
  - `enqueue(input)` — add to pending queue with dedup by (objectId, kind)
  - Debounce: configurable window (default 300ms), timer resets on subsequent enqueue
  - Dedup within queue: same (objectId, kind) → last-write-wins
  - Dedup across flush: within dedupWindowMs (default 5000ms), recently flushed items are skipped
  - `flush()` — manually deliver all pending to store
  - `pending()` — queued count
  - `dispose()` — clear pending, cancel timers
  - Pluggable `TimerProvider` for testing (setTimeout, clearTimeout, now)

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (notification-store) | 36 | Pass |
| Vitest (notification-queue) | 12 | Pass |
| **Phase 15 Total** | **48** | **All Pass** |

## Phase 16: Activity Log (Complete)

Audit trail of GraphObject mutations with actor/timestamp. All pure TypeScript, zero React.

### Completed

- [x] **Activity Store** (`layer1/activity/activity-log.ts`) — append-only per-object event log
  - `ActivityVerb` — 20 semantic verbs: created, updated, deleted, restored, moved, renamed, status-changed, commented, mentioned, assigned, unassigned, attached, detached, linked, unlinked, completed, reopened, blocked, unblocked, custom
  - `FieldChange` — before/after record for a single field mutation
  - `ActivityEvent` — immutable audit record with verb-specific fields (changes, fromParentId/toParentId, fromStatus/toStatus, meta)
  - `createActivityStore(options?)` — in-memory ring buffer per object (default 500 events)
  - `record()` — append event with auto-generated id and createdAt
  - `getEvents(objectId, opts?)` — newest-first retrieval with limit and before filters
  - `getLatest()` / `getEventCount()` — quick access queries
  - `hydrate(objectId, events)` — bulk load from persistence (sorts by createdAt)
  - `subscribe(objectId, listener)` — per-object change notifications
  - `toJSON()` / `clear()` — serialisation and reset
- [x] **Activity Tracker** (`layer1/activity/activity-tracker.ts`) — auto-derives events from GraphObject diffs
  - `TrackableStore` — duck-typed subscription interface (structurally compatible with CollectionStore)
  - `createActivityTracker(options)` — watches objects via per-object subscriptions
  - `track(objectId, store)` — begin watching; diffs snapshots on each emission
  - Verb inference: deleted/restored (deletedAt), moved (parentId), renamed (only name), status-changed (status field), updated (fallback)
  - Shell field diffing + data payload one-level-deep diffing
  - `ignoredFields` config (default: updatedAt) to filter noise
  - Handles object appeared (created if age < 5s) and disappeared (hard delete)
  - `untrackAll()` — stop all subscriptions, `trackedIds()` — introspection
- [x] **Activity Formatter** (`layer1/activity/activity-formatter.ts`) — human-readable event rendering
  - `formatFieldName(field)` — raw field path to display label (data. prefix strip, camelCase split, overrides)
  - `formatFieldValue(value)` — inline display formatting (null → "(none)", booleans, arrays, ISO dates, truncation)
  - `formatActivity(event, opts?)` — text + HTML description for all 20 verbs
  - `groupActivityByDate(events)` — Today/Yesterday/This week/Earlier buckets for timeline rendering

### Axed from Legacy

- `ActivityStore` class — replaced with factory function (Prism convention)
- `storageKey` option — Prism uses Loro CRDT for persistence, not localStorage
- `ITrackableStore` class-based — replaced with `TrackableStore` structural interface

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (activity-log) | 21 | Pass |
| Vitest (activity-tracker) | 21 | Pass |
| Vitest (activity-formatter) | 51 | Pass |
| **Phase 16 Total** | **93** | **All Pass** |

## Phase 17: Batch Operations, Clipboard, Templates (Complete)

Utility Layer 1 systems for bulk manipulation and reuse. All pure TypeScript, zero React.

### Completed

- [x] **Batch Operations** (`layer1/batch/`)
  - `BatchOp` — 7 operation kinds: create-object, update-object, delete-object, move-object, create-edge, update-edge, delete-edge
  - `createBatchTransaction(options)` — collect ops, validate, execute atomically
  - `validate()` — pre-flight checks (missing IDs, types, EdgeModel presence)
  - `execute(options?)` — apply all mutations, push single undo entry, rollback on failure
  - `BatchProgressCallback` — called before each op with current/total/op
  - Undo integration: entire batch = one UndoRedoManager.push() call
- [x] **Clipboard** (`layer1/clipboard/`)
  - `createTreeClipboard(options)` — cut/copy/paste for GraphObject subtrees
  - `copy(ids)` — deep-clone subtrees with descendants + internal edges
  - `cut(ids)` — copy + delete sources on paste (one-time)
  - `paste(options?)` — remap all IDs, reattach under target parent, recreate internal edges
  - `SerializedSubtree` — portable snapshot: root + descendants + internalEdges
  - `PasteResult` — created objects, created edges, oldId→newId map
  - Single undo entry for paste (includes cut deletions)
- [x] **Template System** (`layer1/template/`)
  - `createTemplateRegistry(options)` — catalog of reusable ObjectTemplates
  - `ObjectTemplate` — blueprint: root TemplateNode tree + TemplateEdge[] + TemplateVariable[]
  - `register(template)` / `unregister(id)` / `list(filter?)` — CRUD with category/type/search filtering
  - `instantiate(templateId, options?)` — create live objects from template with variable interpolation
  - `createFromObject(objectId, meta)` — snapshot existing subtree as reusable template (round-trip capable)
  - Variable interpolation: `{{name}}`, `{{date}}` in name, description, status, data string values
  - Undo integration: single entry for entire instantiation

### Axed from Phase 18 Draft

Phase 18's draft content has been promoted to Phase 17 and completed. The remaining phases (19+) retain their numbering.

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (batch-transaction) | 24 | Pass |
| Vitest (tree-clipboard) | 21 | Pass |
| Vitest (template-registry) | 28 | Pass |
| **Phase 17 Total** | **73** | **All Pass** |

---

## Phase 18: Ephemeral Presence (Complete)

Real-time collaboration awareness. All pure TypeScript, zero React.

### Completed

- [x] **Ephemeral Presence** (`layer1/presence/`)
  - `PresenceState` — cursor position, selection ranges, active view, peer identity, arbitrary data
  - `PeerIdentity` — peerId, displayName, color, optional avatarUrl
  - `CursorPosition` — objectId + optional field + offset for text cursor tracking
  - `SelectionRange` — objectId + optional field + anchor/head for inline selection
  - `createPresenceManager(options)` — RAM-only state for connected peers (no CRDT persistence)
  - `setCursor()` / `setSelections()` / `setActiveView()` / `setData()` — local state updates
  - `updateLocal(partial)` — bulk update of local presence fields
  - `receiveRemote(state)` — ingest remote peer state (from awareness protocol)
  - `removePeer(peerId)` — explicit peer removal
  - `subscribe(listener)` — reactive updates for cursor/selection overlays (joined/updated/left)
  - TTL-based eviction: configurable `ttlMs` + automatic `sweepIntervalMs` sweep timer
  - `sweep()` — manual eviction trigger, returns evicted peer IDs
  - `dispose()` — stop sweep timer, remove all remote peers, clear listeners
  - Injectable `TimerProvider` for deterministic testing

### Note

Phase 18's original draft (Batch/Clipboard/Template) was promoted to Phase 17 and completed there. This phase number now covers Ephemeral Presence (previously Phase 19). Subsequent phases retain their original numbering but shift down by one.

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (presence-manager) | 38 | Pass |
| **Phase 18 Total** | **38** | **All Pass** |

---

## Phase 19: Identity & Encryption ✅

W3C DIDs and vault-level encryption. Foundation for federation and trust.

- [x] **DID Identity** (`layer1/identity/`)
  - [x] `PrismIdentity` — W3C DID document wrapper (did:web, did:key)
  - [x] `createIdentity()` — generate Ed25519 keypair + DID document
  - [x] `resolveIdentity(did)` — resolve DID to public key and metadata
  - [x] Threshold multi-sig support for shared vault ownership (createMultiSigConfig, createPartialSignature, assembleMultiSignature, verifyMultiSignature)
  - [x] `signPayload()` / `verifySignature()` — Ed25519 sign/verify for CRDT updates
  - [x] Base58btc + multicodec encoding for did:key format
- [x] **Vault Encryption** (`layer1/encryption/`)
  - [x] `VaultKeyManager` — HKDF-derived AES-GCM-256 vault key from identity keypair
  - [x] `encryptSnapshot()` / `decryptSnapshot()` — encrypt Loro snapshots at rest
  - [x] Per-collection encryption with key rotation support (deriveCollectionKey, rotateKey)
  - [x] Secure key storage integration (KeyStore interface for Tauri keychain / Secure Enclave bridge, createMemoryKeyStore for testing)
  - [x] AAD (Additional Authenticated Data) support for binding ciphertext to collection context
  - [x] Standalone encryptSnapshot/decryptSnapshot for one-off encryption without VaultKeyManager

### Implementation Notes

- All crypto via Web Crypto API (SubtleCrypto) — works in Node.js 20+, browsers, Tauri WebView
- Ed25519 for signing (64-byte signatures), AES-GCM-256 for encryption, HKDF-SHA-256 for key derivation
- DID:key uses multibase z-prefix + base58btc + Ed25519 multicodec (0xed01) per W3C spec
- DID:web builds proper `did:web:domain:path` URIs; resolution requires network resolver (interface ready, not yet wired)
- Multi-sig uses threshold scheme: collect N-of-M partial Ed25519 signatures, verify each individually
- Key rotation derives new key from existing material + version-tagged salt — old ciphertext needs old key version

### Files

- `identity-types.ts` — DID, DIDDocument, PrismIdentity, MultiSigConfig, KeyHandle types
- `identity.ts` — createIdentity, resolveIdentity, signPayload, verifySignature, multi-sig functions, base58btc codec
- `encryption-types.ts` — VaultKeyInfo, EncryptedSnapshot, KeyStore, VaultKeyManager types
- `encryption.ts` — createVaultKeyManager, createMemoryKeyStore, standalone encrypt/decrypt

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (identity) | 29 | Pass |
| Vitest (encryption) | 22 | Pass |
| **Phase 19 Total** | **51** | **All Pass** |

---

## Phase 21: Virtual File System ✅

Decouples the object-graph (text/CRDTs) from heavy binary assets.

- [x] **VFS Layer** (`layer1/vfs/`)
  - [x] `VfsAdapter` interface — abstract file I/O (read, write, stat, list, delete, has, count, totalSize)
  - [x] `createMemoryVfsAdapter()` — in-memory adapter for testing
  - [x] `createLocalVfsAdapter()` — VfsAdapter interface ready; Tauri impl deferred to daemon phase
  - [x] `BinaryRef` — content-addressed reference (SHA-256 hash) stored in GraphObject.data
  - [x] Binary Forking Protocol: acquireLock/releaseLock/replaceLockedFile for non-mergeable files
  - [x] `importFile()` / `exportFile()` — move binaries in/out of vault storage via VfsManager
  - [x] Deduplication via content addressing (same SHA-256 hash = one blob)
  - [x] `computeBinaryHash()` — standalone SHA-256 hash utility

### Implementation Notes

- SHA-256 content addressing via Web Crypto API, hex-encoded (64 chars)
- VfsManager wraps VfsAdapter with lock management + import/export convenience
- Binary Forking Protocol: lock → edit → replaceLockedFile (new blob, moved lock, old preserved) → release
- dispose() clears locks only; blobs persist for history/undo

### Files

- `vfs-types.ts` — BinaryRef, FileStat, BinaryLock, VfsAdapter, VfsManager types
- `vfs.ts` — createMemoryVfsAdapter, createVfsManager, computeBinaryHash

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (vfs) | 34 | Pass |
| **Phase 21 Total** | **34** | **All Pass** |

---

## Next: Phase 22: Federation & Sync

Cross-node object addressing, CRDT sync, and ghost nodes.

- [ ] **Federated Addressing** (`layer1/federation/`)
  - [ ] `PrismAddress` resolution — `prism://did:web:node/objects/id` → local or remote fetch
  - [ ] `GhostNode` — locked placeholder for objects in unshared collections
  - [ ] `FederatedEdge` — edge where source and target live on different nodes
  - [ ] `resolveRemoteObject()` — fetch object snapshot from peer via Relay or direct
- [ ] **Sync Engine** (`layer1/sync/`)
  - [ ] `SyncSession` — bidirectional Loro CRDT sync between two peers
  - [ ] `SyncTransport` interface — pluggable transport (WebSocket, Tauri IPC, Relay)
  - [ ] `createDirectSyncTransport()` — peer-to-peer WebSocket transport
  - [ ] `createRelaySyncTransport()` — store-and-forward via Prism Relay
  - [ ] Conflict quarantine: detect divergent non-CRDT state, surface for manual resolution

## Phase 23: Prism Relay ✅

Modular, composable relay runtime. The Relay is the bridge between Core/Daemon and the outside world — NOT just a server. Users mix and match Web 1/2/3 features via builder pattern. Next.js is optional (for Sovereign Portals).

- [x] **Relay Builder** (`layer1/relay/`)
  - [x] `createRelayBuilder()` — composable builder with `.use()` chaining and `.configure()` overrides
  - [x] `RelayModule` interface — pluggable modules with name/description/dependencies/install/start/stop lifecycle
  - [x] `RelayContext` — shared capability registry for inter-module communication
  - [x] `RelayInstance` — built relay with start/stop lifecycle, capability access, module listing
  - [x] Dependency validation at build time (missing deps, duplicate modules)
  - [x] `RELAY_CAPABILITIES` — well-known capability names for standard modules
- [x] **Blind Mailbox** (module: `blind-mailbox`)
  - [x] E2EE store-and-forward message queue — deposit/collect/pendingCount/evict
  - [x] TTL-based expiry eviction for stale envelopes
  - [x] `RelayEnvelope` — encrypted payload with from/to DID, TTL, optional proof-of-work
- [x] **Relay Router** (module: `relay-router`, depends on blind-mailbox)
  - [x] Zero-knowledge routing: delivers to online peers or queues to mailbox
  - [x] `registerPeer()` — flushes queued envelopes when peer comes online
  - [x] Rejects oversized envelopes (configurable max size)
- [x] **Relay Timestamping** (module: `relay-timestamp`)
  - [x] `stamp()` — cryptographic Ed25519-signed timestamp receipts for data hashes
  - [x] `verify()` — validate receipt signatures
- [x] **Blind Pings** (module: `blind-pings`)
  - [x] Content-free push notifications with pluggable `PingTransport` (APNs, FCM, etc.)
  - [x] `createMemoryPingTransport()` for testing
- [x] **Capability Tokens** (module: `capability-tokens`)
  - [x] Scoped access tokens with Ed25519 signatures — issue/verify/revoke
  - [x] TTL-based expiry, wildcard subjects, tamper detection
- [x] **Webhooks** (module: `webhooks`)
  - [x] Register/unregister/list webhooks with event filtering and wildcard support
  - [x] Pluggable `WebhookHttpClient` for outgoing HTTP; dry-run mode without client
  - [x] HMAC-SHA256 payload signatures, delivery logging
- [x] **Sovereign Portals** (module: `sovereign-portals`)
  - [x] `PortalRegistry` — register/unregister/list/resolve portals
  - [x] Portal levels 1-4 (read-only → complex webapp)
  - [x] Domain + path resolution for routing requests to portals
  - [x] SSR/AutoREST/Next.js integration deferred to `packages/prism-relay/` runtime package

### Implementation Notes

- Relay is a Layer 1 module (agnostic TS) — the actual server runtime (`packages/prism-relay/`) will import these primitives
- Builder pattern central: `createRelayBuilder({ relayDid }).use(mod1()).use(mod2()).build()`
- "Choose your own adventure": Web 1.0 (just portals), Web 2.0 (portals + webhooks), full (all 7 modules), or custom
- Custom modules implement `RelayModule` interface and register capabilities via `RelayContext`
- All crypto uses existing identity module (Ed25519 signing for timestamps/tokens)
- Zero-knowledge: router sees `RelayEnvelope` with encrypted ciphertext, never plaintext

### Files

- `relay-types.ts` — RelayEnvelope, BlindMailbox, RelayRouter, RelayTimestamper, BlindPinger, CapabilityToken/Manager, WebhookEmitter, PortalRegistry, RelayModule, RelayContext, RelayBuilder, RelayInstance types
- `relay.ts` — createRelayBuilder, 7 module factories, createMemoryPingTransport

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (relay) | 44 | Pass |
| **Phase 23 Total** | **44** | **All Pass** |

---

## Phase 24: Actor System (0B/0C) (Complete)

Process queue, language runtimes, and local AI integration.

- [x] **Process Queue** (`layer1/actor/`)
  - [x] `createProcessQueue()` — priority-ordered execution with concurrency control, auto-processing, cancel/prune/dispose
  - [x] `ActorRuntime` interface — pluggable language execution with capability-scoped sandboxing
  - [x] `createLuauActorRuntime()` — wraps luau-web via function injection (no hard WASM dependency)
  - [x] `createSidecarRuntime()` — TypeScript/Python via `SidecarExecutor` interface (Daemon provides Tauri shell)
  - [x] `createTestRuntime()` — synchronous in-memory runtime for testing
  - [x] `CapabilityScope` — zero-trust by default, explicit permission grants per task (network, fs, crdt, spawn, endpoints, duration/memory limits)
  - [x] Queue events: enqueued/started/completed/failed/cancelled with subscribe/unsubscribe
- [x] **Intelligence Layer** (0C)
  - [x] `AiProvider` interface — pluggable providers with name/target/defaultModel/complete/completeInline/listModels/isAvailable
  - [x] `createAiProviderRegistry()` — register, switch active, delegate complete/completeInline
  - [x] `createOllamaProvider()` — local Ollama inference via injected `AiHttpClient` (chat + inline fill-in-the-middle)
  - [x] `createExternalProvider()` — OpenAI-compatible API bridge for Claude, OpenAI, etc. with Bearer auth
  - [x] `createContextBuilder()` — object-aware context from graph neighbors (ancestors/children/edges/collection) with configurable limits, `toSystemMessage()` for AI prompts
  - [x] `createTestAiProvider()` — canned response provider for testing
  - [x] `AiHttpClient` interface — abstracts HTTP calls to avoid fetch dependency in Layer 1

### Implementation Notes
- Three execution targets: Sovereign Local, Federated Delegate, External Provider
- Actor types in `actor-types.ts`, AI types in `ai-types.ts` (separate concerns)
- All HTTP calls abstracted behind interfaces (AiHttpClient, SidecarExecutor) — Layer 1 has no runtime dependencies
- ProcessQueue supports priority ordering (lower = higher), concurrency control, and fire-and-forget auto-processing

### Test Summary (49 tests)
| Suite | Tests | Status |
|-------|-------|--------|
| ProcessQueue basics | 14 | Pass |
| Auto-processing | 2 | Pass |
| LuauActorRuntime | 4 | Pass |
| SidecarRuntime | 3 | Pass |
| AiProviderRegistry | 7 | Pass |
| OllamaProvider | 7 | Pass |
| ExternalProvider | 4 | Pass |
| ContextBuilder | 4 | Pass |
| TestAiProvider | 3 | Pass |
| **Phase 24 Total** | **49** | **All Pass** |

---

## Phase 25: Prism Syntax Engine (Complete)

LSP-like intelligence for the expression and scripting layers.

- [x] **Syntax Engine** (`layer1/syntax/`)
  - [x] `createSyntaxEngine()` — orchestrates SyntaxProviders for diagnostics, completions, hover
  - [x] `createExpressionProvider()` — built-in provider for the Prism expression language
  - [x] Diagnostics: parse errors with source positions, unknown fields/functions, wrong arity, type mismatches
  - [x] Completions: fields (from SchemaContext), functions (9 builtins), keywords, operators with prefix filtering
  - [x] Hover: field type/description/enum values/computed expressions, function signatures, literals, keyword operators
  - [x] `inferNodeType()` — AST type inference mapping EntityFieldType → ExprType via FIELD_TYPE_MAP
  - [x] `validateTypes()` — schema-aware type checking (arithmetic on strings, unknown fields, wrong arity)
  - [x] `generateLuauTypeDef()` — .d.luau generation from ObjectRegistry schemas with @class/@field annotations, enum unions, optional markers, standard GraphObject fields, builtin function stubs
  - [x] `SyntaxProvider` interface for custom language providers (pluggable beyond expression)
  - [x] CodeMirror integration ready: TextRange positions compatible with CM offsets (Layer 2 wiring deferred)

### Implementation Notes
- Types in `syntax-types.ts`, implementation in `syntax.ts` (separation of concerns)
- FIELD_TYPE_MAP maps all 11 EntityFieldTypes to ExprType (number/boolean/string)
- BUILTIN_FUNCTIONS defines 9 functions with param types for completion detail and hover
- SchemaContext provides the bridge from ObjectRegistry to the syntax engine
- SyntaxProvider interface allows adding Luau/TypeScript language providers in future phases

### Test Summary (68 tests)
| Suite | Tests | Status |
|-------|-------|--------|
| Expression diagnostics | 9 | Pass |
| Expression completions | 9 | Pass |
| Expression hover | 10 | Pass |
| Type inference | 15 | Pass |
| SyntaxEngine | 9 | Pass |
| Luau typedef generation | 7 | Pass |
| FIELD_TYPE_MAP | 4 | Pass |
| Edge cases | 5 | Pass |
| **Phase 25 Total** | **68** | **All Pass** |

## Phase 26: Communication Fabric (Complete)

Real-time sessions, transcription, and A/V transport.

- [x] **Session Nodes** (`layer1/session/`)
  - [x] `createSessionManager()` — session lifecycle (create/join/end/pause/resume) with participant/track/delegation management
  - [x] `createTranscriptTimeline()` — ordered, searchable, time-indexed transcript segments with binary-insert sort, finalization, range queries, text search, plain text export
  - [x] `createPlaybackController()` — transcript-synced media seek with speed control (0.25x–4x), seekToSegment, position listeners
  - [x] Self-Dictation: `TranscriptionProvider` interface for Whisper.cpp sidecar integration (Tauri provides the executor)
  - [x] Hypermedia Playback: `seekToSegment()` jumps playback to transcript segment start time
  - [x] Listener Fallback: `requestDelegation()`/`respondToDelegation()` for compute delegation to capable peers
- [x] **A/V Transport**
  - [x] `SessionTransport` interface — abstract transport for LiveKit (SFU), WebRTC (P2P), or custom
  - [x] `createTestTransport()` — in-memory transport for testing (connect/disconnect/publish/unpublish/events)
  - [x] `createTestTranscriptionProvider()` — test provider with `feedSegment()` for simulating transcription
  - [x] `MediaTrack` management — add/remove/mute tracks with participant activeMedia sync
  - [x] Transport events: connected/disconnected/participant-joined/left/track-published/unpublished/muted/unmuted/data-received

### Implementation Notes
- Types in `session-types.ts`, implementation in `session.ts` (separation of concerns)
- All external dependencies abstracted behind interfaces: SessionTransport (LiveKit/WebRTC), TranscriptionProvider (Whisper.cpp)
- SessionManager is a pure state machine — no network I/O, no timers (transport layer handles that)
- TranscriptTimeline uses binary insert for O(log n) sorted insertion by startMs
- Non-final segments can be updated in place (streaming transcription refinement)
- Participant roles: host, speaker, listener, observer
- Delegation targets participants with `canDelegate: true`

### Test Summary (64 tests)
| Suite | Tests | Status |
|-------|-------|--------|
| TranscriptTimeline | 15 | Pass |
| PlaybackController | 9 | Pass |
| TestTransport | 5 | Pass |
| TestTranscriptionProvider | 4 | Pass |
| SessionManager lifecycle | 7 | Pass |
| SessionManager participants | 5 | Pass |
| SessionManager media tracks | 5 | Pass |
| SessionManager transcript | 3 | Pass |
| SessionManager delegation | 5 | Pass |
| SessionManager events | 6 | Pass |
| **Phase 26 Total** | **64** | **All Pass** |

## Phase 27: Trust & Safety (Complete)

The Sovereign Immune System — sandbox, spam protection, content trust.

- [x] **Luau Sandbox** — `createLuauSandbox()` capability-based API restriction per plugin with glob URL/path filtering, violation recording
- [x] **Schema Validation** — `createSchemaValidator()` 5 built-in rules (max-depth, max-string-length, max-array-length, max-total-keys, disallowed-keys for __proto__/constructor/prototype)
- [x] **Relay Spam Protection** — `createHashcashMinter()`/`createHashcashVerifier()` SHA-256 proof-of-work via Web Crypto with configurable difficulty bits
- [x] **Web of Trust** — `createPeerTrustGraph()` peer reputation scoring with configurable thresholds, trust/distrust/ban, content hash flagging, event listeners
- [x] **Secure Recovery** — `createShamirSplitter()` GF(256) Shamir secret sharing with configurable threshold/total shares
- [x] **Relay Encrypted Escrow** — `createEscrowManager()` deposit/claim/evict lifecycle with TTL expiry

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Luau Sandbox | 10 | Pass |
| Schema Validator | 10 | Pass |
| Hashcash | 8 | Pass |
| Peer Trust Graph | 10 | Pass |
| Shamir Secret Sharing | 8 | Pass |
| Escrow Manager | 7 | Pass |
| **Phase 27 Total** | **53** | **All Pass** |

## Phase 28: Builder 3 (3D Viewport) (Complete — Layer 2)

R3F-based 3D editor for spatial content.

- [x] **3D Viewport** (`layer2/viewport3d/`)
  - [x] R3F + @react-three/drei scene graph (types, SceneNode/SceneGraph model)
  - [x] OpenCASCADE.js for CAD geometry (STEP/IGES import, tessellation, bounding box, mesh merge)
  - [x] TSL shader compilation (Three.js Shading Language → WebGPU/WebGL, node graph → GLSL)
  - [x] Loro-backed scene state: object transforms, materials, hierarchy in CRDT
  - [x] Gizmo controls: translate, rotate, scale with undo integration

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Scene State | 19 | Pass |
| CAD Geometry | 15 | Pass |
| TSL Compiler | 19 | Pass |
| Gizmo Controls | 18 | Pass |
| **Phase 28 Total** | **71** | **All Pass** |

## Phase 29: NLE / Timeline System (Grip) (Complete — Layer 1)

Non-linear editing and show control for live production.

- [x] **Timeline Engine** (`layer1/timeline/`)
  - [x] Pluggable `TimelineClock` abstraction (Layer 2 provides tone.js/rAF, tests use `ManualClock`)
  - [x] Track model: 5 kinds (audio, video, lighting, automation, midi) with mute/solo/lock/gain
  - [x] Clip model: time-range regions with sourceRef, sourceOffset, trim, move between tracks, lock/mute/gain
  - [x] Transport controls: play, pause, stop, seek, scrub, setSpeed, loop regions
  - [x] Automation lanes: step/linear/bezier interpolation, per-parameter breakpoint curves
  - [x] Timeline markers: sorted by time, custom colors
  - [x] Tempo map (PPQN): dual time model (seconds ↔ bar/beat/tick), tempo automation, time signature changes
  - [x] Event system: 14 event kinds with subscribe/unsubscribe
  - [x] Reference: OpenDAW SDK (naomiaro/opendaw-test) for Layer 2 audio integration
- [x] **Audio Pipeline** (`layer2/audio/`) — OpenDAW SDK bridge
  - [x] `createOpenDawBridge()` — bidirectional sync between Prism timeline and OpenDAW engine
  - [x] Track loading: AudioFileBox, AudioRegionBox, PPQN conversion, sample provider
  - [x] Transport sync: AnimationFrame position → Prism scrub, timeline events → OpenDAW play/stop
  - [x] 10 audio effects via EffectFactories (Reverb, Compressor, Delay, Crusher, EQ, etc.)
  - [x] Volume/pan/mute/solo per-track control
  - [x] Export: full mix and individual stems to WAV via AudioOfflineRenderer
  - [x] React hooks: useOpenDawBridge, usePlaybackPosition, useTransportControls, useTrackEffects
  - [x] Reference fork: cosmicmonkeysounds/opendaw-prism
  - [ ] peaks.js / waveform-playlist for waveform rendering (future)
  - [ ] WAM (Web Audio Modules) standard for VST-like plugins (future)
- [ ] **Video Pipeline** (Layer 2 — future)
  - [ ] WebCodecs API for frame-accurate seeking
  - [ ] Proxy workflow: low-res edit → full-res export
- [ ] **Hardware Bridges** (Rust daemon — future)
  - [ ] Art-Net (DMX lighting control)
  - [ ] VISCA over IP (PTZ camera control)
  - [ ] OSC (Open Sound Control)
  - [ ] MIDI (instrument/controller I/O)

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Transport | 10 | Pass |
| Tracks | 8 | Pass |
| Clips | 10 | Pass |
| Automation | 8 | Pass |
| Markers | 6 | Pass |
| Queries | 4 | Pass |
| Events | 1 | Pass |
| Lifecycle | 1 | Pass |
| ManualClock | 6 | Pass |
| TempoMap | 10 | Pass |
| **Phase 29 Total** | **67** | **All Pass** |

## Phase 30: Ecosystem Apps — Flux (Complete — Layer 1 Domain Schemas)

Operational hub: productivity, finance, CRM, goals, inventory.

- [x] **Flux Domain Schemas** (`layer1/flux/`)
  - [x] 11 EntityDef schemas: Task, Project, Goal, Milestone, Contact, Organization, Transaction, Account, Invoice, Item, Location
  - [x] 4 categories: productivity, people, finance, inventory
  - [x] 7 edge types: assigned-to, depends-on, blocks, belongs-to, related-to, invoiced-to, stored-at
  - [x] 8 automation presets: task completion timestamps, recurring task reset, overdue notifications, invoice overdue, low/out-of-stock alerts, goal progress tracking, project completion
  - [x] Computed fields: invoice tax/total, item stock value, goal progress formulas
  - [x] CRM fields on Contact: deal value, deal stage pipeline (prospect→closed)
  - [x] Import/export: CSV and JSON with field selection
  - [x] NSIDs for all entity and edge types (io.prismapp.flux.*)
- [ ] **Flux App** (`packages/prism-flux/` — future)
  - [ ] Lens plugins: Tasks, Contacts, Projects, Goals, Finance, Inventory
  - [ ] Dashboard views: kanban, calendar, timeline per entity type

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Entity Definitions | 12 | Pass |
| Edge Definitions | 7 | Pass |
| Automation Presets | 5 | Pass |
| CSV Export/Import | 6 | Pass |
| JSON Export/Import | 4 | Pass |
| Edge Cases | 2 | Pass |
| **Phase 30 Total** | **38** | **All Pass** |

## Phase 30b: Studio Kernel — Page Builder Wiring (Complete)

Wire all Layer 1 systems into Prism Studio, transforming the demo shell into a real page builder.

### Completed

- [x] `StudioKernel` — singleton factory wiring ObjectRegistry + CollectionStore + PrismBus + AtomStore + ObjectAtomStore + UndoRedoManager + NotificationStore
- [x] `entities.ts` — 8 entity types (folder, page, section, heading, text-block, image, button, card), 4 category containment rules, 2 edge types
- [x] `kernel-context.tsx` — React context + hooks: useKernel, useSelection, useObjects, useObject, useUndo, useNotifications (all useSyncExternalStore)
- [x] `StudioShell` — custom shell with real ObjectExplorer sidebar + InspectorPanel + UndoStatusBar
- [x] `ObjectExplorer` — hierarchical tree from CollectionStore, click-to-select, "New Page" creation
- [x] `InspectorPanel` — schema-driven property editor from ObjectRegistry EntityDef fields, grouped fields, Add Child, Delete
- [x] `NotificationToast` — auto-dismissing toast overlay from NotificationStore
- [x] `UndoStatusBar` — undo/redo buttons wired to UndoRedoManager
- [x] CRUD with undo: createObject, updateObject, deleteObject, createEdge, deleteEdge all push undo snapshots
- [x] Bus → AtomStore → React reactivity chain fully connected
- [x] Seed demo data (Home/About pages with sections, heading, text-block)
- [x] KBar undo/redo actions + Cmd+Z/Cmd+Shift+Z keyboard shortcuts
- [x] All panels updated: CRDT panel shows CollectionStore, Graph panel renders real objects, Editor uses kernel context
- [x] `data-testid` attributes on all interactive elements for Playwright

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (studio-kernel) | 23 | Pass |
| Playwright (studio-kernel) | 18 | Written |
| **Phase 30b Total** | **23 Vitest + 18 E2E** | **All Pass** |

## Phase 30c: Studio Canvas + Search + Editor Wiring (Complete)

WYSIWYG preview, search integration, and object-aware editing.

### Completed

- [x] `CanvasPanel` — WYSIWYG page preview rendering page→section→component hierarchy as visual React components
  - Resolves selected page (walks up parentId for child selections)
  - Renders heading/text-block/image/button/card with proper styling
  - Click-to-select blocks in canvas (blue outline highlight)
  - Section padding/background from entity data, layout max-width from page data
- [x] `SearchEngine` integration — `kernel.search` indexed against CollectionStore with auto-reindex on changes
- [x] `ObjectExplorer` search — text input filters objects via SearchEngine, flat result list, click to select
- [x] `EditorPanel` object-aware — edits `text-block.data.content` or `heading.data.text` of selected object
  - Per-object LoroText buffers keyed as `obj_content_{id}`
  - Debounced (500ms) sync back to kernel via updateObject
  - Falls back to scratch buffer when nothing editable is selected
- [x] Canvas lens registered (shortcut: v), now 5 lenses total
- [x] E2E tests updated for 5 lenses (shell, keyboard, tabs specs)
- [x] New Playwright tests: canvas preview (4) + search (4)

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (studio-kernel) | 23 | Pass |
| Playwright (canvas) | 4 | Written |
| Playwright (search) | 4 | Written |
| **Phase 30c Total** | **23 Vitest + 8 E2E** | **All Pass** |

## Phase 30d: Studio Tier 0 — Kernel Feature Wiring (Complete)

Wired all Layer 1 systems into Studio kernel: Search (already done in 30c), Clipboard, Batch Operations, Activity Tracking, Templates, and LiveView.

### Completed
- [x] **Clipboard** — deep copy/cut/paste with subtree traversal, internal edge preservation, ID remapping
- [x] **Batch Operations** — atomic multi-op (create/update/delete) with single undo entry
- [x] **Activity Tracking** — ActivityStore + TrackableStore adapter for CollectionStore, records create/delete events
- [x] **Templates** — register/list/instantiate with `{{variable}}` interpolation, recursive TemplateNode traversal, edge remapping
- [x] **LiveView** — real-time filtered/sorted views over CollectionStore with type facets and dispose

### Key Decisions
- Clipboard/Batch/Templates implemented directly in kernel using CollectionStore primitives (not TreeModel adapters) — cleaner integration with bus events, undo, and atom sync
- `createTrackableAdapter()` bridges CollectionStore → ActivityTracker's duck-typed `{ get, subscribeObject }` interface
- Template instantiation uses `{{name}}` regex interpolation, matching legacy Helm pattern

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (studio-kernel) | 43 | Pass |
| — Clipboard | 4 | Pass |
| — Batch | 3 | Pass |
| — Activity | 3 | Pass |
| — Templates | 5 | Pass |
| — LiveView | 5 | Pass |
| **Phase 30d Total** | **43 Vitest** | **All Pass** |

## Phase 30e: Studio Tier 1 — UI Surface + E2E (Complete)

Surfaced kernel features (clipboard, templates, activity, reorder) in the Studio UI with full Playwright E2E coverage.

### Completed

- [x] **Clipboard UI** — Copy/Cut/Paste buttons in inspector panel + Cmd+C/X/V keyboard shortcuts
  - Inspector shows clipboard section when object selected
  - Paste button disabled when clipboard empty
  - Keyboard shortcuts skip when focus is in input/textarea/contenteditable
- [x] **Template Gallery** — "Templates" button opens gallery overlay
  - Lists registered templates (Blog Post, Landing Page)
  - Click to instantiate with default variables
  - Close button dismisses gallery
- [x] **Activity Feed** — Recent activity section in object explorer sidebar
  - Shows last 10 events from ActivityStore (newest first)
  - Click event to select the corresponding object
  - Auto-updates when objects are created
- [x] **Object Reorder** — Move up/down buttons on selected explorer nodes
  - Position swap with sibling above/below
  - Disabled at boundaries (first/last)
  - Tree re-renders reactively via change-counter versioning

### Bug Fixes

- Fixed `useSyncExternalStore` version tracking: `allObjects().length` doesn't change on reorder — replaced with monotonic counter refs that increment on every store `onChange`
- Fixed Vite alias resolution: generated per-export aliases from `@prism/core` package.json exports map
- Fixed elkjs `web-worker` resolution: aliased to `elkjs/lib/elk.bundled.js`
- Fixed React 19 infinite loop: `getSnapshot` must return stable primitives, not new arrays/objects

### Test Summary

| Suite | Count | Status |
|-------|-------|--------|
| Vitest (all packages) | 1955 | Pass |
| Playwright — Tier 1 | 18 | Pass |
| — Clipboard UI | 5 | Pass |
| — Template Gallery | 6 | Pass |
| — Activity Feed | 4 | Pass |
| — Object Reorder | 3 | Pass |
| **Phase 30e Total** | **1955 Vitest + 18 E2E** | **All Pass** |

## Phase 30f: Prism Relay — Server Package + Sovereign Portals (Complete)

Full server runtime for Prism Relay (`packages/prism-relay/`), deployable to any VPS or container.

### Completed
- [x] **Hono HTTP Server** — `createRelayServer()` with all 14 modules wired as HTTP routes
- [x] **WebSocket Transport** — auth, envelope routing, CRDT sync, hashcash, ping/pong
- [x] **ConnectionRegistry** — tracks WS connections + collection subscriptions for broadcast
- [x] **Deployment CLI** — 3 modes (server/p2p/dev), config file, env vars, CLI flags
- [x] **Identity Persistence** — Ed25519 JWK export/import, auto-create on first run
- [x] **Federation Transport** — HTTP-based envelope forwarding between relay peers
- [x] **Relay Client SDK** — `createRelayClient()` with auth, send/receive, CRDT sync, auto-reconnect
- [x] **Config System** — 4-layer resolution (CLI > env > config file > mode defaults)
- [x] **Docker** — multi-stage Dockerfile for production deployment
- [x] **Per-Package E2E** — Playwright tests moved from global e2e/ to per-package
- [x] **Sovereign Portal Rendering** — Hono JSX SSR for Level 1-4 portals
  - [x] `extractPortalSnapshot()` — tree-structured data extraction from CollectionStore
  - [x] `renderPortalHtml()` — fallback static HTML renderer (framework-agnostic)
  - [x] Portal view routes: `GET /portals`, `GET /portals/:id`, `GET /portals/:id/snapshot.json`
  - [x] Level 2 incremental DOM patching: client-side WS script fetches snapshot JSON and patches `#portal-content` without full-page reload
  - [x] Level 3 interactive forms: `POST /portals/:id/submit` with ephemeral DID auth, capability token verification for non-public portals, form rendering in portal pages
  - [x] Level 4 client-side hydration: `window.__PRISM_PORTAL__` API with subscribe/notify, bidirectional CRDT sync via WebSocket, `sendUpdate()`/`submitObject()` methods
- [x] **Let's Encrypt SSL Provisioning** — ACME HTTP-01 challenge routes (`/.well-known/acme-challenge/:token`), `AcmeCertificateManager` for certificate lifecycle, management API (`/api/acme/challenges`, `/api/acme/certificates`)
- [x] **Portal Template System** — `PortalTemplateRegistry` for user-defined layouts with custom CSS/HTML templates, management API (`/api/templates`)

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Portal Renderer | 11 | Pass |
| Portal View Routes | 7 | Pass |
| Relay Server | 13 | Pass |
| Relay Client | 8 | Pass |
| Config | 11 | Pass |
| Parse Args | 19 | Pass |
| Route Tests (9 files) | 44 | Pass |
| ACME Routes | 5 | Pass |
| Template Routes | 5 | Pass |
| Playwright E2E | 32 | Pass |
| **Phase 30f Total** | **2089 Vitest + 32 E2E** | **All Pass** |

## Phase 30g: Studio Relay Integration (Complete)

Integrated Prism Relay into Studio as a client-only consumer. Studio has NO server code — it manages relays via CLI and connects to them via HTTP + WebSocket.

### Completed

- [x] **RelayManager** (`relay-manager.ts`) — manages relay connections from Studio
  - Add/remove relay endpoint configurations (name + URL)
  - Connect/disconnect via WebSocket (RelayClient SDK)
  - Status tracking with subscriber notifications
  - URL normalization (http→ws, https→wss, trailing slash stripping)
- [x] **Portal Management** — publish/unpublish/list portals via Relay HTTP API
  - `publishPortal()` — POST manifest to relay, returns view URL
  - `unpublishPortal()` — DELETE portal from relay
  - `listPortals()` — GET all portals on a relay
  - `fetchStatus()` — GET relay health/module info
- [x] **Collection Sync** — push CRDT snapshots to relay for portal rendering
  - Creates hosted collection on relay, imports snapshot via HTTP
  - WebSocket live sync for connected relays
- [x] **Kernel Integration** — `kernel.relay` exposes RelayManager to all Studio components
  - `useRelay()` hook for reactive relay state in React
  - Dispose cleans up all relay connections
- [x] **Relay Lens** (shortcut: r) — 6th Studio lens, "Relay Manager" panel
  - Add Relay form (name + URL, Enter key submit)
  - Relay cards with status dot (green/yellow/red/grey), Connect/Disconnect/Remove
  - Publish Portal dialog (name, level 1-4, base path)
  - Portal list with unpublish and view URL links
  - CLI reference section with all deployment modes
  - Summary counter (relays configured + connected count)
- [x] **E2E Tests** (18 Playwright tests) — full coverage of relay panel UI
  - Panel rendering (header, sections, empty state, CLI reference)
  - Add relay form (inputs, submit, notification, clear, Enter key)
  - Relay card actions (connect guidance, remove, notifications)
  - Multiple relays (add multiple, remove selectively)
  - KBar navigation to Relay panel
  - Summary counter updates
- [x] **Existing E2E updated** — shell (6 icons), tabs (6 lenses), keyboard (6 KBar actions)
- [x] **Injectable HTTP/WS clients** for testing (no real network in unit tests)

### Key Decisions
- Studio is client-only: no server code, Relay servers managed via CLI
- RelayManager uses HTTP fetch for portal CRUD, WebSocket for live sync
- `handleConnect()` shows CLI guidance notification — full WS auth requires daemon identity via Tauri IPC (not yet wired)
- Portal publishing defaults to "default" collection (the kernel's CollectionStore)
- HTTP client and WS client factory are injectable for unit testing

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest — RelayManager | 21 | Pass |
| Playwright — Relay Panel | 18 | Pass |
| Playwright — Shell (updated) | 8 | Pass |
| Playwright — Tabs (updated) | 5 | Pass |
| Playwright — Keyboard (updated) | 5 | Pass |
| **Phase 30g Total** | **2084 Vitest + 92 E2E** | **All Pass** |

## Phase 30h: Studio Tier 2 — Settings, View Modes, Presence (Complete)

Surfaced Layer 1 systems that had kernel support but lacked UI: ConfigModel/ConfigRegistry, ViewRegistry, PresenceManager.

### Completed

- [x] **Settings Panel** (`settings-panel.tsx`) — 7th Studio lens (shortcut: ,)
  - Category-grouped settings UI from ConfigRegistry (ui, editor, sync, ai, notifications)
  - Search filter across label, key, and description
  - Toggle switches for boolean settings, dropdowns for select, number/string inputs
  - Setting keys shown in monospace, scope badges, description text
- [x] **View Mode Switcher** in Object Explorer
  - 4 view modes: list (default tree), kanban (columns by status), grid (card tiles), table (rows)
  - Kanban columns grouped by `obj.status`, clickable cards to select
  - Grid view with icon tiles, type labels, click to select
  - Table view with name/type/status columns, click to select rows
  - Search overrides view mode (shows search results regardless of active mode)
  - View mode persists across lens switches
- [x] **Presence Indicators** (`presence-indicator.tsx`) in shell header
  - Colored avatar dots for local + remote peers
  - Initial letter of display name, white border on local peer
  - Peer count badge when remote peers present
  - Reactive via `usePresence()` hook
- [x] **Kernel Wiring** — ConfigRegistry, ConfigModel, PresenceManager, ViewRegistry
  - `useConfig()`, `useConfigSettings()`, `usePresence()`, `useViewMode()` hooks
  - ViewMode state with custom subscribe pattern for useSyncExternalStore
  - dispose() cleans up presence, view mode listeners
- [x] **E2E Tests** (28 new Playwright tests across 3 spec files)
  - Settings panel: rendering, groups, search, toggle, KBar navigation (9 tests)
  - View modes: switcher, kanban/grid/table rendering, click-to-select, persistence, search (14 tests)
  - Presence: indicator rendering, local peer avatar, initial letter, border, color (5 tests)
- [x] **Existing E2E updated** — shell (7 icons), tabs (7 lenses), keyboard (7 KBar actions)

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Playwright — Settings Panel | 9 | Pass |
| Playwright — View Modes | 14 | Pass |
| Playwright — Presence | 5 | Pass |
| Playwright — Shell (updated) | 8 | Pass |
| Playwright — Tabs (updated) | 5 | Pass |
| Playwright — Keyboard (updated) | 5 | Pass |
| **Phase 30h Total** | **2089 Vitest + 120 E2E** | **All Pass** |

## Phase 30i: Relay Hardening — SEO, Auth, Security, Persistence (Complete)

Closed all identified gaps between SPEC.md and the Relay implementation. Updated SPEC.md to reflect Hono JSX architecture (not Next.js).

### Completed

- [x] **SPEC.md updated** — all Next.js references replaced with Hono JSX
- [x] **SEO routes** (`seo-routes.ts`) — `GET /sitemap.xml` auto-generated from public portals, `GET /robots.txt` with crawler directives
- [x] **OpenGraph meta tags** — portal HTML now includes `og:title`, `og:description`, `og:type`, `og:site_name`, Twitter Card tags, and JSON-LD structured data
- [x] **Security middleware** (`middleware/security.ts`)
  - CSRF protection: `X-Prism-CSRF` header required on all mutating `/api/*` requests (disabled in dev mode)
  - Body size enforcement: rejects requests exceeding `maxEnvelopeSizeBytes` via Content-Length check
  - Banned peer rejection: checks `X-Prism-DID` header against PeerTrustGraph
  - CORS updated to allow `X-Prism-CSRF` and `X-Prism-DID` headers
- [x] **OAuth/OIDC auth routes** (`auth-routes.ts`)
  - `GET /api/auth/providers` — lists configured OAuth providers
  - `GET /api/auth/google` + `POST /api/auth/callback/google` — Google OIDC flow
  - `GET /api/auth/github` + `POST /api/auth/callback/github` — GitHub OAuth flow
  - Session tokens issued as Prism capability tokens with configurable TTL
- [x] **Blind Escrow key derivation** — `POST /api/auth/escrow/derive` (PBKDF2-SHA256-600k from password + OAuth salt) and `POST /api/auth/escrow/recover` with key hash matching
- [x] **File-based persistence** (`persistence/file-store.ts`)
  - JSON state file at `{dataDir}/relay-state.json`
  - Persists portals, webhooks, templates, certificates, federation peers, flagged hashes, revoked tokens, collection CRDT snapshots
  - Auto-save on configurable interval, save on shutdown
  - Restore on startup
- [x] **Webhook delivery** — `webhookModule()` now receives a real `WebhookHttpClient` in CLI mode that POSTs to registered URLs with 10s timeout
- [x] **AutoREST API gateway** (`autorest-routes.ts`)
  - `GET/POST /api/rest/:collectionId` — list/create objects
  - `GET/PUT/DELETE /api/rest/:collectionId/:objectId` — CRUD
  - Capability token auth with scope + permission checking
  - Query params: `type`, `status`, `tag`, `limit`, `offset`
  - Fires webhook events on create/update/delete
- [x] **Safety routes** (`safety-routes.ts`)
  - `POST /api/safety/report` — whistleblower packet submission
  - `GET /api/safety/hashes` — list flagged toxic hashes
  - `POST /api/safety/hashes` — import hashes from federated peer
  - `POST /api/safety/check` — batch verify content hashes
  - `POST /api/safety/gossip` — push toxic hashes to all federation peers
- [x] **Blind Ping routes** (`ping-routes.ts`)
  - `POST /api/pings/register` — register device token (APNs/FCM)
  - `DELETE /api/pings/register/:did` — unregister
  - `GET /api/pings/devices` — list registered devices
  - `POST /api/pings/send` — send blind ping to DID
  - `POST /api/pings/wake` — wake all devices for a DID
  - `createPushPingTransport()` — concrete APNs/FCM transport

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| SEO Routes | 3 | Pass |
| Security Middleware | 5 | Pass |
| Auth Routes | 7 | Pass |
| Safety Routes | 5 | Pass |
| AutoREST Routes | 8 | Pass |
| Ping Routes | 6 | Pass |
| Relay Server Integration (updated) | 22 | Pass |
| Existing (unchanged) | 2076 | Pass |
| **Phase 30i Total** | **2132 Vitest + 120 E2E** | **All Pass** |

## Phase 30j: Studio Tier 3 — Automation, Analysis, Expression (Complete)

Wired three additional Layer 1 systems into Studio: AutomationEngine (trigger/condition/action rules), Graph Analysis (dependency graph, critical path, blocking chains, impact), and Expression Engine (formula evaluation).

### Completed

- [x] **Automation Panel** (`automation-panel.tsx`) — 8th Studio lens (shortcut: a)
  - Create/edit/delete automation rules with trigger (manual/object lifecycle/cron) and action (notification/create/update/delete object) configuration
  - Enable/disable toggle with reactive state updates
  - Manual run button with notification feedback
  - Run history tab with status badges (success/failed/skipped/partial)
- [x] **Analysis Panel** (`analysis-panel.tsx`) — 9th Studio lens (shortcut: n)
  - Critical Path tab: CPM plan with total duration, critical path nodes, all nodes with ES/EF/LS/LF/Float
  - Cycles tab: dependency cycle detection
  - Impact tab: blocking chain, downstream impact, slip impact calculator
- [x] **Expression Bar** in Inspector Panel
  - Formula evaluation against selected object fields
  - Arithmetic, comparisons, logic, built-in functions (abs, ceil, floor, round, sqrt, pow, min, max, clamp)
  - Error/success display
- [x] **Kernel Wiring** — AutomationEngine, AutomationStore, ActionHandlerMap, graph analysis, expression evaluator
  - `useAutomation()`, `useGraphAnalysis()`, `useExpression()` hooks
  - Bus events → AutomationEngine for reactive triggers
- [x] **E2E Tests** (25 new tests) + existing updated for 9 lenses

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Playwright — Automation | 10 | Pass |
| Playwright — Analysis | 9 | Pass |
| Playwright — Expression | 6 | Pass |
| Playwright — Shell (updated) | 8 | Pass |
| Playwright — Tabs (updated) | 5 | Pass |
| Playwright — Keyboard (updated) | 5 | Pass |
| Vitest (unchanged) | 2123 | Pass |
| **Phase 30j Total** | **2123 Vitest + 145 E2E** | **All Pass** |

## Phase 30k: Studio Tier 4 — Plugins, Shortcuts, Vaults, Forms (Complete)

Wired four additional Layer 1 systems into Studio: PluginRegistry (extension management), InputRouter (keyboard shortcut management), VaultRoster (workspace discovery), and Forms & Validation (schema-driven form state).

### Completed

- [x] **Kernel wiring** — PluginRegistry, InputRouter with global scope (default bindings: cmd+z/shift+z/k/s/n), VaultRoster with MemoryRosterStore, FormState helpers (createFormState, setFieldValue, setFieldErrors, isDirty, fieldHasVisibleError)
- [x] **React hooks** — `usePlugins` (reactive plugin list + register/unregister), `useInputRouter` (bindings list + bind/unbind + recent events), `useVaultRoster` (reactive vault list + add/remove/pin/touch)
- [x] **Plugin Panel** (`plugin-panel.tsx`) — register/remove plugins, expand to see contributions, contributions tab showing all commands/views across plugins
- [x] **Shortcuts Panel** (`shortcuts-panel.tsx`) — view/add/remove keyboard bindings, scopes tab showing InputScope stack, events tab showing dispatched/pushed/popped/unhandled events
- [x] **Vault Panel** (`vault-panel.tsx`) — add/remove/pin/open vaults, search filtering, pinned section with star indicator, metadata display (path, dates, collections)
- [x] **Lens registration** — 3 new lenses: Plugins (p), Shortcuts (k), Vaults (w). Total: 12 lenses
- [x] **E2E tests** — 25 new Playwright tests across 3 spec files (plugin, shortcuts, vault)
- [x] **Shell test updated** — activity bar icon count 9 → 12

### Test Summary

| Suite | Count | Status |
|-------|-------|--------|
| Playwright — Plugin | 7 | Pass |
| Playwright — Shortcuts | 8 | Pass |
| Playwright — Vault | 9 | Pass |
| Playwright — Shell (updated) | 8 | Pass |
| Playwright — Tabs (updated) | 5 | Pass |
| Playwright — Keyboard (updated) | 5 | Pass |
| Vitest (unchanged) | 2132 | Pass |
| **Phase 30k Total** | **2132 Vitest + 170 E2E** | **All Pass** |

## Phase 30m: Studio Tier 5 — Identity, Assets, Trust (Complete)

Wired three sovereignty Layer 1 systems into Studio: Identity (W3C DID management), Virtual File System (content-addressed blob storage), and Trust & Safety (peer reputation, schema validation, Shamir recovery, escrow).

### Completed

- [x] **Kernel wiring** — createIdentity/signPayload/verifySignature/exportIdentity/importIdentity, VfsManager with MemoryVfsAdapter, PeerTrustGraph, SchemaValidator, LuauSandbox, ShamirSplitter, EscrowManager
- [x] **React hooks** — `useIdentity` (reactive identity + generate/export/import/sign/verify), `useVfs` (reactive locks + import/export/remove/lock/unlock), `useTrust` (reactive peers/flags + trust/distrust/ban/validate/sandbox/shamir/escrow)
- [x] **Identity Panel** (`identity-panel.tsx`) — generate DID, display DID/document/public key, sign & verify payloads, export/import JSON
- [x] **Assets Panel** (`assets-panel.tsx`) — import text files, browse blobs with hash/size/MIME, lock/unlock binary forking, remove files
- [x] **Trust Panel** (`trust-panel.tsx`) — 4-tab UI: Peers (add/trust/distrust/ban/unban with trust level badges), Validation (JSON schema validator), Flags (content hash flagging by category), Escrow (deposit/list encrypted key material)
- [x] **Lens registration** — 3 new lenses: Identity (i), Assets (f), Trust (t). Total: 15 lenses
- [x] **E2E tests** — 25 new Playwright tests across 3 spec files (identity, assets, trust)
- [x] **Kernel unit tests** — 22 new Vitest tests (identity: 6, VFS: 6, trust: 10)
- [x] **Shell test updated** — activity bar icon count 12 → 15

### Test Summary

| Suite | Count | Status |
|-------|-------|--------|
| Playwright — Identity | 8 | Pass |
| Playwright — Assets | 7 | Pass |
| Playwright — Trust | 10 | Pass |
| Playwright — Shell (updated) | 8 | Pass |
| Vitest (total) | 2170 | Pass |
| **Phase 30m Total** | **2170 Vitest + E2E** | **All Pass** |

## Phase 30n: Studio Tier 6 — Facets UI (Complete)

Wired Layer 1 facet engines (FacetParser, SpellEngine, Sequencer, ProseCodec, Emitters) into Studio kernel and built three new React panels for visual data projection and automation building.

### Completed

- [x] **Kernel wiring** — FacetParser (detect/parse/serialize/infer), SpellChecker (check/suggest with MockSpellCheckBackend + static dictionary), ProseCodec (markdownToNodes/nodesToMarkdown), Sequencer (emitConditionLuau/emitScriptLuau), Emitters (TS/JS/C#/Luau/JSON/YAML/TOML via SchemaModel), FacetDefinition registry (register/list/get/remove/builder)
- [x] **React hooks** — `useFacetParser` (detect/parse/serialize/infer), `useSpellCheck` (check/suggest), `useProseCodec` (md↔nodes), `useSequencer` (condition/script→Luau), `useEmitters` (multi-language codegen), `useFacetDefinitions` (reactive definition registry)
- [x] **Form Facet Panel** (`form-facet-panel.tsx`) — schema-driven form renderer: YAML/JSON source editor, auto-detected field types (text/number/boolean/email/tags/textarea), bidirectional source↔form sync, SpellEngine integration for text fields with inline error display
- [x] **Table Facet Panel** (`table-facet-panel.tsx`) — data grid: sortable columns (name/type/status/tags/position/updated), text filter + type dropdown filter, inline editing via double-click, keyboard navigation (arrow keys), row selection synced with kernel
- [x] **Sequencer Panel** (`sequencer-panel.tsx`) — visual automation builder: ConditionBuilder (combinator ALL/ANY, subject kind/operator/value dropdowns, add/remove clauses), ScriptBuilder (action steps with reorder/add/remove), live Luau preview, copy to clipboard
- [x] **Lens registration** — 3 new lenses: Form (d), Table (b), Sequencer (q). Total: 18 lenses
- [x] **Kernel unit tests** — 22 new tests: facet parser (7), spell checker (2), prose codec (2), sequencer (3), emitters (3), facet definitions (4), dispose (1)
- [x] **Shell test updated** — activity bar icon count 15 → 18

### Test Summary

| Suite | Count | Status |
|-------|-------|--------|
| Vitest — facet parser | 7 | Pass |
| Vitest — spell checker | 2 | Pass |
| Vitest — prose codec | 2 | Pass |
| Vitest — sequencer | 3 | Pass |
| Vitest — emitters | 3 | Pass |
| Vitest — facet definitions | 4 | Pass |
| Vitest (total) | 2622 | Pass |
| **Phase 30n Total** | **2622 Vitest + E2E** | **All Pass** |

## Phase 30o: Page Builder Centralization — Tiers 1-2 (Complete)

Connected Studio's page builder panels to the kernel so all state flows through one path (CollectionStore CRDT), not isolated silos.

### Completed

- [x] **1A: Puck ↔ Kernel Bridge** — Rewrote `layout-panel.tsx`:
  - Generates Puck Config dynamically from ObjectRegistry entity defs (component/section categories)
  - Projects kernel objects (page children) into Puck Data format
  - Diffs Puck onChange back into kernel CRUD (create/update/delete)
  - Removed isolated PuckLoroBridge; kernel CollectionStore is now the single source of truth
- [x] **1C: Graph Panel Live Data** — Rewrote `graph-panel.tsx`:
  - Subscribes to `kernel.store.onChange()` for live reactivity
  - Graph rebuilds automatically when objects are created, updated, or deleted
  - No longer snapshot-only at mount time
- [x] **2A: Drag-Drop in Explorer** — Added to `object-explorer.tsx`:
  - HTML5 drag-drop on tree nodes for reorder (above/below) and reparent (on)
  - Containment rule validation via `registry.canBeChildOf()`
  - Drop indicators (blue border top/bottom, highlight for reparent)
  - Automatic sibling position shifting on reorder
- [x] **2B: Component Palette** — New `component-palette.tsx`:
  - Lists all component/section entity types from ObjectRegistry
  - Grouped by category, searchable
  - Click to add as child of selected object (with containment validation)
  - Draggable items for drag-to-add
- [x] **2D: Block Toolbar on Canvas** — Added to `canvas-panel.tsx`:
  - Floating toolbar appears on selected blocks
  - Move up/down (swap with siblings), Duplicate, Delete actions
  - Toolbar positioned absolutely above the selected block
- [x] **2E: Quick-Create Combobox** — Added to `canvas-panel.tsx`:
  - "Add block" button at bottom of each section and page
  - Shows allowed child types from registry containment rules
  - Click to create and auto-select the new object
- [x] **Tests** — 7 new integration tests in `studio-kernel.test.ts`:
  - getAllowedChildTypes validation for page and section
  - Page→section→component hierarchy building
  - Child reorder via position update
  - Reparent via updateObject
  - Object duplication
  - Registry component list for Puck config generation
  - Delete and cleanup verification

| Artifact | Tests | Status |
| -------- | ----- | ------ |
| `layout-panel.tsx` (rewrite) | type-safe | Clean |
| `graph-panel.tsx` (rewrite) | type-safe | Clean |
| `object-explorer.tsx` (drag-drop) | type-safe | Clean |
| `component-palette.tsx` (new) | type-safe | Clean |
| `canvas-panel.tsx` (toolbar + quick-create) | type-safe | Clean |
| `studio-kernel.test.ts` (+7 tests) | 96 pass | All Pass |
| **Phase 30o Total** | **2651 Vitest** | **All Pass** |

## Phase 30l: WebRTC Signaling — All Relays (Complete)

WebRTC signaling as a standard relay module available to ALL relays, not deferred as Nexus-only. Enables P2P connection negotiation (SDP offer/answer, ICE candidates) through any relay.

### Completed

- [x] **SignalingHub capability** in `@prism/core/relay`
  - `RELAY_CAPABILITIES.SIGNALING` — new capability name
  - `webrtcSignalingModule()` — composable relay module (no dependencies)
  - SignalingHub: room management, peer join/leave, signal relay, empty room eviction
  - Types: `SignalMessage`, `SignalingPeer`, `SignalingRoom`, `SignalDelivery`, `SignalingHub`
- [x] **HTTP signaling routes** in `@prism/relay`
  - `GET /api/signaling/rooms` — list active rooms
  - `GET /api/signaling/rooms/:roomId/peers` — list peers
  - `POST /api/signaling/rooms/:roomId/join` — join room, receive existing peer list
  - `POST /api/signaling/rooms/:roomId/leave` — leave room, notify remaining peers
  - `POST /api/signaling/rooms/:roomId/signal` — relay offer/answer/ice-candidate
  - `POST /api/signaling/rooms/:roomId/poll` — poll buffered signals
- [x] **Integration tests** (15 new Vitest tests)
- [x] **E2E tests** (10 new Playwright tests)
- [x] **Module count** — 14 → 15 modules per relay

### Test Summary

| Suite | Count | Status |
|-------|-------|--------|
| Vitest — signaling routes | 15 | Pass |
| Playwright — signaling E2E | 10 | Pass |
| Vitest (total) | 2147 | Pass |
| Relay E2E (total) | 74 | Pass |
| **Phase 30l Total** | **2147 Vitest + 74 relay E2E + 170 studio E2E** | **All Pass** |

## Phase 31: Ecosystem Apps — Lattice

Game middleware suite: narrative, audio, entity authoring, world topology.

- [ ] **Lattice App** (`packages/prism-lattice/`)
  - [ ] **Loom** — Narrative engine: unified entry resolution, Fact Store (Ledger/Var), `.loom` format
  - [ ] **Canto** — Audio middleware: Sound Objects, signal graph, spatial audio, acoustic scenes
  - [ ] **Simulacra** — Entity authoring: game object system, `.sim` format, component slots, codegen
  - [ ] **Topology** — World navigation: scenes, regions, portals, state transitions
  - [ ] **Kami** — AI middleware: behavior trees, HSM, GOAP planners
  - [ ] **Cue** — Event orchestration: timeline editor, sync animations/dialogue/audio
  - [ ] **Meridian** — Stats: axes, pools, skill trees, conditions
  - [ ] **Palette** — Inventory: items, loot tables, equipment slots
  - [ ] **Boon** — Abilities: skills, cooldowns, activation rules

## Phase 30h: Relay CLI Hardening (Complete)

Production-readiness improvements to the Prism Relay CLI and server runtime.

### Completed
- [x] **CLI Subcommands** — `start`, `init`, `status`, `identity show/regenerate`, `modules list`, `config validate/show`
- [x] **`prism-relay init`** — scaffolds a starter config file per deployment mode
- [x] **`prism-relay status`** — queries `/api/health` on a running relay
- [x] **`prism-relay identity show/regenerate`** — inspect or rotate relay identity with backup
- [x] **`prism-relay modules list`** — lists all 15 modules with descriptions
- [x] **`prism-relay config validate`** — validates config (module names, federation, did:web, port range) without starting
- [x] **`prism-relay config show`** — prints fully resolved config with all defaults applied
- [x] **`bin` field + shebang** — package installable as global `prism-relay` command
- [x] **`/api/health` endpoint** — uptime, memory, peer count, federation peer count (200/503)
- [x] **Background eviction jobs** — mailbox envelope eviction, ACME challenge eviction, signaling room cleanup on configurable intervals
- [x] **Auto-save fix** — periodic persistence now saves unconditionally (dirty flag was never set)
- [x] **webrtc-signaling module** — 15th module wired into CLI factories and ALL_MODULES preset

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Parse Args | 36 | Pass |
| Status Routes (+ health) | 3 | Pass |
| Config | 11 | Pass |
| **All Vitest** | **2188** | **1 unrelated failure (Studio VFS)** |

## Phase 32: Facets — FileMaker Pro-Inspired Builder System

Visual projection + automation builder system. "Facet" = a face of a prism = a visual projection of collection data. Non-programmers can design forms, lists, tables, reports, scripts, and calculations without writing code.

### Architecture

Three tiers:
1. **Tier 1** (exists): ObjectRegistry, CollectionStore, ExpressionEngine, AutomationEngine, ViewConfig, Luau Runtime
2. **Tier 2** (Layer 1 engines): FacetParser, FacetSchema, SpellEngine, ProseCodec, Sequencer types, Emitters
3. **Tier 3** (Layer 2 React): FormFacet, TableFacet, ReportFacet, Sequencer UI, LuauFacet, FacetBuilders

### Naming Map (Legacy Helm → Prism Facets)

| Legacy | Prism | Purpose |
|--------|-------|---------|
| Document Surface modes | Facet (FormFacet, ListFacet, TableFacet, ReportFacet) | Visual projections |
| Wizards (Condition/Script) | Sequencer | Visual automation builder |
| Form Parser | FacetParser | YAML/JSON ↔ typed field records |
| Codegen Writers | Emitters (TS/JS/C#/Luau/JSON/YAML/TOML) | Schema → multi-language output |
| Spellcheck Engine | SpellEngine | Text quality across all facets |
| Luau View Renderer | LuauFacet | Custom facets authored in Luau |
| Shell Extension Builders | FacetBuilders | Luau codegen for standard patterns |
| Markdown Serializer | ProseCodec | MD ↔ structured content |

### Tier 2: Layer 1 Engines (`@prism/core/facet`)

- [x] **FacetParser** — port legacy `form-parser.ts`
  - [x] `detectFormat(value)` → 'yaml' | 'json'
  - [x] `parseValues(value, format)` → Record<string, unknown>
  - [x] `serializeValues(values, format, originalSource)` → string (preserves comments/ordering)
  - [x] `inferFields(values)` → FieldSchema[] (auto-detect types: boolean, number, url, email, date, textarea, tags)
- [x] **FacetSchema** — NEW: layout part definitions
  - [x] `FacetLayout` type (form, list, table, report, card)
  - [x] `LayoutPart` (header, body, footer, summary, leading-grand-summary, trailing-grand-summary)
  - [x] `FieldSlot` (field placement within a layout part: field ref, label position, width, validation display)
  - [x] `PortalSlot` (inline related records via EdgeTypeDef relationship)
  - [x] `FacetDefinition` (layout + parts + slots + scripts + title + description)
  - [x] `createFacetDefinition()` factory + `FacetDefinitionBuilder` fluent API
- [x] **SpellEngine** — port legacy `spellcheck/` (full engine, not just CM6 extension)
  - [x] `SpellCheckRegistry` (dictionary + filter registration, events)
  - [x] `SpellChecker` class (load dict, check text, suggest, personal dict)
  - [x] `PersonalDictionary` (persistent storage + session-only ignore)
  - [x] `SpellCheckerBuilder` fluent API
  - [x] 12 built-in TokenFilters (URL, email, allCaps, camelCase, filePath, inlineCode, wikiLink, etc.)
  - [x] Dictionary providers (URL, static, lazy, npm)
  - [x] `MockSpellCheckBackend` for tests
- [x] **ProseCodec** — port legacy `markdown-serializer.ts`
  - [x] `markdownToNodes(md)` → structured node tree (headings, paragraphs, lists, code blocks, blockquotes, HR)
  - [x] `nodesToMarkdown(nodes)` → string (round-trip preserving)
  - [x] Inline element support (bold, italic, code, links, wiki-links)
  - [x] Task list support (`- [ ]`, `- [x]`)
- [x] **Sequencer types** — port legacy wizard data model + Luau emission
  - [x] `SequencerSubject` (variable, field, event, custom — with id, label, type)
  - [x] `SequencerConditionState` (combinator: all|any, clauses with 12 operators)
  - [x] `SequencerScriptState` (steps: set-variable, add-variable, emit-event, call-function, custom)
  - [x] `emitConditionLuau(state)` → Luau expression string
  - [x] `emitScriptLuau(state)` → Luau statement block
- [x] **Emitters** — port legacy `codegen/writers/` (SchemaModel → multi-language)
  - [x] `SchemaModel` / `SchemaField` / `SchemaInterface` / `SchemaEnum` types
  - [x] `TypeScriptWriter` (interfaces + enums + JSDoc)
  - [x] `JavaScriptWriter` (JSDoc @typedef)
  - [x] `CSharpWriter` (classes + enums with namespace)
  - [x] `LuauWriter` (table + field definitions)
  - [x] `JsonWriter` (pretty-print serializer)
  - [x] `YamlWriter` (zero-dep: scalars, blocks, anchors)
  - [x] `TomlWriter` (zero-dep: tables, array-of-tables)

### Tier 3: Layer 2 React Components

- [x] **FormFacet** — schema-driven field renderer (replaces document-surface form stub)
  - [x] Render FieldSchema[] → form fields with validation
  - [x] FacetParser integration (YAML/JSON source ↔ form state)
  - [x] SpellEngine integration for text/textarea fields
  - [x] PortalSlot rendering (inline related records)
  - [x] Conditional field visibility
- [x] **TableFacet** — data grid (replaces document-surface spreadsheet stub)
  - [x] Column definitions from EntityFieldDef
  - [x] Inline editing
  - [x] Sort/filter/group headers
  - [x] Keyboard navigation (arrow keys, tab, enter)
- [x] **ReportFacet** — grouped/summarized view (replaces document-surface report stub)
  - [x] LayoutPart rendering (header/body/footer/summary)
  - [x] Sub-summary groups by field
  - [x] Expression evaluation for summary fields (count, sum, avg)
- [x] **Sequencer UI** — visual automation builder
  - [x] ConditionBuilder (dropdowns for subject → operator → value)
  - [x] ScriptBuilder (step list with add/remove/reorder)
  - [x] Live Luau preview
  - [x] Integration with AutomationEngine
- [x] **LuauFacet** — execute Luau render scripts → React
  - [x] `ui` builder table (label, button, section, badge, input, row, column, spacer, divider)
  - [x] `ctx` context object (viewId, instanceKey, isActive)
  - [x] Error states (no VM, execution error, null return)
  - [x] LuauRuntimeProvider context integration
- [x] **FacetBuilders** — Luau codegen for common shell patterns
  - [x] `luauBrowserView()` — generate Luau for a collection browser view
  - [x] `luauCollectionRule()` — generate Luau for a validation rule
  - [x] `luauStatsCommand()` — generate Luau for a summary command
  - [x] `luauMenuItem()` — generate Luau for a menu contribution
  - [x] `luauCommand()` — generate Luau for a keyboard command

### Integration with Studio

- [x] **Facet lenses** — Form, Table, Report, Sequencer, Luau Facet (20 total lenses, "facet" category)
- [x] **Studio kernel wiring** — FacetParser, SpellEngine, ProseCodec, Sequencer, Emitters, FacetDefinitions all wired
- [x] **Kernel hooks** — useFacetParser, useSpellCheck, useProseCodec, useSequencer, useEmitters, useFacetDefinitions
- [x] **Facet Designer lens** — visual layout builder (like FileMaker Layout Mode)
- [x] **Record Browser** — form/list/table/report/card toggle per collection (like FileMaker Browse Mode)

## Phase 34: Relay Production Readiness (Complete)

Full test coverage, CLI expansion, Studio integration, and documentation for Prism Relay.

### Completed

- [x] **Unit test gaps filled** — 9 new test files: file-store, logger, presence-store, push-transport, collection-routes, escrow-routes, hashcash-routes, trust-routes, presence-routes (69 new tests, 2828 total)
- [x] **CLI management commands** — 22 new subcommands: peers (list/ban/unban), collections (list/inspect/export/import/delete), portals (list/inspect/delete), webhooks (list/delete/test), tokens (list/revoke), certs (list/renew), backup, restore, logs
- [x] **New API endpoints** — GET/POST /api/backup, GET/DELETE /api/logs, GET /api/tokens (list), POST /api/webhooks/:id/test, DELETE /api/collections/:id
- [x] **Studio relay integration** — RelayManager expanded with 13 new methods (collections, webhooks, peers, certs, backup/restore, health, discovery). RelayPanel expanded with 7 new management sections (health, collections, federation, webhooks, certificates, backup/restore) + relay auto-discovery
- [x] **Documentation** — docs/deployment.md (Docker, TLS, federation, monitoring, security), docs/development.md (architecture, modules, testing, contributing), updated README.md and CLAUDE.md
- [x] **Full CLI E2E test** — 44 commands tested against running relay, all passing including error cases

## Phase 35: Relay Deployment Infrastructure (Complete)

All deployment options fully developed, documented, and tested.

### Completed

- [x] **Dockerfile hardened** — multi-stage build with non-root `prism` user, built-in HEALTHCHECK, VOLUME for persistent data, `pnpm prune --prod` for slim production image
- [x] **.dockerignore** — excludes node_modules, dist, tests, legacy packages, .git
- [x] **docker-compose.yml** — single-relay deployment with health checks, volumes, env overrides
- [x] **docker-compose.federation.yml** — two-relay federated mesh with shared network, dependency ordering, separate volumes
- [x] **.env.example** — environment variable template documenting all 6 PRISM_RELAY_* vars
- [x] **Deployment E2E tests** — 37 tests (34 run, 3 skip when Docker unavailable): Dockerfile structure, Docker Compose validation, config system, health check contract, CSRF enforcement, CORS behavior, backup/restore API round-trip, graceful shutdown + state persistence, SEO endpoints, rate limiting, identity persistence, multi-mode startup, WebSocket connectivity
- [x] **Pre-existing test fix** — "many collections" resource exhaustion test fixed (rate-limit retry on GET /api/collections)
- [x] **Deployment docs updated** — references actual file paths, Docker user/volume changes, federation compose, .env.example

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (all) | 3006 | Pass |
| Playwright (relay) | 87 | Pass |
| Playwright (production-readiness) | 48 | Pass |
| Playwright (deployment) | 34 | Pass |
| **Playwright (relay total)** | **169** | **Pass** |

## Phase 33: Ecosystem Apps — Cadence & Grip

Music education and live production.

- [ ] **Cadence App** (`packages/prism-cadence/`)
  - [ ] LMS: courses, lessons, assignments, student progress
  - [ ] Interactive music lessons with CRDT-synced notation
  - [ ] Practice tracking and repertoire management
- [ ] **Grip App** (`packages/prism-grip/`)
  - [ ] 3D stage plot editor (Builder 3 + venue templates)
  - [ ] NLE timeline for show programming (Phase 28 integration)
  - [ ] Hardware control dashboard: DMX, PTZ, audio mixer
  - [ ] Show runtime: cue-to-cue execution with hardware sync
