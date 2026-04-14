/**
 * Playground seed initializer — starter-app edition.
 *
 * Seeds three full Prism Apps via `materializeStarterApp` from the
 * @prism/core builder package:
 *
 *   • Flux    — records-first workspace (tasks/contacts/invoices).
 *   • Cadence — lesson-heavy landing with a dark top bar.
 *   • Lattice — graph / notebook workspace with minimal chrome.
 *
 * Each app gets its own `app`, `app-shell`, `route`s, and `page`s with per-
 * profile chrome, so the two-level sidebar in `playground-app.tsx` can pivot
 * between them. Demo collections (tasks / contacts / sales / places /
 * events) still live at the workspace root so the dynamic data widgets have
 * something to render inside the routed pages.
 *
 * Kept in the playground (not studio) so studio's builtin-initializers stay
 * focused on the canonical Home/About workspace.
 */

import type { StudioInitializer, StudioKernel } from "@prism/studio/kernel/index.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import {
  materializeStarterApp,
  FLUX_PROFILE,
  LATTICE_PROFILE,
  CADENCE_PROFILE,
  type StarterCreateObjectFn,
} from "@prism/core/builder";

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
    make(kernel, "demo-task", title, null, i, {
      name: title,
      status: statuses[i % statuses.length],
      priority: ["low", "medium", "high"][i % 3],
      owner: ["Ash", "Bea", "Cy", "Dee"][i % 4],
      estimate: (i % 5) + 1,
    });
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
      make(kernel, "demo-sale", `${product} in ${region}`, null, n, {
        name: `${product} in ${region}`,
        region,
        product,
        amount: Math.round(500 + Math.random() * 4500),
        quarter: ["Q1", "Q2", "Q3", "Q4"][n % 4],
      });
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
    make(kernel, "demo-event", labels[i] ?? `Event ${i}`, null, i, {
      name: labels[i] ?? `Event ${i}`,
      date: fmt(d),
      owner: ["Ash", "Bea", "Cy", "Dee"][i % 4],
    });
  });
}

// ── Starter app bootstrap ──────────────────────────────────────────────────

/**
 * Bridge the kernel's createObject signature into the shape
 * `materializeStarterApp` expects. The materializer returns `{ id }` for
 * every created object so it can wire parent/child relationships; we
 * forward the kernel's real ObjectId through the cast.
 */
function makeStarterAdapter(kernel: StudioKernel): StarterCreateObjectFn {
  return (input) =>
    ({
      id: kernel.createObject({
        type: input.type,
        name: input.name,
        parentId: input.parentId as unknown as ObjectId | null,
        position: input.position,
        data: input.data,
      }).id as unknown as string,
    });
}

// ── Public initializer ─────────────────────────────────────────────────────

export const playgroundSeedInitializer: StudioInitializer = {
  id: "playground-seed",
  name: "Playground Seed",
  install({ kernel }) {
    if (kernel.store.objectCount() > 0) return () => {};

    // Workspace-level demo data that all three starter apps read from.
    seedTasks(kernel);
    seedContacts(kernel);
    seedSales(kernel);
    seedPlaces(kernel);
    seedEvents(kernel);

    // Three full Prism Apps, each with their own shell + routes + pages.
    const adapter = makeStarterAdapter(kernel);
    const flux = materializeStarterApp(FLUX_PROFILE, adapter);
    materializeStarterApp(CADENCE_PROFILE, adapter);
    materializeStarterApp(LATTICE_PROFILE, adapter);

    kernel.undo.clear();
    // Default selection: the home route of the Flux starter — gives the
    // sidebar something useful to mount when the playground first loads.
    kernel.select(flux.homeRouteId as unknown as ObjectId);

    return () => {};
  },
};
