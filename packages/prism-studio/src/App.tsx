import { useEffect, useState } from "react";
import type { Action } from "kbar";
import { LensProvider } from "@prism/core/shell";
import { PrismKBarProvider } from "@prism/core/kbar";
import { StudioShell } from "./components/studio-shell.js";
import {
  createStudioKernel,
  createBuiltinInitializers,
  KernelProvider,
} from "./kernel/index.js";
import {
  createBuiltinLensBundles,
  EDITOR_LENS_ID,
} from "./lenses/index.js";
import { NotificationToast } from "./components/notification-toast.js";

// ── Kernel (singleton for app lifetime) ─────────────────────────────────────
// The kernel is fully self-wiring: it installs the built-in lens bundles
// (registering their manifests + React components into its own lens
// registry and component map) and runs the built-in initializers (which
// seed templates + demo workspace). App.tsx just constructs it and pulls
// state off the resulting instance — no seeding helpers, no parallel
// lens-registry wiring at this layer.

const kernel = createStudioKernel({
  lensBundles: createBuiltinLensBundles(),
  initializers: createBuiltinInitializers(),
});

// ── App ─────────────────────────────────────────────────────────────────────

export function App() {
  const { lensRegistry, lensComponents, shellStore } = kernel;

  const [globalActions, setGlobalActions] = useState<Action[]>([]);

  // Derive KBar actions from the lens registry. Bundles were installed
  // inside createStudioKernel(), so by the time this effect runs the
  // registry is already populated — we just subscribe for future changes.
  useEffect(() => {
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

    return lensRegistry.subscribe(() => deriveActions());
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
          components={lensComponents}
        >
          <StudioShell />
          <NotificationToast />
        </LensProvider>
      </PrismKBarProvider>
    </KernelProvider>
  );
}
