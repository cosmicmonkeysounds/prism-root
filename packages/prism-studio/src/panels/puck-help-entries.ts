import { HelpRegistry, type HelpEntry } from "@prism/core/help";

/**
 * Help entries for the Puck layout panel.
 *
 * Importing this module registers every entry as a side-effect. Studio's
 * `layout-panel.tsx` imports it once at startup and the entries then flow
 * into HelpTooltip / DocSearch / DocSheet throughout the editor.
 *
 * Id convention:
 *
 *   puck.categories.<kebab>        palette category headers
 *   puck.components.<kebab-type>   individual entity types
 *   puck.fields.<kebab>            style-field groups shared across blocks
 *   puck.regions.<slot>            shell slots (header / sidebar / main / footer)
 *
 * Twelve flagship components ship with full markdown docs under
 * `./help-docs/` and point at them via `docPath`. The remaining entries
 * are summary-only — hover works, but "View full docs" is hidden because
 * `docPath` is unset. Add more markdown files as the help surface grows.
 */

// ── Categories ──────────────────────────────────────────────────────────────

const CATEGORY_ENTRIES: HelpEntry[] = [
  {
    id: "puck.categories.layout",
    title: "Layout",
    summary:
      "Structural blocks that frame content: sections, columns, shells, and spacing primitives.",
  },
  {
    id: "puck.categories.content",
    title: "Content",
    summary:
      "Static content blocks: headings, paragraphs, images, buttons, cards, and media.",
  },
  {
    id: "puck.categories.data",
    title: "Data",
    summary:
      "Live data views over kernel records: lists, tables, kanban boards, reports, charts, and maps.",
  },
  {
    id: "puck.categories.form",
    title: "Form",
    summary:
      "Input primitives: text, textarea, select, checkbox, number, and date fields for building forms.",
  },
  {
    id: "puck.categories.records",
    title: "Records",
    summary:
      "Domain-specific widgets for tasks, reminders, contacts, events, notes, goals, habits, and more.",
  },
  {
    id: "puck.categories.dynamic",
    title: "Dynamic",
    summary:
      "Scripted blocks that render themselves at runtime: Luau UI, Facet Views, and Spatial Canvases.",
  },
  {
    id: "puck.categories.app",
    title: "App",
    summary:
      "App-level structure: the app itself, routes, behaviors, and the outer app shell.",
  },
];

// ── Components (12 flagship + summary stubs) ────────────────────────────────

const FLAGSHIP_ENTRIES: HelpEntry[] = [
  {
    id: "puck.components.page-shell",
    title: "Page Shell",
    summary:
      "Outer frame for a single page. Defines header / sidebar / main / footer regions that resize from their shared edge.",
    docPath: "page-shell",
  },
  {
    id: "puck.components.app-shell",
    title: "App Shell",
    summary:
      "Outer frame for a whole multi-page app. Persistent side-nav and header stay mounted as users navigate between pages.",
    docPath: "app-shell",
  },
  {
    id: "puck.components.site-header",
    title: "Site Header",
    summary:
      "Standalone masthead block with logo, nav links, and optional sticky behaviour. Use this on marketing pages.",
    docPath: "site-header",
  },
  {
    id: "puck.components.site-footer",
    title: "Site Footer",
    summary:
      "Bottom-of-page footer with columns of nav links, social icons, and copyright text.",
    docPath: "site-footer",
  },
  {
    id: "puck.components.section",
    title: "Section",
    summary:
      "Horizontal band that groups related content. Sections stack vertically; every page block lives inside one.",
    docPath: "section",
  },
  {
    id: "puck.components.columns",
    title: "Columns",
    summary:
      "Lays out its children horizontally in a 1–6 column grid with configurable gap and responsive collapse.",
    docPath: "columns",
  },
  {
    id: "puck.components.heading",
    title: "Heading",
    summary:
      "Renders an h1–h6 with inline editing, alignment, color, and weight overrides. Anchors the page outline.",
    docPath: "heading",
  },
  {
    id: "puck.components.image",
    title: "Image",
    summary:
      "Renders an <img> with configurable source, alt text, fit, aspect ratio, and rounding. Supports vault-managed blobs.",
    docPath: "image",
  },
  {
    id: "puck.components.facet-view",
    title: "Facet View",
    summary:
      "Embeds a FacetDefinition as a form / list / table / report / card grid. Schema-driven data UI without duplicate configs.",
    docPath: "facet-view",
  },
  {
    id: "puck.components.luau-block",
    title: "Luau Block",
    summary:
      "Embeds a Luau script that renders its own UI via `ui.*` calls. Runs in Prism's sandboxed browser runtime.",
    docPath: "luau-block",
  },
  {
    id: "puck.components.record-list",
    title: "Record List",
    summary:
      "Queries kernel records by type and renders them as rows. Parametric filter / sort / limit via a compact expression grammar.",
    docPath: "record-list",
  },
  {
    id: "puck.components.spatial-canvas",
    title: "Spatial Canvas",
    summary:
      "Free-form absolute-positioning surface with drag / resize / rotate / snap. For diagrams and pixel-perfect layouts.",
    docPath: "spatial-canvas",
  },
];

const STUB_ENTRIES: HelpEntry[] = [
  // Structural / layout primitives
  {
    id: "puck.components.folder",
    title: "Folder",
    summary:
      "Workspace-level container for grouping pages. Not a renderable block — it only exists in the object explorer.",
  },
  {
    id: "puck.components.page",
    title: "Page",
    summary:
      "A single page at the workspace root. Contains sections and blocks and has its own URL slug.",
  },
  {
    id: "puck.components.text-block",
    title: "Text Block",
    summary:
      "A paragraph of plain or markdown-formatted text. Use for body copy between headings.",
  },
  {
    id: "puck.components.button",
    title: "Button",
    summary:
      "Real button or link with variants, sizes, icons, loading state, and a full styling palette.",
  },
  {
    id: "puck.components.card",
    title: "Card",
    summary:
      "Bordered content card with optional media, title, body, and call-to-action slots.",
  },
  {
    id: "puck.components.data-portal",
    title: "Data Portal",
    summary:
      "Inline table of related records resolved via edge relationships from the host object.",
  },
  {
    id: "puck.components.site-nav",
    title: "Site Nav",
    summary:
      "Auto-generated multi-page navigation driven by every page object in the vault. Edit via the Site Nav lens (Shift+N).",
  },
  {
    id: "puck.components.nav-bar",
    title: "Nav Bar",
    summary:
      "Simple horizontal navigation bar with configurable items, alignment, and variant.",
  },
  {
    id: "puck.components.side-bar",
    title: "Side Bar",
    summary:
      "Vertical navigation rail with collapsible groups of links. Pair with a Page Shell or App Shell.",
  },
  {
    id: "puck.components.hero",
    title: "Hero",
    summary:
      "Full-width headline band with headline, subhead, optional image or video, and call-to-action button.",
  },
  {
    id: "puck.components.divider",
    title: "Divider",
    summary:
      "Horizontal rule separating sections. Supports optional centered label text.",
  },
  {
    id: "puck.components.spacer",
    title: "Spacer",
    summary:
      "Empty vertical or horizontal space. Prefer padding on containers when possible.",
  },
  {
    id: "puck.components.tab-container",
    title: "Tab Container",
    summary:
      "Renders its children as switchable tabs. Each child appears as one tab panel.",
  },
  {
    id: "puck.components.popover-widget",
    title: "Popover",
    summary:
      "Anchor element that reveals a floating panel of content on click or hover.",
  },
  {
    id: "puck.components.slide-panel",
    title: "Slide Panel",
    summary:
      "Slide-in panel triggered from a button. Use for settings, filters, or detail views.",
  },
  {
    id: "puck.components.breadcrumbs",
    title: "Breadcrumbs",
    summary:
      "Auto-generated breadcrumb trail from the page's position in the site nav tree.",
  },

  // Content primitives
  {
    id: "puck.components.markdown-widget",
    title: "Markdown",
    summary:
      "Renders a markdown string as HTML. Safe — raw HTML is escaped.",
  },
  {
    id: "puck.components.iframe-widget",
    title: "Iframe",
    summary:
      "Embeds an http(s) URL in a sandboxed iframe. Restricted to http/https — no javascript: or data: URLs.",
  },
  {
    id: "puck.components.code-block",
    title: "Code Block",
    summary:
      "Static <pre><code> block with language label, line numbers, and wrap toggle.",
  },
  {
    id: "puck.components.video-widget",
    title: "Video",
    summary:
      "HTML5 video player with configurable source, controls, autoplay, and poster image. http(s) only.",
  },
  {
    id: "puck.components.audio-widget",
    title: "Audio",
    summary:
      "HTML5 audio player with configurable source and controls. http(s) only.",
  },

  // Data display
  {
    id: "puck.components.stat-widget",
    title: "Stat",
    summary:
      "KPI card over kernel objects with a count / sum / avg / min / max aggregation and a label.",
  },
  {
    id: "puck.components.badge",
    title: "Badge",
    summary:
      "Small colored tag with configurable tone. Use for status chips and labels.",
  },
  {
    id: "puck.components.alert",
    title: "Alert",
    summary:
      "Inline message banner with info / success / warning / error variants and optional dismiss button.",
  },
  {
    id: "puck.components.progress-bar",
    title: "Progress Bar",
    summary:
      "Horizontal progress indicator with configurable value, max, tone, and optional label.",
  },

  // Dynamic data widgets (generic)
  {
    id: "puck.components.list-widget",
    title: "List Widget",
    summary:
      "Simple list view of records filtered by collection type. Read-only.",
  },
  {
    id: "puck.components.table-widget",
    title: "Table Widget",
    summary:
      "Sortable columnar table over kernel records. Columns defined by a compact spec (`id:Label, id:Label`).",
  },
  {
    id: "puck.components.kanban-widget",
    title: "Kanban Widget",
    summary:
      "Board view grouping records into columns by a status field. Drag cards to change status.",
  },
  {
    id: "puck.components.card-grid-widget",
    title: "Card Grid",
    summary:
      "Responsive grid of cards over filtered kernel records. Good for galleries and product lists.",
  },
  {
    id: "puck.components.report-widget",
    title: "Report Widget",
    summary:
      "Grouped / aggregated report with count / sum / avg / min / max per group.",
  },
  {
    id: "puck.components.calendar-widget",
    title: "Calendar",
    summary:
      "Month / week / day calendar over records with a date field. Click a day to see items.",
  },
  {
    id: "puck.components.chart-widget",
    title: "Chart",
    summary:
      "Bar / line / pie / area chart over aggregated records. Built on recharts.",
  },
  {
    id: "puck.components.map-widget",
    title: "Map",
    summary:
      "Leaflet + OpenStreetMap map with markers pulled from records that have lat / lng fields.",
  },

  // Form inputs
  {
    id: "puck.components.text-input",
    title: "Text Input",
    summary:
      "Single-line text input with placeholder, label, and validation hooks.",
  },
  {
    id: "puck.components.textarea-input",
    title: "Textarea",
    summary:
      "Multi-line text input with configurable rows and placeholder.",
  },
  {
    id: "puck.components.select-input",
    title: "Select",
    summary:
      "Dropdown input. Options authored inline or pulled from a Value List.",
  },
  {
    id: "puck.components.checkbox-input",
    title: "Checkbox",
    summary:
      "Boolean checkbox with label. Integrates with form state.",
  },
  {
    id: "puck.components.number-input",
    title: "Number Input",
    summary:
      "Numeric input with min / max / step constraints.",
  },
  {
    id: "puck.components.date-input",
    title: "Date Input",
    summary:
      "Date picker with optional time component and min / max bounds.",
  },

  // Domain-specific record widgets
  {
    id: "puck.components.tasks-widget",
    title: "Tasks",
    summary:
      "Task list with inline done-toggle, priority color, and filter by status.",
  },
  {
    id: "puck.components.reminders-widget",
    title: "Reminders",
    summary:
      "Reminder list with round checkboxes, repeat glyph, and grouping by day.",
  },
  {
    id: "puck.components.contacts-widget",
    title: "Contacts",
    summary:
      "Contact cards with avatar initials and mailto: / tel: action buttons.",
  },
  {
    id: "puck.components.events-widget",
    title: "Events",
    summary:
      "Event timeline with date chips and hh:mm times, sorted chronologically.",
  },
  {
    id: "puck.components.notes-widget",
    title: "Notes",
    summary:
      "Note cards with pinned star, preview text, and tag chips.",
  },
  {
    id: "puck.components.goals-widget",
    title: "Goals",
    summary:
      "Goal cards with progress bar, target, and current-value display.",
  },
  {
    id: "puck.components.habit-tracker-widget",
    title: "Habit Tracker",
    summary:
      "Weekly habit grid with streak fire icon and per-day completion ratio.",
  },
  {
    id: "puck.components.bookmarks-widget",
    title: "Bookmarks",
    summary:
      "Bookmark grid with favicon, title, and host text. Click to open in a new tab.",
  },
  {
    id: "puck.components.timer-widget",
    title: "Timer",
    summary:
      "Local focus timer with start / pause / reset and a session log.",
  },
  {
    id: "puck.components.capture-inbox-widget",
    title: "Capture Inbox",
    summary:
      "Quick-capture inbox with inline add and mark-processed action.",
  },

  // Records (data rows, not widgets)
  { id: "puck.components.task", title: "Task", summary: "A single task record with status, priority, due date, and tags." },
  { id: "puck.components.reminder", title: "Reminder", summary: "A single reminder record with date, repeat, and snooze." },
  { id: "puck.components.contact", title: "Contact", summary: "A single contact record with name, email, phone, and organization." },
  { id: "puck.components.event", title: "Event", summary: "A single calendar event with start / end, location, and attendees." },
  { id: "puck.components.note", title: "Note", summary: "A single note record with body, tags, and pinned flag." },
  { id: "puck.components.goal", title: "Goal", summary: "A single goal record with target, current value, and deadline." },
  { id: "puck.components.habit", title: "Habit", summary: "A single habit record with frequency, streak, and completion log." },
  { id: "puck.components.bookmark", title: "Bookmark", summary: "A single bookmark record with URL, title, and favicon." },
  { id: "puck.components.timer-session", title: "Timer Session", summary: "A single timer session record with duration and context." },
  { id: "puck.components.capture", title: "Capture", summary: "A single capture inbox row — a quick thought to process later." },

  // App structure
  {
    id: "puck.components.app",
    title: "App",
    summary:
      "A buildable Prism App — Flux, Cadence, etc. Contains an app-shell, routes, pages, and behaviors.",
  },
  {
    id: "puck.components.route",
    title: "Route",
    summary:
      "URL-to-page mapping inside an app. Define a path pattern and the page that renders for it.",
  },
  {
    id: "puck.components.behavior",
    title: "Behavior",
    summary:
      "Named Luau script attached to a component or route. Runs on events like mount, click, or change.",
  },
];

// ── Shell regions (Page Shell slots) ────────────────────────────────────────

const REGION_ENTRIES: HelpEntry[] = [
  {
    id: "puck.regions.header",
    title: "Header region",
    summary:
      "Top strip of a Page Shell. Good for page titles, breadcrumbs, or a page-level toolbar. Resize from its bottom edge.",
    docPath: "page-shell",
    docAnchor: "regions",
  },
  {
    id: "puck.regions.sidebar",
    title: "Sidebar region",
    summary:
      "Left column of a Page Shell. Good for a table of contents or filter panel. Resize from its right edge.",
    docPath: "page-shell",
    docAnchor: "regions",
  },
  {
    id: "puck.regions.main",
    title: "Main region",
    summary:
      "Primary content area of a Page Shell. Most blocks live here. Main expands to fill whatever space the other regions leave.",
    docPath: "page-shell",
    docAnchor: "regions",
  },
  {
    id: "puck.regions.footer",
    title: "Footer region",
    summary:
      "Bottom strip of a Page Shell. Good for status chips, tags, or a save button. Resize from its top edge.",
    docPath: "page-shell",
    docAnchor: "regions",
  },
];

// ── Style field groups (shared across all styled blocks) ────────────────────

const STYLE_FIELD_ENTRIES: HelpEntry[] = [
  {
    id: "puck.fields.spacing",
    title: "Spacing",
    summary:
      "Margin and padding for this block. Accepts css-style shorthand like `16px` or `8px 16px`.",
  },
  {
    id: "puck.fields.colors",
    title: "Colors",
    summary:
      "Foreground, background, and accent color overrides. Prefer design tokens for consistency.",
  },
  {
    id: "puck.fields.typography",
    title: "Typography",
    summary:
      "Font family, size, weight, line height, and letter spacing for text inside this block.",
  },
  {
    id: "puck.fields.borders",
    title: "Borders",
    summary:
      "Border width, style, color, and radius. Set each side independently or use the shorthand.",
  },
  {
    id: "puck.fields.effects",
    title: "Effects",
    summary:
      "Shadow, blur, opacity, and transform effects. Combine multiple shadows for layered glow.",
  },
  {
    id: "puck.fields.background",
    title: "Background",
    summary:
      "Fill color, gradient, image, or video for this block's background layer. Supports responsive overrides.",
  },
];

// Flatten, register, and re-export for tests.

export const PUCK_HELP_ENTRIES: readonly HelpEntry[] = [
  ...CATEGORY_ENTRIES,
  ...FLAGSHIP_ENTRIES,
  ...STUB_ENTRIES,
  ...REGION_ENTRIES,
  ...STYLE_FIELD_ENTRIES,
];

HelpRegistry.registerMany(PUCK_HELP_ENTRIES);
