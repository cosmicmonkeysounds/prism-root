# Flux Port Gap Analysis

> What's missing from the Rust Prism codebase to reach feature parity
> with the legacy Helm core + Flux app (React/TypeScript).
>
> **Naming:** Helm → Prism, Flux → Flux (unchanged).

**Created:** 2026-05-01
**Last updated:** 2026-05-02

---

## Ported (solid coverage)

The Rust port covers the **infrastructure layer** comprehensively:

- **Object model** — registry, types, edges, trees, NSID, weak refs, context engine
- **Store / Atom / State machine** — zustand replacement, reactive cells, flat FSM
- **CRDT sync** — bidirectional Loro ↔ Atom bridge
- **Plugin system** — registry, contributions, 6 built-in bundles
- **Automation engine** — trigger/condition/action, 30+ action types
- **Config** — layered cascade, feature flags, JSON Schema validator
- **Identity** — DID (Ed25519), AES-GCM-256 encryption, manifest, trust (Luau sandbox, hashcash, Shamir, escrow, PBKDF2)
- **Language** — syntax/expression/forms/luau/visual/slint_lang/markdown/codegen
- **Notification / Activity / Query** — registry, debounced queue, activity log, filter/sort/group pipeline
- **Network** — 17 relay modules, presence, discovery, session, server, relay manager
- **Domain: Flux** — 11 entity types, 7 edges, 8 automation presets, CSV/JSON import-export
- **Domain: Timeline** — NLE engine, TempoMap, transport/track/clip/automation CRUD
- **Domain: Graph Analysis** — topological sort, cycle detection, CPM critical path
- **Foundation** — VFS, template, clipboard, undo, batch, date, geometry, spatial, persistence
- **Builder** — 16 Slint components, layout (Taffy), signals, facets, live document, source map, dual render (Slint + HTML SSR)
- **Shell** — 7 panels (Identity, Builder, Inspector, Properties, Signals, Navigation, CodeEditor), command palette, search (TF-IDF), input manager, dock layout, e2e testing
- **Daemon** — 13 modules (CRDT, Luau, VFS, crypto, actors, whisper, conferencing, etc.), 5 transports
- **Relay** — full SSR + 18-module relay protocol, ~80 HTTP endpoints

Entity defs are seeded across 6 plugin bundles:
- **work:** gig, time-entry, focus-block
- **life:** habit, habit-log, fitness-log, sleep-record, journal-entry, meal-plan, cycle-entry
- **assets:** media-asset, content-item, scanned-doc, collection
- **finance:** loan, grant, budget
- **platform:** calendar-event, message, reminder, feed, feed-item
- **CRM:** views/commands on existing flux contact/organization types

---

## Tier 1 — Core Engines

These are prerequisite engines that multiple Flux panels depend on.
Without them, Flux is a builder IDE, not a productivity app.

### 1.1 Calendar Engine

**Legacy:** `@core/calendar` — `CalendarEvent`, RRULE recurrence
expansion, `queryCalendar`, range helpers.

**Rust target:** `prism_core::domain::calendar`

**Scope:**
- `CalendarEvent` struct (title, start, end, all_day, recurrence_rule, reminders)
- RRULE parser + occurrence expander (RFC 5545 subset)
- `queryCalendar(events, range) -> Vec<CalendarOccurrence>` — expand recurrences within a date range
- Range helpers: `events_on_date`, `events_in_range`, `next_occurrence`
- Integration point: consumes any `GraphObject` with date fields

**External crate:** `rrule` (RFC 5545 RRULE parsing + expansion, built on `chrono`).
Falls back to hand-rolled if the crate is too heavy — the legacy only
used a subset (FREQ, INTERVAL, COUNT, UNTIL, BYDAY, BYMONTH, BYMONTHDAY).

**Depends on:** `chrono` (workspace), `foundation::object_model`

**Blocks:** Calendar panel, Day Planner panel, Reminders engine

### 1.2 Timekeeping Engine

**Legacy:** `@core/timekeeping` — Stopwatch FSM, `TimeEntry`,
`TimerSnapshot`; React hooks via `@core/timekeeping/react`.

**Rust target:** `prism_core::domain::timekeeping`

**Scope:**
- `StopwatchState` FSM: Idle → Running → Paused → Idle (with lap support)
- `TimeEntry` struct (object_id, start, end, duration_ms, tags, notes)
- `TimerSnapshot` for serialization across hot-reload
- `TimekeepingStore` — CRUD for time entries, query by object/date range
- Aggregation: `total_time_for_object`, `time_by_date`, `time_by_tag`

**External crate:** None — pure state machine + `chrono` arithmetic.

**Depends on:** `chrono` (workspace), `kernel::state_machine`

**Blocks:** Time Tracking panel, Focus panel, Global Timer widget

### 1.3 Comments Engine

**Legacy:** `@core/comments` — `CommentStore`, `buildThreads`,
reactions, soft-delete.

**Rust target:** `prism_core::interaction::comments`

**Scope:**
- `Comment` struct (id, object_id, parent_comment_id, author_id, body, created_at, edited_at, deleted_at)
- `Reaction` struct (comment_id, author_id, emoji)
- `CommentStore` — add/edit/soft-delete/restore, add/remove reactions
- `build_threads(comments) -> Vec<CommentThread>` — nest by parent_comment_id
- Subscription bus for real-time updates

**External crate:** None — pure data + tree building.

**Depends on:** `chrono`, `uuid`, `indexmap`

**Blocks:** Object detail view (comments tab), collaboration features

### 1.4 Dashboard Engine

**Legacy:** `@core/dashboard` — `DashboardController`, `WidgetLayout`
(grid math), default presets.

**Rust target:** `prism_core::interaction::dashboard` -- build this on top of the Slint systems already in place in Prism.

**Scope:**
- `Dashboard` struct (id, title, tabs: Vec<DashboardTab>)
- `DashboardTab` struct (id, label, widgets: Vec<WidgetInstance>)
- `WidgetInstance` struct (id, widget_type, position: GridPosition, size: GridSize, config: Value)
- `WidgetLayout` — grid placement math (collision detection, auto-place, resize constraints)
- `DashboardController` — tab CRUD, widget CRUD, layout mutations
- Default presets: home, work, life, assets, platform

**External crate:** None — grid math is self-contained. Taffy (already
in workspace) could be reused for more complex layouts, but the legacy
used a simpler fixed-column grid.

**Depends on:** `serde`, `serde_json`, `uuid`, `indexmap`

**Blocks:** Home Dashboard, Work/Life/Assets/Platform dashboards

### 1.5 Ledger / Finance Engine

**Legacy:** `@core/ledger` — `formatCurrency`, `LineItem`,
`calcLineTotals`, `amortize`, TVM functions (fv, pv, pmt, npv, irr).

**Rust target:** `prism_core::domain::ledger`

**Scope:**
- `Money` type (i64 cents + currency code) — no floating-point
- `LineItem` struct (description, quantity, unit_price, tax_rate, discount)
- `calc_line_totals(items) -> LineTotals` (subtotal, tax, discount, total)
- `format_currency(cents, currency) -> String`
- TVM functions: `future_value`, `present_value`, `payment`, `npv`, `irr`
- `amortize(principal, rate, periods) -> Vec<AmortizationRow>`
- Budget tracking: `BudgetPeriod`, `budget_vs_actual`, `categorize_expense`

**External crate:** None for TVM (pure math). Consider `rust_decimal`
for precise decimal arithmetic if cent-based integers prove limiting.

**Depends on:** `serde`, `chrono`

**Blocks:** Finance Hub panel, invoice generation, budget dashboards

### 1.6 Spreadsheet Engine

**Legacy:** `@core/spreadsheet` — `SpreadsheetData`, `ColumnDef`,
`FormulaEngine` (HyperFormula optional), clipboard TSV.

**Rust target:** `prism_core::domain::spreadsheet` -- and again, use the Slint builder / facet systems already in Prism to build the UI for spreadsheets.

**Scope:**
- `SpreadsheetData` struct (columns: Vec<ColumnDef>, rows: Vec<Row>)
- `ColumnDef` struct (id, label, field_type, width, formula)
- `FormulaEngine` — evaluate cell formulas (SUM, AVERAGE, COUNT, MIN, MAX, IF, VLOOKUP, CONCATENATE)
- Cell reference parsing (A1 notation + ranges)
- Clipboard: copy/paste as TSV
- Sort/filter integration with `interaction::query`

**External crate:** Roll our own — no mature Rust spreadsheet formula
engine exists. The legacy used HyperFormula optionally; for Prism we
build a focused subset (the `language::expression` evaluator already
handles most of the math). Consider extending `language::expression`
with cell-reference resolution rather than building a parallel engine.

**Depends on:** `language::expression`, `serde`, `indexmap`

**Blocks:** Spreadsheet view component, Table view with formulas

---

## Tier 2 — Domain Module Engines ✅

Entity defs exist in plugin bundles. These engines add the
**behavioral logic** that makes the entities useful.

**Status:** All 7 core engines ported (2026-05-02). Finance module
engine (2.8) deferred — extends `domain::ledger` when needed.

### 2.1 CRM Engine ✅
**Legacy:** `@helm/crm` — deal pipeline stages, contract lifecycle,
e-signature tracking, deal→project bridge, client profitability.

**Rust target:** `prism_core::domain::crm`

**Scope:** `DealPipeline` (stage progression, win/loss, probability),
`ContractLifecycle` (draft → sent → signed → active → expired),
`ClientProfitability` (revenue - costs by client), bridge functions
(`deal_to_project_seed`, `contract_to_kickoff_tasks`).

### 2.2 Projects Engine ✅
**Legacy:** `@helm/projects` — risk register, scope tracker, velocity,
project health metrics, burndown data.

**Rust target:** `prism_core::domain::projects`

**Scope:** `RiskRegister` (impact × probability scoring, mitigation),
`ScopeTracker` (scope changes, creep alerts), `VelocityCalculator`
(points/tasks per sprint), `ProjectHealth` (composite score from
schedule/scope/risk), burndown/burnup data series generation.

**Note:** CPM critical path already exists in `domain::graph_analysis`.

### 2.3 Habits Engine ✅
**Legacy:** `@helm/habits` — streak computation, completion rates,
wellness summary + composite score.

**Rust target:** `prism_core::domain::habits`

**Scope:** `compute_streak(logs) -> StreakInfo` (current, longest, gaps),
`completion_rate(logs, period) -> f64`, `wellness_summary(habits) ->
WellnessSummary` (composite score across habits, sleep, fitness).

### 2.4 Fitness Engine ✅
**Legacy:** `@helm/fitness` — MET-based calorie estimation, personal
bests tracking.

**Rust target:** `prism_core::domain::fitness`

**Scope:** `estimate_calories(activity, duration, weight, met_value)`,
`PersonalBests` tracker (per exercise, all-time and period-best).

### 2.5 Goals Engine ✅
**Legacy:** `@helm/goals` — goal hierarchy, milestone tracking,
progress computation.

**Rust target:** `prism_core::domain::goals`

**Scope:** `compute_goal_progress(goal, milestones) -> f64`, hierarchy
traversal (parent goals roll up child progress), milestone status
tracking, deadline proximity alerts.

### 2.6 Reminders Engine ✅
**Legacy:** `@helm/reminders` — `computeNextOccurrence`, recurrence
scheduling, delivery, `buildNotificationPayload`.

**Rust target:** `prism_core::domain::reminders`

**Scope:** `compute_next_occurrence(reminder) -> Option<DateTime>`,
`build_notification_payload(reminder) -> NotificationPayload`,
snooze support, overdue detection.

**Depends on:** Calendar engine (Tier 1.1) for RRULE expansion.

### 2.7 Focus Planner Engine ✅
**Legacy:** `@helm/focus-planner` — daily context engine, planning
methods (MIT/3-things/duck), check-ins, brain dump.

**Rust target:** `prism_core::domain::focus_planner`

**Scope:** `DailyPlan` (date, method, items, check_ins),
`PlanningMethod` enum, `generate_daily_context(objects, date)` (pull
today's tasks + events + habits + reminders into one view).

### 2.8 Finance Module Engine
**Legacy:** `@helm/finance` — budget tracking, expense categorization,
financial reporting.

**Rust target:** Extends `domain::ledger` (Tier 1.5)

**Scope:** `BudgetTracker` (budget vs actual by category/period),
`ExpenseCategorizer` (rule-based auto-categorization),
`FinancialReport` (income/expense summary, trends).

---

## Tier 3 — Unified Messaging

### 3.1 Inbox Engine
**Legacy:** `@core/inbox` — `InboxMessage`, `InboxProvider`,
`InboxProviderRegistry`, `DraftStore`, `buildThreads`.

**Rust target:** `prism_core::interaction::inbox`

**Scope:**
- `InboxMessage` struct (id, provider, thread_id, from, to, subject, body, timestamp, read, starred, labels)
- `InboxProvider` trait (fetch, send, mark_read, search)
- `InboxProviderRegistry` — register/unregister/list providers
- `DraftStore` — draft CRUD, auto-save
- `build_threads(messages) -> Vec<MessageThread>` — group by thread_id + sort

### 3.2 Messaging Providers (9 total)
Each implements `InboxProvider`:
- Gmail (Google APIs OAuth2)
- Outlook (Microsoft Graph API)
- Discord (REST v10)
- Slack (@slack/web-api equivalent)
- Telegram (Bot API)
- Messenger (Meta Graph API)
- Instagram (Meta Graph API)
- SMS (Twilio)
- iMessage (BlueBubbles)

**Priority:** Gmail + Slack first, others follow the same pattern.

---

## Tier 4 — View Components ✅

Builder `Component` implementations for data visualization.
These are `prism_builder` components registered via `ComponentRegistry`.

**Status:** 7 of 9 view components implemented (2026-05-02). Kanban,
Calendar View, Gantt, Gallery, Inbox View, and Timeline View as
`WidgetContribution` widgets; Graph View as a raw builder `Component`.
Spreadsheet View and Chart components deferred (low priority).

| View | Legacy | Priority |
|---|---|---|
| **Kanban** ✅ | Column-based boards with drag-drop | High |
| **Calendar View** ✅ | Week/month/day with event rendering | High |
| **Gantt** ✅ | Timeline bars with dependency arrows | Medium |
| **Graph View** ✅ | Object relationship visualization | Medium |
| **Gallery** ✅ | Image/card grid with lightbox | Medium |
| **Inbox View** ✅ | Threaded message list | Medium |
| **Timeline View** ✅ | Chronological event stream | Low |
| **Spreadsheet View** | Formula-capable grid | Low |
| **Chart components** | Burndown, burnup, velocity, budget, spending | Low |

---

## Tier 5 — Flux App Panels

Slint panel implementations in `prism-shell`. Each panel renders into
the dock layout and consumes one or more Tier 1–2 engines.

| Panel | Engine Dependencies | Priority |
|---|---|---|
| **Home Dashboard** | Dashboard (1.4) | High |
| **Tasks** | Flux entity defs, Query | High |
| **Calendar** | Calendar (1.1) | High |
| **Time Tracking** | Timekeeping (1.2) | High |
| **Focus** | Timekeeping (1.2), Focus Planner (2.7) | Medium |
| **Capture** | Flux entity defs | Medium |
| **Day Planner** | Calendar (1.1), Focus Planner (2.7) | Medium |
| **Finance Hub** | Ledger (1.5) | Medium |
| **Work Dashboard** | Dashboard (1.4), CRM (2.1), Projects (2.2) | Medium |
| **Life Dashboard** | Dashboard (1.4), Habits (2.3), Goals (2.5) | Medium |
| **Assets Dashboard** | Dashboard (1.4) | Low |
| **Platform Dashboard** | Dashboard (1.4), Inbox (3.1) | Low |
| **Settings** | Config (existing) | Low |
| **Automation** | Automation engine (existing) | Low |
| **Object Detail View** | Comments (1.3), Activity (existing) | High |

---

## Tier 6 — Flux App Wiring

| Feature | Description | Priority |
|---|---|---|
| **flux-bus** ✅ | Cross-module event bus: deal won → project + invoice, contract signed → kickoff tasks, task complete → update progress, invoice overdue → reminder | High |
| **Object Detail View** ✅ | Tabbed detail surface (overview, relations, notes, time logs) | High |
| **Quick Create** ✅ | Cmd+N rapid object creation by entity type | High |
| **Global Timer** | Persistent toolbar timer (focus/time tracking sessions) | Medium |
| **Inbox Tray** | Notification + reminder + task-due dropdown | Medium |
| **Activity Feed** | Chronological feed widget for dashboards | Medium |
| **Seed Data** ✅ | 50+ sample objects for dev/demo mode | Medium |
| **Routine Registry** ✅ | 6 named routines for command palette | Low |

---

## Tier 7 — Processing

| Feature | Legacy | Priority |
|---|---|---|
| **OCR** | `@core/ocr` — amount/date/line-item extraction | Low |
| **Document Processor** | `@core/document-processor` — 10-state FSM, classifier | Low |
| **PDF** | `@core/pdf` — invoice/text document generation | Low |
| **Media Processing** | `@core/media` — compress/resize/thumbnail | Low |
| **Export expansion** | Markdown/HTML/TSV export (currently CSV/JSON only) | Low |
| **Logger** | `@core/logger` — ring-buffer structured logging with sinks | Low |

---

## Tier 8 — External Integrations

| Integration | Legacy | Priority |
|---|---|---|
| **Google Calendar** | OAuth2, event sync | Medium |
| **Microsoft Calendar** | Graph API, event sync | Low |
| **Stripe** | Payment processing | Low |
| **Weather** | Open-Meteo (free, no key) | Low |
| **News** | RSS + GDELT, bias registry, digest scheduling | Low |

---

## External Crate Candidates

| Need | Crate | Version | Downloads | Decision |
|---|---|---|---|---|
| RRULE expansion | `rrule` | 0.14.0 | 590k | **Use.** RFC 5545 parser + expander, built on `chrono` + `chrono-tz`. |
| Decimal math | `rust_decimal` | 1.41 | 95M | **Use.** 96-bit integer decimal, serde, no float rounding. Standard for money in Rust. |
| Currency format | `rusty-money` | 0.5.0 | 721k | **Optional.** Wraps `rust_decimal` with ISO 4217 codes + locale formatting. |
| TVM functions | `financial` | 1.1.5 | 35k | **Use.** Excel-compatible fv/pv/pmt/npv/irr. 180+ test cases. Math is settled. |
| Amortization | — | — | — | **Roll own** (~30 LOC on top of `financial`). |
| Formula eval | — | — | — | **Roll own.** Extend `language::expression` with cell refs + aggregates. No good Rust crate. |
| Grid layout | `taffy` | 0.7 | — | **Reuse.** Already in workspace. CSS Grid for dashboard widget placement. |
| Date i18n | `icu_datetime` | 2.2.0 | 5.1M | **Consider.** Unicode Consortium's official Rust CLDR impl. Heavy — defer unless needed. |
| Relative time | — | — | — | **Roll own** (~40 LOC threshold logic on `chrono::Duration`). |
| Comment threading | — | — | — | **Roll own** (~150 LOC tree building). |

---

## Recommended Implementation Order

```
Phase A (Tier 1 — core engines, unblocks everything): ✅
  1. Calendar engine (RRULE, occurrence expansion) ✅
  2. Timekeeping engine (stopwatch FSM, time entries) ✅
  3. Comments engine (threads, reactions) ✅
  4. Dashboard engine (widget grid, presets) ✅
  5. Ledger engine (money, TVM, line items) ✅
  6. Spreadsheet engine (formulas, cell refs) ✅

Phase B (Tier 2 + 6 — domain logic + Flux wiring): ✅
  7. Habits / Goals / Fitness / Reminders engines ✅
  8. CRM / Projects engines ✅
  9. Focus Planner engine ✅
 10. flux-bus (cross-module event wiring) ✅
 11. Object Detail View ✅
 12. Quick Create ✅
 12b. Seed Data + Routine Registry ✅

Phase C (Tier 4 + 5 — UI):
 13. View components (Kanban, Calendar, Gantt, Graph, Gallery, Inbox, Timeline) ✅
 14. Flux panels (Home, Tasks, Calendar, Time Tracking)
 15. Remaining panels

Phase D (Tier 3, 7, 8 — integrations):
 16. Inbox engine + Gmail/Slack providers
 17. Processing (OCR, PDF, media)
 18. External integrations
```
