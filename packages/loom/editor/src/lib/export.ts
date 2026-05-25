// FSA export / import for remote (multi-user) workspaces. Phase 6 of
// `docs/dev/loom-multiuser.md` — the relay's CRDT is the canonical
// store; FSA roundtrips a workspace into / out of a local folder for
// offline backups, hand-off to a different editor, or one-shot
// imports from an existing on-disk project.
//
// Lives next to `fs.ts` (which still owns the local-root flow) but
// is wholly LoomDoc-keyed: callers hand in the active `LoomDoc` and
// a directory handle, and the helper walks the workspace's file
// graph against that handle.

import type { LoomDoc } from "@/loom-wasm/loom_wasm";

export interface ProgressEvent {
    path: string;
    /** 1-based index of the file currently being processed. */
    index: number;
    /** Total number of files in this run. */
    total: number;
}

export interface ExportOptions {
    /** Skip files whose path matches any of these prefixes (e.g. `node_modules/`). */
    skipPrefixes?: string[];
    /** Per-file progress callback. */
    onProgress?: (e: ProgressEvent) => void;
}

export interface ExportResult {
    filesWritten: number;
    skipped: string[];
}

/**
 * Write every file in `doc` under `root`, materialising nested paths
 * as a directory tree.
 */
export async function exportWorkspaceToFolder(
    doc: LoomDoc,
    root: FileSystemDirectoryHandle,
    opts: ExportOptions = {},
): Promise<ExportResult> {
    const paths = doc.listFiles();
    const skipped: string[] = [];
    let written = 0;

    for (let i = 0; i < paths.length; i++) {
        const path = String(paths[i]);
        if (shouldSkip(path, opts.skipPrefixes)) {
            skipped.push(path);
            continue;
        }
        const body = doc.getText(path);
        if (body == null) {
            skipped.push(path);
            continue;
        }
        const { parent, name } = await openParentForPath(root, path);
        const file = await parent.getFileHandle(name, { create: true });
        const writable = await file.createWritable();
        try {
            await writable.write(body);
        } finally {
            await writable.close();
        }
        written++;
        opts.onProgress?.({ path, index: i + 1, total: paths.length });
    }

    return { filesWritten: written, skipped };
}

export interface ImportOptions {
    /** Skip paths matching any of these prefixes (`.git/`, `node_modules/`, …). */
    skipPrefixes?: string[];
    /** Only import files whose name matches this RegExp. */
    include?: RegExp;
    /** Per-file progress callback. */
    onProgress?: (e: ProgressEvent) => void;
}

export interface ImportResult {
    filesRead: number;
    skipped: string[];
}

/**
 * Walk `root` recursively, reading each file into `doc.setText(path,
 * body)`. The path is relative to `root` (no leading `root.name`
 * segment) so importing a folder named "project" doesn't mirror the
 * folder name in the workspace.
 */
export async function importFolderIntoWorkspace(
    doc: LoomDoc,
    root: FileSystemDirectoryHandle,
    opts: ImportOptions = {},
): Promise<ImportResult> {
    // Two-pass: gather first so the progress callback has an honest
    // total, then write.
    const entries = await gatherFiles(root, "", opts);
    const skipped: string[] = [];
    let read = 0;

    for (let i = 0; i < entries.length; i++) {
        const { path, handle } = entries[i];
        try {
            const file = await handle.getFile();
            const body = await file.text();
            doc.setText(path, body);
            read++;
            opts.onProgress?.({ path, index: i + 1, total: entries.length });
        } catch {
            skipped.push(path);
        }
    }

    return { filesRead: read, skipped };
}

// ── internals ────────────────────────────────────────────────────

interface GatheredFile {
    path: string;
    handle: FileSystemFileHandle;
}

async function gatherFiles(
    dir: FileSystemDirectoryHandle,
    prefix: string,
    opts: ImportOptions,
): Promise<GatheredFile[]> {
    const out: GatheredFile[] = [];
    // FileSystemDirectoryHandle's iteration shape isn't yet in the
    // workspace TS lib; values() is an async iterator of handles.
    const iter = (
        dir as unknown as {
            values(): AsyncIterable<
                FileSystemFileHandle | FileSystemDirectoryHandle
            >;
        }
    ).values();
    for await (const entry of iter) {
        const childPath = prefix ? `${prefix}/${entry.name}` : entry.name;
        if (shouldSkip(childPath, opts.skipPrefixes)) continue;
        if (entry.kind === "directory") {
            out.push(
                ...(await gatherFiles(
                    entry as FileSystemDirectoryHandle,
                    childPath,
                    opts,
                )),
            );
        } else if (entry.kind === "file") {
            if (opts.include && !opts.include.test(entry.name)) continue;
            out.push({
                path: childPath,
                handle: entry as FileSystemFileHandle,
            });
        }
    }
    return out;
}

async function openParentForPath(
    root: FileSystemDirectoryHandle,
    path: string,
): Promise<{ parent: FileSystemDirectoryHandle; name: string }> {
    const segments = path.split("/").filter(Boolean);
    if (segments.length === 0) {
        throw new Error(`empty path: ${path}`);
    }
    const name = segments.pop() as string;
    let cursor = root;
    for (const seg of segments) {
        cursor = await cursor.getDirectoryHandle(seg, { create: true });
    }
    return { parent: cursor, name };
}

function shouldSkip(path: string, prefixes?: string[]): boolean {
    if (!prefixes || prefixes.length === 0) return false;
    return prefixes.some((p) => path === p || path.startsWith(p));
}
