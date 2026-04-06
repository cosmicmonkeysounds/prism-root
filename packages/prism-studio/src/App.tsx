import { useMemo, useEffect, useState } from "react";
import type { Action } from "kbar";
import type { ObjectTemplate } from "@prism/core/template";
import { createLensRegistry, createShellStore } from "@prism/core/lens";
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
registerSeedTemplates(kernel);
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

function registerSeedTemplates(k: StudioKernel) {
  const blogPage: ObjectTemplate = {
    id: "blog-page",
    name: "Blog Post",
    description: "A page with hero section, heading, and body text",
    category: "page",
    createdAt: new Date().toISOString(),
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
    variables: [
      { name: "title", label: "Page Title", required: true },
    ],
  };

  const landingPage: ObjectTemplate = {
    id: "landing-page",
    name: "Landing Page",
    description: "Hero + features + CTA sections",
    category: "page",
    createdAt: new Date().toISOString(),
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
    variables: [
      { name: "title", label: "Page Title", required: true },
    ],
  };

  k.registerTemplate(blogPage);
  k.registerTemplate(landingPage);
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

  // Open default editor tab and show inspector
  useEffect(() => {
    if (shellStore.getState().tabs.length === 0) {
      shellStore.getState().openTab(EDITOR_LENS_ID, "Editor");
    }
    if (!shellStore.getState().panelLayout.inspector) {
      shellStore.getState().toggleInspector();
    }
  }, [shellStore]);

  // Wire global keyboard shortcuts for undo/redo and clipboard
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      // Skip when user is typing in an input/textarea/contenteditable
      const tag = (e.target as HTMLElement)?.tagName;
      const isEditable = tag === "INPUT" || tag === "TEXTAREA" || (e.target as HTMLElement)?.isContentEditable;

      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key === "z" && !e.shiftKey) {
        e.preventDefault();
        if (kernel.undo.canUndo) kernel.undo.undo();
      } else if (mod && e.key === "z" && e.shiftKey) {
        e.preventDefault();
        if (kernel.undo.canRedo) kernel.undo.redo();
      } else if (mod && e.key === "c" && !isEditable) {
        const sel = kernel.atoms.getState().selectedId;
        if (sel) {
          e.preventDefault();
          kernel.clipboardCopy([sel]);
          kernel.notifications.add({ title: "Copied", kind: "info" });
        }
      } else if (mod && e.key === "x" && !isEditable) {
        const sel = kernel.atoms.getState().selectedId;
        if (sel) {
          e.preventDefault();
          kernel.clipboardCut([sel]);
          kernel.notifications.add({ title: "Cut", kind: "info" });
        }
      } else if (mod && e.key === "v" && !isEditable) {
        if (kernel.clipboardHasContent) {
          e.preventDefault();
          const sel = kernel.atoms.getState().selectedId;
          const result = kernel.clipboardPaste(sel);
          if (result) {
            kernel.notifications.add({
              title: `Pasted ${result.created.length} object(s)`,
              kind: "success",
            });
          }
        }
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
