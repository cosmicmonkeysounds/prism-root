/**
 * Built-in studio initializers — seed templates and demo content.
 *
 * These replaces the ad-hoc `registerSeedTemplates()` and `seedDemoData()`
 * helpers that used to live at the top of App.tsx. Moving them into the
 * kernel's initializer pipeline means the host (App.tsx) stops knowing
 * about demo-data literals or template shapes — it just constructs the
 * kernel and the bundles handle themselves.
 */

import type { ObjectTemplate } from "@prism/core/template";
import type { StudioInitializer } from "./initializer.js";
import { registerSectionTemplates } from "./section-templates.js";

// ── Page templates ──────────────────────────────────────────────────────────

const BLOG_PAGE_TEMPLATE: ObjectTemplate = {
  id: "blog-page",
  name: "Blog Post",
  description: "A page with hero section, heading, and body text",
  category: "page",
  createdAt: new Date(0).toISOString(),
  root: {
    placeholderId: "root",
    type: "page",
    name: "{{title}}",
    data: { title: "{{title}}", slug: "", layout: "single", published: false },
    children: [
      {
        placeholderId: "hero",
        type: "section",
        name: "Hero",
        data: { variant: "hero", padding: "lg" },
      },
      {
        placeholderId: "body",
        type: "section",
        name: "Body",
        data: { variant: "default", padding: "md" },
        children: [
          {
            placeholderId: "heading",
            type: "heading",
            name: "Title",
            data: { text: "{{title}}", level: "h1", align: "left" },
          },
          {
            placeholderId: "text",
            type: "text-block",
            name: "Content",
            data: { content: "Start writing here...", format: "markdown" },
          },
        ],
      },
    ],
  },
  variables: [{ name: "title", label: "Page Title", required: true }],
};

const LANDING_PAGE_TEMPLATE: ObjectTemplate = {
  id: "landing-page",
  name: "Landing Page",
  description: "Hero + features + CTA sections",
  category: "page",
  createdAt: new Date(0).toISOString(),
  root: {
    placeholderId: "root",
    type: "page",
    name: "{{title}}",
    data: { title: "{{title}}", slug: "", layout: "single", published: false },
    children: [
      {
        placeholderId: "hero",
        type: "section",
        name: "Hero",
        data: { variant: "hero", padding: "lg" },
        children: [
          {
            placeholderId: "h1",
            type: "heading",
            name: "Headline",
            data: { text: "{{title}}", level: "h1", align: "center" },
          },
          {
            placeholderId: "sub",
            type: "text-block",
            name: "Subtitle",
            data: { content: "Describe your product or service here.", format: "markdown" },
          },
          {
            placeholderId: "cta",
            type: "button",
            name: "CTA Button",
            data: { label: "Get Started", variant: "primary", url: "#" },
          },
        ],
      },
      {
        placeholderId: "features",
        type: "section",
        name: "Features",
        data: { variant: "default", padding: "md" },
        children: [
          {
            placeholderId: "fh",
            type: "heading",
            name: "Features Heading",
            data: { text: "Features", level: "h2", align: "center" },
          },
        ],
      },
    ],
  },
  variables: [{ name: "title", label: "Page Title", required: true }],
};

/** Initializer that registers the blog + landing page templates. */
export const pageTemplatesInitializer: StudioInitializer = {
  id: "builtin-page-templates",
  name: "Built-in Page Templates",
  install({ kernel }) {
    kernel.registerTemplate(BLOG_PAGE_TEMPLATE);
    kernel.registerTemplate(LANDING_PAGE_TEMPLATE);
    return () => {};
  },
};

/** Initializer that registers all section-level templates (hero, CTA, etc). */
export const sectionTemplatesInitializer: StudioInitializer = {
  id: "builtin-section-templates",
  name: "Built-in Section Templates",
  install({ kernel }) {
    registerSectionTemplates(kernel);
    return () => {};
  },
};

/**
 * Initializer that writes a small demo workspace (Home / About pages) the
 * first time the kernel boots against an empty store. No-op when objects
 * already exist.
 */
export const demoWorkspaceInitializer: StudioInitializer = {
  id: "builtin-demo-workspace",
  name: "Demo Workspace",
  install({ kernel }) {
    if (kernel.store.objectCount() > 0) return () => {};

    const page = kernel.createObject({
      type: "page",
      name: "Home",
      parentId: null,
      position: 0,
      status: "draft",
      tags: [],
      date: null,
      endDate: null,
      description: "The landing page",
      color: null,
      image: null,
      pinned: false,
      data: { title: "Welcome to Prism", slug: "/", layout: "flow", published: false },
    });

    kernel.createObject({
      type: "section",
      name: "Hero",
      parentId: page.id,
      position: 0,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: { variant: "hero", padding: "lg" },
    });

    const contentSection = kernel.createObject({
      type: "section",
      name: "Content",
      parentId: page.id,
      position: 1,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: { variant: "default", padding: "md" },
    });

    kernel.createObject({
      type: "heading",
      name: "Main Heading",
      parentId: contentSection.id,
      position: 0,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: { text: "Build anything with Prism", level: "h1", align: "center" },
    });

    kernel.createObject({
      type: "text-block",
      name: "Intro Text",
      parentId: contentSection.id,
      position: 1,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: {
        content: "Prism is a **distributed visual operating system**. Every app is an IDE.",
        format: "markdown",
      },
    });

    kernel.createObject({
      type: "luau-block",
      name: "Status Widget",
      parentId: contentSection.id,
      position: 2,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: {
        title: "Status Widget",
        source: `-- Status Widget\nreturn ui.column({\n  ui.row({\n    ui.badge("Online", "green"),\n    ui.badge("v1.0", "blue"),\n    ui.spacer(),\n    ui.label("Prism Studio"),\n  }),\n  ui.divider(),\n  ui.row({\n    ui.button("Refresh"),\n    ui.button("Settings"),\n  }),\n})`,
      },
    });

    kernel.createObject({
      type: "page",
      name: "About",
      parentId: null,
      position: 1,
      status: "draft",
      tags: [],
      date: null,
      endDate: null,
      description: "About page",
      color: null,
      image: null,
      pinned: false,
      data: { title: "About Us", slug: "/about", layout: "shell", published: false },
    });

    // ── Demo App with routes for the Sitemap lens ──────────────────────────
    // Seeds an `app` object plus six routes (with hierarchy + a behavior that
    // calls `ui.navigate(...)`) so a fresh-install Sitemap tab isn't empty.
    const app = kernel.createObject({
      type: "app",
      name: "Demo App",
      parentId: null,
      position: 2,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "A small app to demonstrate the Sitemap lens",
      color: null,
      image: null,
      pinned: false,
      data: { name: "Demo App", profileId: "studio", themePrimary: "#a855f7" },
    });

    const home = kernel.createObject({
      type: "route",
      name: "Home",
      parentId: app.id,
      position: 0,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: { path: "/", label: "Home", showInNav: true },
    });

    const dashboard = kernel.createObject({
      type: "route",
      name: "Dashboard",
      parentId: app.id,
      position: 1,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: { path: "/dashboard", label: "Dashboard", showInNav: true },
    });

    const tasks = kernel.createObject({
      type: "route",
      name: "Tasks",
      parentId: app.id,
      position: 2,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: {
        path: "/dashboard/tasks",
        label: "Tasks",
        showInNav: true,
        parentRouteId: dashboard.id,
      },
    });

    kernel.createObject({
      type: "route",
      name: "Task Detail",
      parentId: app.id,
      position: 3,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: {
        path: "/dashboard/tasks/:id",
        label: "Task Detail",
        showInNav: false,
        parentRouteId: tasks.id,
      },
    });

    kernel.createObject({
      type: "route",
      name: "Settings",
      parentId: app.id,
      position: 4,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: { path: "/settings", label: "Settings", showInNav: true },
    });

    kernel.createObject({
      type: "route",
      name: "About",
      parentId: app.id,
      position: 5,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: { path: "/about", label: "About", showInNav: true },
    });

    // Tag the home route on the app so the sitemap can mark it. Using
    // updateObject keeps the seed in one place even though createObject
    // doesn't accept forward references.
    kernel.updateObject(app.id, { data: { ...app.data, homeRouteId: home.id } });

    // Behavior that navigates from the home route → /dashboard so the
    // sitemap renders an animated "transition" edge by default.
    kernel.createObject({
      type: "behavior",
      name: "Open Dashboard",
      parentId: app.id,
      position: 6,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: {
        targetObjectId: home.id,
        trigger: "onClick",
        source: 'ui.navigate("/dashboard")',
        enabled: true,
      },
    });

    // Clear undo history so seed data isn't undoable, and select the home page.
    kernel.undo.clear();
    kernel.select(page.id);

    return () => {};
  },
};

/**
 * Seed a small set of dynamic records (tasks, reminders, contacts, events,
 * notes, goals, habits, bookmarks, captures) so the dynamic widgets have
 * something to render on a first-run. Guarded to an empty store so it never
 * overwrites existing user data.
 */
export const dynamicDataInitializer: StudioInitializer = {
  id: "builtin-dynamic-data",
  name: "Dynamic Demo Data",
  install({ kernel }) {
    const existing = kernel.store
      .allObjects()
      .filter((o) => ["task", "reminder", "contact", "event", "note", "goal", "habit", "bookmark", "capture"].includes(o.type));
    if (existing.length > 0) return () => {};

    const now = Date.now();
    const isoInDays = (days: number, hour = 9) => {
      const d = new Date(now);
      d.setDate(d.getDate() + days);
      d.setHours(hour, 0, 0, 0);
      return d.toISOString();
    };

    const makeRecord = (args: {
      type: string;
      name: string;
      status?: string | null;
      date?: string | null;
      tags?: string[];
      description?: string;
      pinned?: boolean;
      color?: string | null;
      data: Record<string, unknown>;
    }) => {
      kernel.createObject({
        type: args.type,
        name: args.name,
        parentId: null,
        position: 0,
        status: args.status ?? "todo",
        tags: args.tags ?? [],
        date: args.date ?? null,
        endDate: null,
        description: args.description ?? "",
        color: args.color ?? null,
        image: null,
        pinned: args.pinned ?? false,
        data: args.data,
      });
    };

    // ── Tasks ────────────────────────────────────────────────────────────
    makeRecord({
      type: "task",
      name: "Review Q2 roadmap draft",
      status: "todo",
      date: isoInDays(0, 17),
      tags: ["planning"],
      data: { priority: "high", project: "Roadmap", estimateMinutes: 45 },
    });
    makeRecord({
      type: "task",
      name: "Send launch checklist to team",
      status: "doing",
      date: isoInDays(1, 10),
      tags: ["launch"],
      data: { priority: "urgent", project: "Launch", estimateMinutes: 20 },
    });
    makeRecord({
      type: "task",
      name: "Refactor auth middleware",
      status: "todo",
      date: isoInDays(-2, 12),
      tags: ["backend"],
      data: { priority: "normal", project: "Infra", estimateMinutes: 120 },
    });
    makeRecord({
      type: "task",
      name: "Expense report for March",
      status: "done",
      date: isoInDays(-5, 10),
      tags: ["ops"],
      data: { priority: "low", project: "Admin", estimateMinutes: 30 },
    });
    makeRecord({
      type: "task",
      name: "Weekly review",
      status: "todo",
      date: isoInDays(3, 16),
      tags: ["ritual"],
      data: { priority: "normal", project: "Self", estimateMinutes: 30 },
    });

    // ── Reminders ────────────────────────────────────────────────────────
    makeRecord({
      type: "reminder",
      name: "Water the plants",
      status: "todo",
      date: isoInDays(0, 18),
      data: { repeat: "daily", channel: "notification" },
    });
    makeRecord({
      type: "reminder",
      name: "Call the vet",
      status: "todo",
      date: isoInDays(2, 10),
      data: { repeat: "none", channel: "notification" },
    });
    makeRecord({
      type: "reminder",
      name: "Pay credit card bill",
      status: "todo",
      date: isoInDays(-1, 9),
      data: { repeat: "monthly", channel: "email" },
    });

    // ── Contacts ─────────────────────────────────────────────────────────
    makeRecord({
      type: "contact",
      name: "Alex Chen",
      status: null,
      pinned: true,
      tags: ["team"],
      data: {
        email: "alex@example.com",
        phone: "+1-555-0142",
        org: "Prism Labs",
        role: "Design Lead",
        lastContactedAt: isoInDays(-3),
      },
    });
    makeRecord({
      type: "contact",
      name: "Priya Rao",
      status: null,
      pinned: true,
      tags: ["team"],
      data: {
        email: "priya@example.com",
        phone: "+1-555-0193",
        org: "Prism Labs",
        role: "Eng Manager",
        lastContactedAt: isoInDays(-1),
      },
    });
    makeRecord({
      type: "contact",
      name: "Jordan Baker",
      status: null,
      tags: ["client"],
      data: {
        email: "jordan@acme.com",
        org: "Acme Co.",
        role: "Product Manager",
        lastContactedAt: isoInDays(-10),
      },
    });

    // ── Events ───────────────────────────────────────────────────────────
    makeRecord({
      type: "event",
      name: "Team standup",
      status: "confirmed",
      date: isoInDays(0, 10),
      data: { location: "Zoom", allDay: false, attendance: "confirmed" },
    });
    makeRecord({
      type: "event",
      name: "Launch review",
      status: "confirmed",
      date: isoInDays(2, 14),
      data: { location: "Room 3B", allDay: false, attendance: "confirmed" },
    });
    makeRecord({
      type: "event",
      name: "1:1 with Priya",
      status: "confirmed",
      date: isoInDays(1, 15),
      data: { location: "Zoom", allDay: false, attendance: "confirmed" },
    });
    makeRecord({
      type: "event",
      name: "Design crit",
      status: "tentative",
      date: isoInDays(5, 11),
      data: { location: "Figma", allDay: false, attendance: "tentative" },
    });

    // ── Notes ────────────────────────────────────────────────────────────
    makeRecord({
      type: "note",
      name: "Architecture sketch",
      pinned: true,
      tags: ["ideas", "infra"],
      data: {
        body: "Kernel holds the store, lenses project it. Records are CRDT leaves. Widgets query by type and render.",
        format: "markdown",
      },
    });
    makeRecord({
      type: "note",
      name: "Pitch outline",
      tags: ["marketing"],
      data: {
        body: "1. Problem 2. Insight 3. Solution 4. Demo 5. Traction 6. Ask",
        format: "markdown",
      },
    });
    makeRecord({
      type: "note",
      name: "Book recommendations",
      tags: ["reading"],
      data: {
        body: "- Seeing Like a State\n- Designing Data-Intensive Applications\n- The Beginning of Infinity",
        format: "markdown",
      },
    });

    // ── Goals ────────────────────────────────────────────────────────────
    makeRecord({
      type: "goal",
      name: "Ship v1.0",
      status: "doing",
      date: isoInDays(30),
      data: { targetValue: 100, currentValue: 68, unit: "%", cadence: "once" },
    });
    makeRecord({
      type: "goal",
      name: "Read 24 books",
      status: "doing",
      date: isoInDays(260),
      data: { targetValue: 24, currentValue: 9, unit: "books", cadence: "yearly" },
    });
    makeRecord({
      type: "goal",
      name: "Run 200km",
      status: "doing",
      date: isoInDays(80),
      data: { targetValue: 200, currentValue: 47, unit: "km", cadence: "quarterly" },
    });

    // ── Habits ───────────────────────────────────────────────────────────
    makeRecord({
      type: "habit",
      name: "Morning pages",
      status: "doing",
      data: { frequency: "daily", streak: 12, longestStreak: 42, targetPerWeek: 7 },
    });
    makeRecord({
      type: "habit",
      name: "Workout",
      status: "doing",
      data: { frequency: "weekdays", streak: 4, longestStreak: 18, targetPerWeek: 5 },
    });
    makeRecord({
      type: "habit",
      name: "Read 20 pages",
      status: "doing",
      data: { frequency: "daily", streak: 3, longestStreak: 30, targetPerWeek: 7 },
    });

    // ── Bookmarks ────────────────────────────────────────────────────────
    makeRecord({
      type: "bookmark",
      name: "Loro CRDT docs",
      data: { url: "https://loro.dev", folder: "Reference", excerpt: "CRDT framework used in Prism" },
    });
    makeRecord({
      type: "bookmark",
      name: "React docs",
      data: { url: "https://react.dev", folder: "Reference" },
    });
    makeRecord({
      type: "bookmark",
      name: "Tauri docs",
      data: { url: "https://tauri.app", folder: "Reference" },
    });
    makeRecord({
      type: "bookmark",
      name: "Measured Puck",
      data: { url: "https://puckeditor.com", folder: "Reference" },
    });

    // ── Captures ─────────────────────────────────────────────────────────
    makeRecord({
      type: "capture",
      name: "Idea: pinboard lens for related notes",
      status: "todo",
      data: { body: "Idea: pinboard lens for related notes", source: "quick" },
    });
    makeRecord({
      type: "capture",
      name: "Buy birthday gift for M.",
      status: "todo",
      data: { body: "Buy birthday gift for M.", source: "quick" },
    });

    kernel.undo.clear();
    return () => {};
  },
};

/**
 * The canonical built-in initializer list. Pass this to
 * `createStudioKernel({ initializers: createBuiltinInitializers() })`.
 * Order matters: templates register first so the demo workspace can
 * reference them if it wants to (today it doesn't, but the ordering
 * keeps future extensions safe).
 */
export function createBuiltinInitializers(): StudioInitializer[] {
  return [
    pageTemplatesInitializer,
    sectionTemplatesInitializer,
    demoWorkspaceInitializer,
    dynamicDataInitializer,
  ];
}
