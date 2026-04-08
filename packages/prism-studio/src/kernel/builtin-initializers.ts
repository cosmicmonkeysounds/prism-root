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
      data: { title: "Welcome to Prism", slug: "/", layout: "single", published: false },
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
      type: "lua-block",
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
      data: { title: "About Us", slug: "/about", layout: "sidebar", published: false },
    });

    // Clear undo history so seed data isn't undoable, and select the home page.
    kernel.undo.clear();
    kernel.select(page.id);

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
  ];
}
