/**
 * Materialise a starter-app template into a kernel / store.
 *
 * Turns an `AppProfile.starterApp` recipe into a concrete tree of objects:
 *
 *   app (category=app)
 *     ├── app-shell (+ per-slot children)
 *     ├── route #1 ─────── page (+ page-shell + per-slot children)
 *     ├── route #2 ─────── page (+ page-shell)
 *     └── …
 *
 * Pure and kernel-agnostic — takes a `createObject` callback that mirrors
 * the shape of `StudioKernel.createObject`, so the same helper is usable
 * from tests with a minimal fake and from the Puck playground with a real
 * kernel. No React, no Puck, no studio imports.
 */

import type { AppProfile, StarterAppTemplate, StarterRouteTemplate, StarterShellChild } from "./types.js";

// ── Caller-facing types ────────────────────────────────────────────────────

/**
 * Minimal creation input accepted by `materializeStarterApp`. Shaped to be
 * structurally compatible with `StudioKernel.createObject` — studio calls
 * end up passing `kernel.createObject` directly. Tests use a small fake.
 */
export interface StarterCreateObjectInput {
  type: string;
  name: string;
  parentId: string | null;
  position: number;
  data: Record<string, unknown>;
}

/** What the callback must return — just the ID so we can nest children. */
export interface StarterCreatedObject {
  id: string;
}

export type StarterCreateObjectFn = (input: StarterCreateObjectInput) => StarterCreatedObject;

/** Summary of everything the materialiser produced, keyed by logical role. */
export interface MaterializedStarterApp {
  appId: string;
  appShellId: string;
  /** Route id → created page id. */
  routeToPageId: Record<string, string>;
  /** Route id → the `route` object id. */
  routeIds: string[];
  /** The route id marked as the home. */
  homeRouteId: string;
}

// ── Main entry ─────────────────────────────────────────────────────────────

/**
 * Create the full `app → app-shell + routes + pages` tree for a profile
 * whose `starterApp` field is set. Throws if the profile has no template
 * or if no route is marked as the home route.
 *
 * The caller is responsible for running this inside whatever transaction
 * boundary the host kernel expects (Loro commits, undo batches, etc.).
 */
export function materializeStarterApp(
  profile: AppProfile,
  createObject: StarterCreateObjectFn,
): MaterializedStarterApp {
  const template = profile.starterApp;
  if (!template) {
    throw new Error(`Profile "${profile.id}" has no starterApp template`);
  }
  validateTemplate(profile.id, template);

  // 1. The root `app` object itself.
  const app = createObject({
    type: "app",
    name: template.label,
    parentId: null,
    position: 0,
    data: {
      name: template.label,
      profileId: profile.id,
      themePrimary: profile.theme?.primary ?? "",
      description: template.description ?? "",
    },
  });

  // 2. App Shell (outer chrome shared by every route).
  const appShell = createObject({
    type: "app-shell",
    name: `${template.label} App Shell`,
    parentId: app.id,
    position: 0,
    data: { ...template.appShell.data },
  });
  for (const [index, child] of (template.appShell.children ?? []).entries()) {
    createShellChild(appShell.id, child, index, createObject);
  }

  // 3. Routes and pages — routes carry `path → pageId`, pages carry a
  //    page-shell with its slot children.
  const routeIds: string[] = [];
  const routeToPageId: Record<string, string> = {};
  let homeRouteId: string | null = null;

  for (const [index, routeTemplate] of template.routes.entries()) {
    // Create the page under the `app` (so it's addressable).
    const page = createObject({
      type: "page",
      name: routeTemplate.label,
      parentId: app.id,
      position: index + 1, // 0 is the app-shell
      data: {
        title: routeTemplate.label,
        slug: routeTemplate.path,
        layout: "shell",
        published: false,
      },
    });

    // Seed a page-shell under the page.
    const pageShell = createObject({
      type: "page-shell",
      name: "Page Shell",
      parentId: page.id,
      position: 0,
      data: { ...template.defaultPageShell.data },
    });
    for (const [childIndex, child] of (template.defaultPageShell.children ?? []).entries()) {
      createShellChild(pageShell.id, child, childIndex, createObject);
    }

    // Seed the default body content for the page template.
    seedPageTemplateBody(routeTemplate.pageTemplate, pageShell.id, createObject);

    // Create the route under the app.
    const route = createObject({
      type: "route",
      name: routeTemplate.label,
      parentId: app.id,
      position: template.routes.length + 1 + index,
      data: {
        path: routeTemplate.path,
        pageId: page.id,
        label: routeTemplate.label,
        showInNav: routeTemplate.showInNav ?? true,
        parentRouteId: "",
      },
    });

    routeIds.push(route.id);
    routeToPageId[route.id] = page.id;
    if (routeTemplate.isHome) {
      homeRouteId = route.id;
    }
  }

  if (homeRouteId === null) {
    // validateTemplate already enforced this, but keep the narrowing typesafe.
    throw new Error(`Profile "${profile.id}" starterApp has no home route`);
  }

  return {
    appId: app.id,
    appShellId: appShell.id,
    routeToPageId,
    routeIds,
    homeRouteId,
  };
}

// ── Helpers ────────────────────────────────────────────────────────────────

function createShellChild(
  parentId: string,
  child: StarterShellChild,
  position: number,
  createObject: StarterCreateObjectFn,
): StarterCreatedObject {
  return createObject({
    type: child.type,
    name: child.name ?? child.type,
    parentId,
    position,
    data: {
      __slot: child.slot,
      ...(child.data ?? {}),
    },
  });
}

/**
 * Seed the body content for a page template kind. `blank` creates nothing;
 * `landing` and `blog` drop a hero + body text into the `main` slot of the
 * page-shell so the starter app is not an empty canvas.
 */
function seedPageTemplateBody(
  pageTemplate: StarterRouteTemplate["pageTemplate"],
  pageShellId: string,
  createObject: StarterCreateObjectFn,
): void {
  if (pageTemplate === "blank") return;

  if (pageTemplate === "landing") {
    createObject({
      type: "hero",
      name: "Hero",
      parentId: pageShellId,
      position: 0,
      data: {
        __slot: "main",
        align: "center",
        minHeight: 360,
      },
    });
    createObject({
      type: "text-block",
      name: "Body",
      parentId: pageShellId,
      position: 1,
      data: {
        __slot: "main",
        content: "Write a compelling intro here.",
      },
    });
    return;
  }

  // blog
  createObject({
    type: "heading",
    name: "Title",
    parentId: pageShellId,
    position: 0,
    data: {
      __slot: "main",
      text: "Blog post title",
      level: "h1",
      align: "left",
    },
  });
  createObject({
    type: "text-block",
    name: "Body",
    parentId: pageShellId,
    position: 1,
    data: {
      __slot: "main",
      content: "Start writing your post...",
    },
  });
}

function validateTemplate(profileId: string, template: StarterAppTemplate): void {
  if (template.routes.length === 0) {
    throw new Error(`Profile "${profileId}" starterApp has no routes`);
  }
  const homeCount = template.routes.filter((r) => r.isHome === true).length;
  if (homeCount === 0) {
    throw new Error(`Profile "${profileId}" starterApp must mark one route as home`);
  }
  if (homeCount > 1) {
    throw new Error(`Profile "${profileId}" starterApp has ${homeCount} home routes, expected 1`);
  }
}
