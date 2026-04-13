/**
 * Playground seed initializer.
 *
 * Creates a self-contained demo workspace with:
 *   - sample collections (tasks, contacts, sales, places, events) so the
 *     data-aware Puck widgets have something real to render;
 *   - several demo pages, each pre-populated with a curated set of Puck
 *     blocks that exercise a specific category of widgets.
 *
 * Lives in the playground (not in @prism/studio) so the main app's
 * builtin-initializers stay focused on the canonical Home/About workspace.
 */

import type { StudioInitializer, StudioKernel } from "@prism/studio/kernel/index.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";

type CreateObjectInput = Parameters<StudioKernel["createObject"]>[0];

function make(
  kernel: StudioKernel,
  type: string,
  name: string,
  parentId: ObjectId | null,
  position: number,
  data: Record<string, unknown>,
  extras: Partial<CreateObjectInput> = {},
): GraphObject {
  return kernel.createObject({
    type,
    name,
    parentId,
    position,
    data,
    ...extras,
  });
}

// ── Sample collections ─────────────────────────────────────────────────────

function seedTasks(kernel: StudioKernel): void {
  const statuses = ["backlog", "todo", "in-progress", "review", "done"] as const;
  const titles = [
    "Wire Puck bridge to Loro",
    "Draft kanban widget",
    "Implement chart aggregation",
    "Add leaflet map view",
    "Pin sample data set",
    "QA the layout panel",
    "Document widget catalog",
    "Polish form input renderers",
    "Refactor widget palette",
    "Audit accessibility",
    "Wire undo/redo into Puck",
    "Add publish workflow stub",
    "Hook up keyboard shortcuts",
    "Final pass on dark theme",
    "Demo recording for stakeholders",
  ];
  titles.forEach((title, i) => {
    make(
      kernel,
      "demo-task",
      title,
      null,
      i,
      {
        name: title,
        status: statuses[i % statuses.length],
        priority: ["low", "medium", "high"][i % 3],
        owner: ["Ash", "Bea", "Cy", "Dee"][i % 4],
        estimate: (i % 5) + 1,
      },
    );
  });
}

function seedContacts(kernel: StudioKernel): void {
  const contacts = [
    { name: "Ada Lovelace", role: "Engineer", company: "Analytical", email: "ada@analytical.test" },
    { name: "Alan Turing", role: "Researcher", company: "Bletchley", email: "alan@bletchley.test" },
    { name: "Grace Hopper", role: "Compiler Designer", company: "Univac", email: "grace@univac.test" },
    { name: "Edsger Dijkstra", role: "Mathematician", company: "Eindhoven", email: "edsger@eth.test" },
    { name: "Margaret Hamilton", role: "Software Lead", company: "MIT", email: "mh@apollo.test" },
    { name: "Linus Torvalds", role: "Kernel Hacker", company: "Linux", email: "linus@kernel.test" },
    { name: "Donald Knuth", role: "Author", company: "Stanford", email: "knuth@stanford.test" },
    { name: "Barbara Liskov", role: "Type Theorist", company: "MIT", email: "bl@mit.test" },
  ];
  contacts.forEach((c, i) => {
    make(kernel, "demo-contact", c.name, null, i, c);
  });
}

function seedSales(kernel: StudioKernel): void {
  const regions = ["east", "west", "north", "south"];
  const products = ["Studio", "Flux", "Lattice", "Cadence"];
  let n = 0;
  for (const region of regions) {
    for (const product of products) {
      n++;
      make(
        kernel,
        "demo-sale",
        `${product} in ${region}`,
        null,
        n,
        {
          name: `${product} in ${region}`,
          region,
          product,
          amount: Math.round(500 + Math.random() * 4500),
          quarter: ["Q1", "Q2", "Q3", "Q4"][n % 4],
        },
      );
    }
  }
}

function seedPlaces(kernel: StudioKernel): void {
  const places = [
    { name: "Anthropic HQ", lat: 37.7853, lng: -122.3963 },
    { name: "Brooklyn Studio", lat: 40.6782, lng: -73.9442 },
    { name: "Berlin Office", lat: 52.52, lng: 13.405 },
    { name: "Tokyo Outpost", lat: 35.6762, lng: 139.6503 },
    { name: "São Paulo Hub", lat: -23.5505, lng: -46.6333 },
    { name: "Cape Town Lab", lat: -33.9249, lng: 18.4241 },
    { name: "Sydney Workshop", lat: -33.8688, lng: 151.2093 },
  ];
  places.forEach((p, i) => {
    make(kernel, "demo-place", p.name, null, i, p);
  });
}

function seedEvents(kernel: StudioKernel): void {
  const today = new Date();
  const fmt = (d: Date) => d.toISOString().slice(0, 10);
  const days = [-3, -1, 0, 2, 5, 8, 12, 18];
  const labels = [
    "Kickoff",
    "Design Review",
    "Standup",
    "Demo Day",
    "Sprint Planning",
    "Retro",
    "Launch",
    "Postmortem",
  ];
  days.forEach((offset, i) => {
    const d = new Date(today);
    d.setDate(today.getDate() + offset);
    make(
      kernel,
      "demo-event",
      labels[i] ?? `Event ${i}`,
      null,
      i,
      {
        name: labels[i] ?? `Event ${i}`,
        date: fmt(d),
        owner: ["Ash", "Bea", "Cy", "Dee"][i % 4],
      },
    );
  });
}

// ── Page builders ──────────────────────────────────────────────────────────

function pageRoot(kernel: StudioKernel, name: string, position: number, slug: string): GraphObject {
  return make(
    kernel,
    "page",
    name,
    null,
    position,
    { title: name, slug, layout: "single", published: false },
  );
}

function buildWelcomePage(kernel: StudioKernel, page: GraphObject): void {
  const hero = make(
    kernel,
    "section",
    "Hero",
    page.id,
    0,
    { variant: "hero", padding: "lg" },
  );
  make(kernel, "heading", "Welcome heading", hero.id, 0, {
    text: "Prism Puck Playground",
    level: "h1",
    align: "center",
  });
  make(kernel, "text-block", "Intro", hero.id, 1, {
    content:
      "This is a standalone harness for the **Puck visual builder** wired to the Prism kernel. Switch pages on the left to preview every category of block.",
    format: "markdown",
  });
  make(kernel, "button", "Primary CTA", hero.id, 2, {
    label: "Start exploring",
    variant: "primary",
    href: "#",
  });

  const body = make(
    kernel,
    "section",
    "Highlights",
    page.id,
    1,
    { variant: "default", padding: "md" },
  );
  make(kernel, "columns", "Three highlights", body.id, 0, {
    columnCount: 3,
    gap: 16,
    align: "stretch",
  });
  make(kernel, "stat-widget", "Tasks", body.id, 1, {
    collectionType: "demo-task",
    label: "Open tasks",
    aggregation: "count",
    valueField: "",
    prefix: "",
    suffix: "",
    decimals: 0,
    thousands: "true",
  });
  make(kernel, "stat-widget", "Contacts", body.id, 2, {
    collectionType: "demo-contact",
    label: "Known contacts",
    aggregation: "count",
    valueField: "",
    prefix: "",
    suffix: "",
    decimals: 0,
    thousands: "true",
  });
  make(kernel, "stat-widget", "Revenue", body.id, 3, {
    collectionType: "demo-sale",
    label: "Pipeline",
    aggregation: "sum",
    valueField: "amount",
    prefix: "$",
    suffix: "",
    decimals: 0,
    thousands: "true",
  });
}

function buildDataPage(kernel: StudioKernel, page: GraphObject): void {
  const section = make(
    kernel,
    "section",
    "Data widgets",
    page.id,
    0,
    { variant: "default", padding: "md" },
  );
  make(kernel, "heading", "Title", section.id, 0, {
    text: "Data widgets",
    level: "h2",
    align: "left",
  });
  make(kernel, "kanban-widget", "Task kanban", section.id, 1, {
    collectionType: "demo-task",
    groupField: "status",
    titleField: "name",
    colorField: "",
    maxCardsPerColumn: 50,
  });
  make(kernel, "list-widget", "Task list", section.id, 2, {
    collectionType: "demo-task",
    titleField: "name",
    subtitleField: "owner",
    showStatus: true,
    showTimestamp: true,
  });
  make(kernel, "table-widget", "Task table", section.id, 3, {
    collectionType: "demo-task",
    columns: "name:Title, status:Status, owner:Owner, priority:Priority, estimate:Est",
    sortField: "name",
    sortDir: "asc",
  });
  make(kernel, "card-grid-widget", "Contact cards", section.id, 4, {
    collectionType: "demo-contact",
    titleField: "name",
    subtitleField: "role",
    minColumnWidth: 220,
    showStatus: false,
  });
}

function buildChartsPage(kernel: StudioKernel, page: GraphObject): void {
  const section = make(
    kernel,
    "section",
    "Charts & reports",
    page.id,
    0,
    { variant: "default", padding: "md" },
  );
  make(kernel, "heading", "Title", section.id, 0, {
    text: "Charts & reports",
    level: "h2",
    align: "left",
  });
  make(kernel, "chart-widget", "Bar by region", section.id, 1, {
    collectionType: "demo-sale",
    chartType: "bar",
    groupField: "region",
    valueField: "amount",
    aggregation: "sum",
  });
  make(kernel, "chart-widget", "Line by quarter", section.id, 2, {
    collectionType: "demo-sale",
    chartType: "line",
    groupField: "quarter",
    valueField: "amount",
    aggregation: "sum",
  });
  make(kernel, "chart-widget", "Pie by product", section.id, 3, {
    collectionType: "demo-sale",
    chartType: "pie",
    groupField: "product",
    valueField: "amount",
    aggregation: "sum",
  });
  make(kernel, "chart-widget", "Area task estimates", section.id, 4, {
    collectionType: "demo-task",
    chartType: "area",
    groupField: "owner",
    valueField: "estimate",
    aggregation: "sum",
  });
  make(kernel, "report-widget", "Sales report", section.id, 5, {
    collectionType: "demo-sale",
    groupField: "region",
    titleField: "name",
    valueField: "amount",
    aggregation: "sum",
  });
}

function buildMapCalendarPage(kernel: StudioKernel, page: GraphObject): void {
  const section = make(
    kernel,
    "section",
    "Map & calendar",
    page.id,
    0,
    { variant: "default", padding: "md" },
  );
  make(kernel, "heading", "Title", section.id, 0, {
    text: "Map & calendar",
    level: "h2",
    align: "left",
  });
  make(kernel, "map-widget", "Office map", section.id, 1, {
    collectionType: "demo-place",
    latField: "lat",
    lngField: "lng",
    titleField: "name",
    initialZoom: 2,
  });
  make(kernel, "calendar-widget", "Upcoming events", section.id, 2, {
    collectionType: "demo-event",
    dateField: "date",
    titleField: "name",
    viewType: "month",
  });
}

function buildFormsPage(kernel: StudioKernel, page: GraphObject): void {
  const section = make(
    kernel,
    "section",
    "Forms",
    page.id,
    0,
    { variant: "default", padding: "md" },
  );
  make(kernel, "heading", "Title", section.id, 0, {
    text: "Form inputs",
    level: "h2",
    align: "left",
  });
  make(kernel, "text-input", "Name", section.id, 1, {
    label: "Full name",
    placeholder: "Ada Lovelace",
    defaultValue: "",
    inputType: "text",
    required: "true",
    help: "Used as the display name on the dashboard.",
  });
  make(kernel, "text-input", "Email", section.id, 2, {
    label: "Email",
    placeholder: "you@example.com",
    defaultValue: "",
    inputType: "email",
    required: "true",
    help: "",
  });
  make(kernel, "textarea-input", "Bio", section.id, 3, {
    label: "Bio",
    placeholder: "Tell us about yourself…",
    defaultValue: "",
    rows: 5,
    required: "false",
    help: "Markdown supported.",
  });
  make(kernel, "select-input", "Role", section.id, 4, {
    label: "Role",
    options: "Engineer,Designer,PM,Researcher",
    defaultValue: "Engineer",
    required: "true",
    help: "",
  });
  make(kernel, "number-input", "Estimate", section.id, 5, {
    label: "Story points",
    defaultValue: 3,
    min: 0,
    max: 13,
    step: 1,
    required: "false",
    help: "",
  });
  make(kernel, "date-input", "Due date", section.id, 6, {
    label: "Due date",
    defaultValue: "",
    dateKind: "date",
    required: "false",
    help: "",
  });
  make(kernel, "checkbox-input", "Agree", section.id, 7, {
    label: "I accept the terms",
    defaultChecked: "false",
    help: "Required to continue.",
  });
}

function buildDisplayPage(kernel: StudioKernel, page: GraphObject): void {
  const section = make(
    kernel,
    "section",
    "Display & content",
    page.id,
    0,
    { variant: "default", padding: "md" },
  );
  make(kernel, "heading", "Title", section.id, 0, {
    text: "Display & content",
    level: "h2",
    align: "left",
  });
  make(kernel, "alert", "Alert", section.id, 1, {
    title: "Heads up",
    message: "These widgets are content-only — no kernel data binding.",
    tone: "info",
    icon: "",
  });
  make(kernel, "badge", "Badge new", section.id, 2, {
    label: "New",
    tone: "success",
    icon: "",
    outline: "false",
  });
  make(kernel, "progress-bar", "Progress", section.id, 3, {
    label: "Build progress",
    value: 72,
    max: 100,
    tone: "info",
    showPercent: "true",
  });
  make(kernel, "markdown-widget", "Markdown", section.id, 4, {
    source:
      "## Markdown widget\n\nSupports **bold**, *italic*, lists, and `inline code`.\n\n- One\n- Two\n- Three\n",
  });
  make(kernel, "code-block", "Code", section.id, 5, {
    source:
      "function greet(name: string): string {\n  return `Hello, ${name}!`;\n}\n",
    language: "typescript",
    caption: "greet.ts",
    lineNumbers: "true",
    wrap: "false",
  });
}

function buildLayoutPage(kernel: StudioKernel, page: GraphObject): void {
  const section = make(
    kernel,
    "section",
    "Layout primitives",
    page.id,
    0,
    { variant: "default", padding: "md" },
  );
  make(kernel, "heading", "Title", section.id, 0, {
    text: "Layout primitives",
    level: "h2",
    align: "left",
  });
  make(kernel, "columns", "Two columns", section.id, 1, {
    columnCount: 2,
    gap: 24,
    align: "stretch",
  });
  make(kernel, "divider", "Divider", section.id, 2, {
    dividerStyle: "dashed",
    thickness: 2,
    color: "#475569",
    spacing: 24,
    label: "Section break",
  });
  make(kernel, "spacer", "Spacer", section.id, 3, {
    size: 32,
    axis: "vertical",
  });
  make(kernel, "tab-container", "Tabs", section.id, 4, {
    tabs: "Overview,Details,Activity",
    activeTab: 0,
  });
}

// ── Public initializer ─────────────────────────────────────────────────────

export const playgroundSeedInitializer: StudioInitializer = {
  id: "playground-seed",
  name: "Playground Seed",
  install({ kernel }) {
    if (kernel.store.objectCount() > 0) return () => {};

    seedTasks(kernel);
    seedContacts(kernel);
    seedSales(kernel);
    seedPlaces(kernel);
    seedEvents(kernel);

    const welcome = pageRoot(kernel, "1. Welcome", 100, "/");
    buildWelcomePage(kernel, welcome);

    const data = pageRoot(kernel, "2. Data Widgets", 101, "/data");
    buildDataPage(kernel, data);

    const charts = pageRoot(kernel, "3. Charts & Reports", 102, "/charts");
    buildChartsPage(kernel, charts);

    const mapcal = pageRoot(kernel, "4. Map & Calendar", 103, "/map");
    buildMapCalendarPage(kernel, mapcal);

    const forms = pageRoot(kernel, "5. Forms", 104, "/forms");
    buildFormsPage(kernel, forms);

    const display = pageRoot(kernel, "6. Display & Content", 105, "/display");
    buildDisplayPage(kernel, display);

    const layout = pageRoot(kernel, "7. Layout Primitives", 106, "/layout");
    buildLayoutPage(kernel, layout);

    kernel.undo.clear();
    kernel.select(welcome.id);

    return () => {};
  },
};
