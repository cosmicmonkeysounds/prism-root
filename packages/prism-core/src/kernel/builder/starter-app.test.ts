import { describe, it, expect } from "vitest";
import {
  materializeStarterApp,
  type StarterCreateObjectFn,
  type StarterCreateObjectInput,
} from "./starter-app.js";
import {
  STUDIO_PROFILE,
  FLUX_PROFILE,
  LATTICE_PROFILE,
  CADENCE_PROFILE,
  GRIP_PROFILE,
  RELAY_PROFILE,
} from "./profiles.js";
import type { AppProfile } from "./types.js";

interface CreatedRecord extends StarterCreateObjectInput {
  id: string;
}

/** Minimal fake store for the materialiser. Captures every create. */
function createFakeStore(): { createObject: StarterCreateObjectFn; records: CreatedRecord[] } {
  const records: CreatedRecord[] = [];
  let nextId = 1;
  const createObject: StarterCreateObjectFn = (input) => {
    const id = `obj-${nextId++}`;
    records.push({ ...input, id });
    return { id };
  };
  return { createObject, records };
}

describe("materializeStarterApp", () => {
  it("throws when the profile has no starterApp template", () => {
    const { createObject } = createFakeStore();
    expect(() => materializeStarterApp(RELAY_PROFILE, createObject)).toThrow(
      /no starterApp template/,
    );
  });

  it("throws when no route is marked as home", () => {
    const { createObject } = createFakeStore();
    const badProfile: AppProfile = {
      ...STUDIO_PROFILE,
      id: "bad",
      starterApp: {
        label: "Bad",
        appShell: { data: {} },
        defaultPageShell: { data: {} },
        routes: [{ path: "/", label: "R", pageTemplate: "blank" }],
      },
    };
    expect(() => materializeStarterApp(badProfile, createObject)).toThrow(
      /must mark one route as home/,
    );
  });

  it("throws when more than one route is marked as home", () => {
    const { createObject } = createFakeStore();
    const badProfile: AppProfile = {
      ...STUDIO_PROFILE,
      id: "double-home",
      starterApp: {
        label: "Bad",
        appShell: { data: {} },
        defaultPageShell: { data: {} },
        routes: [
          { path: "/", label: "A", pageTemplate: "blank", isHome: true },
          { path: "/b", label: "B", pageTemplate: "blank", isHome: true },
        ],
      },
    };
    expect(() => materializeStarterApp(badProfile, createObject)).toThrow(
      /2 home routes/,
    );
  });

  it("creates an app root, one app-shell, one route+page per entry, plus a page-shell per page", () => {
    const { createObject, records } = createFakeStore();
    const result = materializeStarterApp(STUDIO_PROFILE, createObject);

    const app = records.find((r) => r.type === "app");
    expect(app).toBeDefined();
    expect(app?.parentId).toBeNull();
    expect(result.appId).toBe(app?.id);

    const appShells = records.filter((r) => r.type === "app-shell");
    expect(appShells).toHaveLength(1);
    expect(appShells[0]?.parentId).toBe(result.appId);
    expect(result.appShellId).toBe(appShells[0]?.id);

    const routes = records.filter((r) => r.type === "route");
    expect(routes.length).toBe(STUDIO_PROFILE.starterApp!.routes.length);

    const pages = records.filter((r) => r.type === "page");
    expect(pages.length).toBe(STUDIO_PROFILE.starterApp!.routes.length);

    const pageShells = records.filter((r) => r.type === "page-shell");
    expect(pageShells.length).toBe(pages.length);

    // Every page-shell's parent must be a page.
    for (const shell of pageShells) {
      const parentIsPage = pages.some((p) => p.id === shell.parentId);
      expect(parentIsPage).toBe(true);
    }

    // routeToPageId must map every route to an actual page object.
    for (const [routeId, pageId] of Object.entries(result.routeToPageId)) {
      const routeObj = routes.find((r) => r.id === routeId);
      const pageObj = pages.find((p) => p.id === pageId);
      expect(routeObj).toBeDefined();
      expect(pageObj).toBeDefined();
      expect(routeObj?.data["pageId"]).toBe(pageId);
    }
  });

  it("picks the home route from the template", () => {
    const { createObject } = createFakeStore();
    const result = materializeStarterApp(FLUX_PROFILE, createObject);

    // Flux marks `/` (first) as home.
    expect(result.routeIds[0]).toBe(result.homeRouteId);
  });

  it("tags shell children with the correct __slot field", () => {
    const { createObject, records } = createFakeStore();
    materializeStarterApp(FLUX_PROFILE, createObject);

    const appShellChildren = records.filter(
      (r) => records.find((p) => p.id === r.parentId && p.type === "app-shell"),
    );
    expect(appShellChildren.length).toBeGreaterThan(0);
    for (const child of appShellChildren) {
      expect(typeof child.data["__slot"]).toBe("string");
    }
  });

  it("seeds landing page bodies with a hero + text-block in the page-shell main slot", () => {
    const { createObject, records } = createFakeStore();
    materializeStarterApp(STUDIO_PROFILE, createObject); // home route uses `landing`

    const heroes = records.filter(
      (r) => r.type === "hero" && r.data["__slot"] === "main",
    );
    expect(heroes.length).toBeGreaterThan(0);
  });

  it("seeds blog page bodies with a heading + text-block", () => {
    const { createObject, records } = createFakeStore();
    materializeStarterApp(STUDIO_PROFILE, createObject); // has a /docs route with `blog`

    const headings = records.filter(
      (r) => r.type === "heading" && r.data["__slot"] === "main",
    );
    expect(headings.length).toBeGreaterThan(0);
  });

  it("leaves blank page bodies empty", () => {
    const { createObject, records } = createFakeStore();
    materializeStarterApp(FLUX_PROFILE, createObject); // all blank routes

    const nonShellPageChildren = records.filter((r) => {
      const parent = records.find((p) => p.id === r.parentId);
      return parent?.type === "page-shell" && (r.data["__slot"] ?? null);
    });
    // Flux is all blank templates — no hero/heading seeded by the materialiser.
    expect(nonShellPageChildren.filter((r) => r.type === "hero")).toHaveLength(0);
    expect(nonShellPageChildren.filter((r) => r.type === "heading")).toHaveLength(0);
  });

  it("copies profile theme primary color onto the app object", () => {
    const { createObject, records } = createFakeStore();
    materializeStarterApp(FLUX_PROFILE, createObject);
    const app = records.find((r) => r.type === "app");
    expect(app?.data["themePrimary"]).toBe(FLUX_PROFILE.theme?.primary);
  });

  it("works end-to-end for every profile that ships a template", () => {
    const profiles: AppProfile[] = [
      STUDIO_PROFILE,
      FLUX_PROFILE,
      LATTICE_PROFILE,
      CADENCE_PROFILE,
      GRIP_PROFILE,
    ];
    for (const profile of profiles) {
      const { createObject } = createFakeStore();
      const result = materializeStarterApp(profile, createObject);
      expect(result.appId).toBeTruthy();
      expect(result.appShellId).toBeTruthy();
      expect(result.homeRouteId).toBeTruthy();
      expect(result.routeIds.length).toBeGreaterThan(0);
    }
  });
});
