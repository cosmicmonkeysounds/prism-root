/**
 * CodeMirror 6 ↔ LoroText synchronization extension.
 *
 * CodeMirror is NOT the source of truth — Loro is. This extension:
 * 1. Intercepts CM transactions → applies them to LoroText
 * 2. Listens for external Loro changes → updates CM state
 *
 * The hidden buffer is always the Loro CRDT. CodeMirror is a projection.
 */

import {
  StateField,
  StateEffect,
  Transaction,
  type Extension,
  type ChangeSpec,
} from "@codemirror/state";
import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { LoroDoc, LoroText } from "loro-crdt";

/** Effect to push external Loro changes into CM without re-dispatching to Loro. */
const loroRemoteChange = StateEffect.define<ChangeSpec>();

/** Tracks whether we're currently applying a Loro change to avoid feedback loops. */
const applyingRemote = StateField.define<boolean>({
  create: () => false,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(loroRemoteChange)) return true;
    }
    return false;
  },
});

export type LoroSyncConfig = {
  doc: LoroDoc;
  text: LoroText;
};

/**
 * Creates a CodeMirror extension that bidirectionally syncs with a LoroText.
 *
 * Usage:
 * ```ts
 * const doc = new LoroDoc();
 * const text = doc.getText("content");
 * const extensions = [loroSync({ doc, text }), ...otherExtensions];
 * ```
 */
export function loroSync(config: LoroSyncConfig): Extension {
  const { doc, text } = config;

  const plugin = ViewPlugin.fromClass(
    class {
      private unsubscribe: (() => void) | null = null;
      private isApplyingRemote = false;

      constructor(private view: EditorView) {
        // Subscribe to Loro text changes (from imports, merges, other tabs)
        const sub = text.subscribe((_event) => {
          if (this.isApplyingRemote) return;

          // Rebuild CM content from Loro text on external changes
          const loroContent = text.toString();
          const cmContent = this.view.state.doc.toString();

          if (loroContent !== cmContent) {
            this.isApplyingRemote = true;
            const changes: ChangeSpec = {
              from: 0,
              to: cmContent.length,
              insert: loroContent,
            };
            this.view.dispatch({
              changes,
              effects: loroRemoteChange.of(changes),
              annotations: [Transaction.remote.of(true)],
            });
            this.isApplyingRemote = false;
          }
        });

        this.unsubscribe = () => sub();
      }

      update(update: ViewUpdate) {
        if (this.isApplyingRemote) return;

        // Only process local user edits, not our own remote application
        if (!update.docChanged) return;
        const isRemote = update.transactions.some((tr) =>
          tr.effects.some((e) => e.is(loroRemoteChange)),
        );
        if (isRemote) return;

        // Apply CM changes to LoroText
        update.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
          // Delete
          if (toA > fromA) {
            text.delete(fromA, toA - fromA);
          }
          // Insert
          const insertText = inserted.toString();
          if (insertText.length > 0) {
            text.insert(fromA, insertText);
          }
        });
        doc.commit();
      }

      destroy() {
        this.unsubscribe?.();
      }
    },
  );

  return [applyingRemote, plugin];
}

/**
 * Create a LoroDoc with a named LoroText, pre-populated with initial content.
 * Convenience helper for creating editor documents.
 */
export function createLoroTextDoc(
  textId: string,
  initialContent?: string,
): { doc: LoroDoc; text: LoroText } {
  const doc = new LoroDoc();
  const text = doc.getText(textId);
  if (initialContent) {
    text.insert(0, initialContent);
    doc.commit();
  }
  return { doc, text };
}
