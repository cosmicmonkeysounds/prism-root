// CodeMirror 6 ↔ `LoomDoc` binding.
//
// Round-trips text changes through a single `LoroText` keyed by `path`:
// CodeMirror transactions emit per-change `spliceText` calls into the
// doc, and a `LoomDoc.subscribe` callback dispatches Loro-side mutations
// back into the editor. Remote commits land as a *minimal* CodeMirror
// change (one replacement of the differing middle, computed via longest
// common prefix + suffix) so CM's built-in selection mapping preserves
// local cursors and selections across remote edits.

import {
    EditorState,
    StateEffect,
    StateField,
    type Extension,
} from "@codemirror/state";
import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";

import type { LoomDoc, SubscriptionHandle } from "../loom-wasm/loom_wasm";

/** Flip on a transaction to mark it as a remote-origin edit. */
const remoteOriginEffect = StateEffect.define<true>();

const isRemoteOrigin = StateField.define<boolean>({
    create: () => false,
    update(value, tr) {
        for (const e of tr.effects) if (e.is(remoteOriginEffect)) return true;
        return tr.docChanged ? false : value;
    },
});

export interface LoomBindingOptions {
    doc: LoomDoc;
    path: string;
}

/**
 * Returns a CodeMirror [`Extension`] that mirrors the given `path`'s
 * `LoroText` body in both directions. Drop the editor (or its plugin)
 * to detach.
 */
export function loomBinding(opts: LoomBindingOptions): Extension {
    return [isRemoteOrigin, viewPlugin(opts)];
}

function viewPlugin(opts: LoomBindingOptions) {
    return ViewPlugin.define((view) => new LoomBindingPlugin(view, opts), {
        provide: () => [],
    });
}

class LoomBindingPlugin {
    private handle: SubscriptionHandle;
    private applying = false;
    private readonly view: EditorView;
    private readonly opts: LoomBindingOptions;

    constructor(view: EditorView, opts: LoomBindingOptions) {
        this.view = view;
        this.opts = opts;
        // Seed the editor with whatever the doc has, then watch.
        const initial = opts.doc.getText(opts.path);
        if (initial != null && initial !== view.state.doc.toString()) {
            this.replaceEditorContents(initial);
        }
        this.handle = opts.doc.subscribe(() => this.onLoomCommit());
    }

    update(update: ViewUpdate): void {
        if (!update.docChanged) return;
        if (update.state.field(isRemoteOrigin, false)) return;
        if (this.applying) return;

        // Forward each CodeMirror change into the Loro doc.
        update.changes.iterChanges(
            (fromA, toA, _fromB, _toB, insertedText) => {
                this.opts.doc.spliceText(
                    this.opts.path,
                    fromA,
                    toA - fromA,
                    insertedText.toString(),
                );
            },
        );
    }

    private onLoomCommit(): void {
        const next = this.opts.doc.getText(this.opts.path) ?? "";
        const current = this.view.state.doc.toString();
        if (next === current) return;
        this.applyRemote(current, next);
    }

    /**
     * Apply a remote update as the minimal `(from, to, insert)`
     * change CodeMirror can absorb without disturbing the local
     * selection. Computes the longest matching prefix + suffix so
     * cursors anchored outside the changed region survive intact.
     */
    private applyRemote(current: string, next: string): void {
        const currLen = current.length;
        const nextLen = next.length;
        let prefix = 0;
        const maxPrefix = Math.min(currLen, nextLen);
        while (prefix < maxPrefix && current.charCodeAt(prefix) === next.charCodeAt(prefix)) {
            prefix++;
        }
        let suffix = 0;
        const maxSuffix = Math.min(currLen - prefix, nextLen - prefix);
        while (
            suffix < maxSuffix &&
            current.charCodeAt(currLen - 1 - suffix) === next.charCodeAt(nextLen - 1 - suffix)
        ) {
            suffix++;
        }
        const from = prefix;
        const to = currLen - suffix;
        const insert = next.slice(prefix, nextLen - suffix);
        this.applying = true;
        try {
            this.view.dispatch({
                changes: { from, to, insert },
                effects: remoteOriginEffect.of(true),
            });
        } finally {
            this.applying = false;
        }
    }

    private replaceEditorContents(next: string): void {
        this.applying = true;
        try {
            this.view.dispatch({
                changes: {
                    from: 0,
                    to: this.view.state.doc.length,
                    insert: next,
                },
                effects: remoteOriginEffect.of(true),
            });
        } finally {
            this.applying = false;
        }
    }

    destroy(): void {
        this.handle.unsubscribe();
    }
}

// Re-export for hosts that need to construct a state plus an extension
// in one go (matches the editor's existing `EditorState.create` pattern).
export { EditorState };
