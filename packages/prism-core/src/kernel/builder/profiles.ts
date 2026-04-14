/**
 * Built-in App Profiles for the self-replicating Studio.
 *
 * These map the four ecosystem apps (Flux, Lattice, Cadence, Grip) plus
 * the universal Studio host and a Relay-only profile into pinned
 * configurations the BuilderManager can consume.
 */

import type { AppProfile, BuiltInProfileId } from "./types.js";

export const STUDIO_PROFILE: AppProfile = {
  id: "studio",
  name: "Prism Studio",
  version: "0.1.0",
  // undefined plugins/lenses = all built-ins = universal host
  defaultLens: "editor",
  allowGlassFlip: true,
  theme: {
    displayName: "Prism Studio",
  },
  starterApp: {
    label: "Studio",
    description: "Universal host — all lenses, minimal chrome.",
    appShell: {
      data: {
        brand: "Prism Studio",
        brandIcon: "\u25A6",
        topBarHeight: 48,
        leftBarWidth: 220,
        rightBarWidth: 0,
        bottomBarHeight: 0,
        stickyTopBar: true,
      },
      children: [
        { type: "heading", slot: "topBar", name: "Brand", data: { text: "Prism Studio", level: "h3", align: "left" } },
        { type: "site-nav", slot: "leftBar", name: "Site nav", data: { orientation: "vertical", source: "pages" } },
      ],
    },
    defaultPageShell: {
      data: {
        topBarHeight: 0,
        leftBarWidth: 0,
        rightBarWidth: 0,
        bottomBarHeight: 0,
        stickyTopBar: false,
      },
    },
    routes: [
      { path: "/", label: "Home", pageTemplate: "landing", isHome: true, showInNav: true },
      { path: "/docs", label: "Docs", pageTemplate: "blog", showInNav: true },
    ],
  },
};

export const FLUX_PROFILE: AppProfile = {
  id: "flux",
  name: "Flux",
  version: "0.1.0",
  plugins: ["work", "finance", "crm"],
  lenses: [
    "editor",
    "canvas",
    "record-browser",
    "automation",
    "work",
    "finance",
    "crm",
  ],
  defaultLens: "record-browser",
  allowGlassFlip: true,
  theme: {
    primary: "#6C5CE7",
    displayName: "Flux",
    brandIcon: "flux.svg",
  },
  kbarCommands: ["new-task", "new-invoice", "new-contact", "start-timer"],
  starterApp: {
    label: "Flux",
    description: "Productivity — tasks, contacts, invoices in a record-browser chrome.",
    appShell: {
      data: {
        brand: "Flux",
        brandIcon: "\u26A1",
        topBarHeight: 56,
        leftBarWidth: 280,
        rightBarWidth: 0,
        bottomBarHeight: 0,
        stickyTopBar: true,
      },
      children: [
        { type: "heading", slot: "topBar", name: "Brand", data: { text: "Flux", level: "h2", align: "left" } },
        { type: "site-nav", slot: "leftBar", name: "Record nav", data: { orientation: "vertical", source: "pages", showIcons: true } },
      ],
    },
    defaultPageShell: {
      data: {
        topBarHeight: 0,
        leftBarWidth: 0,
        rightBarWidth: 320,
        bottomBarHeight: 0,
        stickyTopBar: false,
      },
    },
    routes: [
      { path: "/", label: "Inbox", pageTemplate: "blank", isHome: true, showInNav: true },
      { path: "/tasks", label: "Tasks", pageTemplate: "blank", showInNav: true },
      { path: "/contacts", label: "Contacts", pageTemplate: "blank", showInNav: true },
      { path: "/invoices", label: "Invoices", pageTemplate: "blank", showInNav: true },
    ],
  },
};

export const LATTICE_PROFILE: AppProfile = {
  id: "lattice",
  name: "Lattice",
  version: "0.1.0",
  plugins: ["assets", "platform"],
  lenses: [
    "editor",
    "graph",
    "spatial-canvas",
    "visual-script",
    "luau-facet",
    "assets-mgmt",
  ],
  defaultLens: "graph",
  allowGlassFlip: true,
  theme: {
    primary: "#00B894",
    displayName: "Lattice",
    brandIcon: "lattice.svg",
  },
  kbarCommands: ["new-dialogue", "compile-bank", "open-entity"],
  starterApp: {
    label: "Lattice",
    description: "Game middleware — graph-first editor with minimal chrome.",
    appShell: {
      data: {
        brand: "Lattice",
        brandIcon: "\u2B22",
        topBarHeight: 40,
        leftBarWidth: 0,
        rightBarWidth: 300,
        bottomBarHeight: 120,
        stickyTopBar: true,
      },
      children: [
        { type: "heading", slot: "topBar", name: "Brand", data: { text: "Lattice", level: "h3", align: "left" } },
      ],
    },
    defaultPageShell: {
      data: {
        topBarHeight: 0,
        leftBarWidth: 0,
        rightBarWidth: 0,
        bottomBarHeight: 0,
        stickyTopBar: false,
      },
    },
    routes: [
      { path: "/", label: "Graph", pageTemplate: "blank", isHome: true, showInNav: true },
      { path: "/assets", label: "Assets", pageTemplate: "blank", showInNav: true },
      { path: "/scripts", label: "Scripts", pageTemplate: "blank", showInNav: true },
    ],
  },
};

export const CADENCE_PROFILE: AppProfile = {
  id: "cadence",
  name: "Cadence",
  version: "0.1.0",
  plugins: ["life", "platform"],
  lenses: [
    "editor",
    "canvas",
    "luau-facet",
    "record-browser",
    "life",
  ],
  defaultLens: "canvas",
  allowGlassFlip: true,
  theme: {
    primary: "#FD79A8",
    displayName: "Cadence",
    brandIcon: "cadence.svg",
  },
  kbarCommands: ["new-lesson", "transcribe-session", "open-course"],
  starterApp: {
    label: "Cadence",
    description: "Music education — lesson canvas with hero landing.",
    appShell: {
      data: {
        brand: "Cadence",
        brandIcon: "\u266B",
        topBarHeight: 72,
        leftBarWidth: 0,
        rightBarWidth: 0,
        bottomBarHeight: 0,
        stickyTopBar: true,
      },
      children: [
        { type: "heading", slot: "topBar", name: "Brand", data: { text: "Cadence", level: "h1", align: "center" } },
      ],
    },
    defaultPageShell: {
      data: {
        topBarHeight: 0,
        leftBarWidth: 0,
        rightBarWidth: 0,
        bottomBarHeight: 0,
        stickyTopBar: false,
      },
    },
    routes: [
      { path: "/", label: "Welcome", pageTemplate: "landing", isHome: true, showInNav: true },
      { path: "/lessons", label: "Lessons", pageTemplate: "blank", showInNav: true },
      { path: "/courses", label: "Courses", pageTemplate: "blank", showInNav: true },
      { path: "/about", label: "About", pageTemplate: "blog", showInNav: true },
    ],
  },
};

export const GRIP_PROFILE: AppProfile = {
  id: "grip",
  name: "Grip",
  version: "0.1.0",
  plugins: ["work", "assets", "platform"],
  lenses: [
    "editor",
    "graph",
    "spatial-canvas",
    "canvas",
    "automation",
    "work",
    "assets-mgmt",
  ],
  defaultLens: "spatial-canvas",
  allowGlassFlip: true,
  theme: {
    primary: "#E17055",
    displayName: "Grip",
    brandIcon: "grip.svg",
  },
  kbarCommands: ["new-cue", "open-stage-plot", "arm-transport"],
  starterApp: {
    label: "Grip",
    description: "Live production — cue list driven spatial canvas.",
    appShell: {
      data: {
        brand: "Grip",
        brandIcon: "\u2726",
        topBarHeight: 48,
        leftBarWidth: 320,
        rightBarWidth: 0,
        bottomBarHeight: 160,
        stickyTopBar: true,
      },
      children: [
        { type: "heading", slot: "topBar", name: "Brand", data: { text: "Grip", level: "h2", align: "left" } },
      ],
    },
    defaultPageShell: {
      data: {
        topBarHeight: 0,
        leftBarWidth: 0,
        rightBarWidth: 0,
        bottomBarHeight: 0,
        stickyTopBar: false,
      },
    },
    routes: [
      { path: "/", label: "Stage", pageTemplate: "blank", isHome: true, showInNav: true },
      { path: "/cues", label: "Cues", pageTemplate: "blank", showInNav: true },
      { path: "/assets", label: "Assets", pageTemplate: "blank", showInNav: true },
    ],
  },
};

export const RELAY_PROFILE: AppProfile = {
  id: "relay",
  name: "Prism Relay",
  version: "0.1.0",
  // Relay has no plugins or lenses — it's a headless server profile.
  plugins: [],
  lenses: [],
  relayModules: [
    "blind-mailbox",
    "relay-router",
    "relay-timestamp",
    "capability-tokens",
    "sovereign-portals",
    "webrtc-signaling",
  ],
  theme: { displayName: "Prism Relay" },
  allowGlassFlip: false,
};

export const BUILT_IN_PROFILES: Record<BuiltInProfileId, AppProfile> = {
  studio: STUDIO_PROFILE,
  flux: FLUX_PROFILE,
  lattice: LATTICE_PROFILE,
  cadence: CADENCE_PROFILE,
  grip: GRIP_PROFILE,
  relay: RELAY_PROFILE,
};

export function listBuiltInProfiles(): AppProfile[] {
  return Object.values(BUILT_IN_PROFILES);
}

export function getBuiltInProfile(id: BuiltInProfileId): AppProfile {
  return BUILT_IN_PROFILES[id];
}

/** Serialize a profile to canonical `.prism-app.json` text. */
export function serializeAppProfile(profile: AppProfile): string {
  return `${JSON.stringify(profile, null, 2)}\n`;
}

/** Parse a `.prism-app.json` file. Throws on invalid shape. */
export function parseAppProfile(source: string): AppProfile {
  const parsed: unknown = JSON.parse(source);
  if (!parsed || typeof parsed !== "object") {
    throw new Error("App profile must be a JSON object");
  }
  const obj = parsed as Record<string, unknown>;
  if (typeof obj.id !== "string" || typeof obj.name !== "string" || typeof obj.version !== "string") {
    throw new Error("App profile requires id/name/version strings");
  }
  return parsed as AppProfile;
}
