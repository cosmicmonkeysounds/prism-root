import { useMemo, useEffect, useState } from "react";
import type { Action } from "kbar";
import { createLensRegistry, createShellStore } from "@prism/core/workspace";
import { LensProvider } from "@prism/core/shell";
import { PrismKBarProvider } from "@prism/core/kbar";
import { StudioShell } from "./components/studio-shell.js";
import {
  createStudioKernel,
  KernelProvider,
} from "./kernel/index.js";
import type { StudioKernel } from "./kernel/index.js";
import {
  registerBuiltinLenses,
  createLensComponentMap,
  EDITOR_LENS_ID,
} from "./lenses/index.js";
import { NotificationToast } from "./components/notification-toast.js";

// ── Kernel (singleton for app lifetime) ─────────────────────────────────────

const kernel = createStudioKernel();
seedDemoData(kernel);

function seedDemoData(k: StudioKernel) {
  // Only seed if store is empty
  if (k.store.objectCount() > 0) return;

  const page = k.createObject({
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

  k.createObject({
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

  const contentSection = k.createObject({
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

  k.createObject({
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

  k.createObject({
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

  k.createObject({
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

  // Clear undo history so seed data isn't undoable
  k.undo.clear();

  // Select the home page by default
  k.select(page.id);
}

// ── App ─────────────────────────────────────────────────────────────────────

export function App() {
  const lensRegistry = useMemo(() => createLensRegistry(), []);
  const shellStore = useMemo(() => createShellStore(), []);
  const components = useMemo(() => createLensComponentMap(), []);

  const [globalActions, setGlobalActions] = useState<Action[]>([]);

  // Register built-in lenses and derive KBar actions
  useEffect(() => {
    const unregister = registerBuiltinLenses(lensRegistry);

    function deriveActions(): void {
      const lensActions: Action[] = lensRegistry.allLenses().map((m) => {
        const action: Action = {
          id: `switch-${m.id}`,
          name: `Switch to ${m.name}`,
          perform: () => shellStore.getState().openTab(m.id, m.name),
          section: "Navigation",
        };
        const shortcut = m.contributes.commands[0]?.shortcut;
        if (shortcut) action.shortcut = shortcut;
        return action;
      });

      // Add undo/redo actions
      lensActions.push(
        {
          id: "undo",
          name: "Undo",
          shortcut: ["$mod+z"],
          perform: () => kernel.undo.canUndo && kernel.undo.undo(),
          section: "Edit",
        },
        {
          id: "redo",
          name: "Redo",
          shortcut: ["$mod+shift+z"],
          perform: () => kernel.undo.canRedo && kernel.undo.redo(),
          section: "Edit",
        },
      );

      setGlobalActions(lensActions);
    }
    deriveActions();

    const unsubscribe = lensRegistry.subscribe(() => deriveActions());

    return () => {
      unregister();
      unsubscribe();
    };
  }, [lensRegistry, shellStore]);

  // Open default editor tab
  useEffect(() => {
    if (shellStore.getState().tabs.length === 0) {
      shellStore.getState().openTab(EDITOR_LENS_ID, "Editor");
    }
  }, [shellStore]);

  // Wire global keyboard shortcuts for undo/redo
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key === "z" && !e.shiftKey) {
        e.preventDefault();
        if (kernel.undo.canUndo) kernel.undo.undo();
      } else if (mod && e.key === "z" && e.shiftKey) {
        e.preventDefault();
        if (kernel.undo.canRedo) kernel.undo.redo();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  return (
    <KernelProvider kernel={kernel}>
      <PrismKBarProvider globalActions={globalActions}>
        <LensProvider
          registry={lensRegistry}
          store={shellStore}
          components={components}
        >
          <StudioShell />
          <NotificationToast />
        </LensProvider>
      </PrismKBarProvider>
    </KernelProvider>
  );
}
