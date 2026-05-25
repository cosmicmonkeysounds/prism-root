// `LoomProject` — the unification layer. A folder on disk becomes a
// "Loom Project" when it carries a `.loom-workspace.json` manifest.
// The manifest pins the folder to a remote workspace id (so the same
// folder always reconnects to the same collaborative document) and
// records a few authoring fields.
//
// The unification model is intentionally **Obsidian-shaped**:
//
//   * Files on disk are real files the user can edit with any tool.
//   * The CRDT (`LoomDoc`) is a live in-memory mirror of those files.
//   * The relay is the wire layer that fans CRDT deltas out to peers.
//
// `LoomFolderBridge` is the glue that keeps the LoomDoc and the
// folder in sync in both directions:
//
//   doc-side commit  →  write changed files to disk
//   filesystem edit  →  splice into the doc
//
// The relay subscription rides on top via the existing
// `LoomSyncClient` — the bridge owns nothing about networking.

import {
    createFsObserver,
    readFileText,
    writeFileText,
    type FsChangeRecord,
} from "./fs";
import type {
    LoomDoc,
    SubscriptionHandle,
} from "@/loom-wasm/loom_wasm";

const MANIFEST_NAME = ".loom-workspace.json";
const MANIFEST_VERSION = 1;

export interface LoomProjectManifest {
    version: typeof MANIFEST_VERSION;
    /** Workspace id on the relay this folder is linked to. */
    workspaceId: string | null;
    /** Relay origin that minted `workspaceId`. */
    relayUrl: string | null;
    /** Human-facing label; defaults to the folder name on init. */
    name: string;
    /** ISO timestamp. */
    createdAt: string;
}

export function defaultManifest(name: string): LoomProjectManifest {
    return {
        version: MANIFEST_VERSION,
        workspaceId: null,
        relayUrl: null,
        name,
        createdAt: new Date().toISOString(),
    };
}

/** Read `.loom-workspace.json`. Returns null when the file is absent. */
export async function readManifest(
    root: FileSystemDirectoryHandle,
): Promise<LoomProjectManifest | null> {
    let handle: FileSystemFileHandle;
    try {
        handle = await root.getFileHandle(MANIFEST_NAME);
    } catch {
        return null;
    }
    try {
        const file = await handle.getFile();
        const text = await file.text();
        const parsed = JSON.parse(text) as Partial<LoomProjectManifest>;
        return {
            version: MANIFEST_VERSION,
            workspaceId: parsed.workspaceId ?? null,
            relayUrl: parsed.relayUrl ?? null,
            name: parsed.name ?? root.name,
            createdAt: parsed.createdAt ?? new Date().toISOString(),
        };
    } catch {
        return null;
    }
}

/** Write `.loom-workspace.json`. Creates it if missing. */
export async function writeManifest(
    root: FileSystemDirectoryHandle,
    manifest: LoomProjectManifest,
): Promise<void> {
    const handle = await root.getFileHandle(MANIFEST_NAME, { create: true });
    await writeFileText(handle, JSON.stringify(manifest, null, 2) + "\n");
}

/**
 * Initialise the folder as a Loom Project — writes a default manifest
 * (workspaceId still null; link happens via `linkManifest`).
 */
export async function initManifest(
    root: FileSystemDirectoryHandle,
): Promise<LoomProjectManifest> {
    const existing = await readManifest(root);
    if (existing) return existing;
    const manifest = defaultManifest(root.name);
    await writeManifest(root, manifest);
    return manifest;
}

/** Update the workspace binding on the manifest. */
export async function linkManifest(
    root: FileSystemDirectoryHandle,
    workspaceId: string,
    relayUrl: string,
): Promise<LoomProjectManifest> {
    const current = (await readManifest(root)) ?? defaultManifest(root.name);
    const next: LoomProjectManifest = {
        ...current,
        workspaceId,
        relayUrl,
    };
    await writeManifest(root, next);
    return next;
}

/** Strip the workspace binding without removing the manifest. */
export async function unlinkManifest(
    root: FileSystemDirectoryHandle,
): Promise<LoomProjectManifest | null> {
    const current = await readManifest(root);
    if (!current) return null;
    const next: LoomProjectManifest = {
        ...current,
        workspaceId: null,
        relayUrl: null,
    };
    await writeManifest(root, next);
    return next;
}

// ── Folder ↔ Doc bridge ──────────────────────────────────────────

export interface BridgeOptions {
    /** Debounce for doc → folder writes, in ms. Default 250. */
    writeDebounceMs?: number;
    /** Paths matching any of these prefixes are skipped both ways. */
    skipPrefixes?: string[];
    /** Surface async errors (write fail, parse fail, …). */
    onError?: (err: Error) => void;
}

/**
 * Bidirectional sync between a folder and a `LoomDoc`. Construct once
 * per (folder, doc) pair after the folder has been seeded into the
 * doc (`seedDocFromFolder` does the first read).
 */
export class LoomFolderBridge {
    private readonly root: FileSystemDirectoryHandle;
    private readonly doc: LoomDoc;
    private readonly opts: Required<BridgeOptions>;
    private docSub: SubscriptionHandle | null = null;
    private fsObserver: ReturnType<typeof createFsObserver> = null;
    private flushTimer: ReturnType<typeof setTimeout> | null = null;
    /** Last-known `doc.getText(path)` we wrote to disk, for change detection. */
    private lastWritten = new Map<string, string>();
    /** Suppress feedback: while the bridge is writing the FS, we ignore observer events. */
    private writing = 0;

    constructor(
        root: FileSystemDirectoryHandle,
        doc: LoomDoc,
        opts: BridgeOptions = {},
    ) {
        this.root = root;
        this.doc = doc;
        this.opts = {
            writeDebounceMs: opts.writeDebounceMs ?? 250,
            skipPrefixes: opts.skipPrefixes ?? defaultSkipPrefixes,
            onError:
                opts.onError ??
                ((e) => {
                    console.error("[LoomFolderBridge]", e);
                }),
        };
    }

    /** Subscribe to both sides. Returns nothing — call `dispose` to detach. */
    start(): void {
        if (this.docSub) return;
        this.docSub = this.doc.subscribe(() => this.scheduleFlush());
        const observer = createFsObserver((records) =>
            this.onFsChanges(records),
        );
        if (observer) {
            this.fsObserver = observer;
            void observer
                .observe(this.root, { recursive: true })
                .catch((err) =>
                    this.opts.onError(
                        err instanceof Error
                            ? err
                            : new Error(String(err)),
                    ),
                );
        }
    }

    dispose(): void {
        this.docSub?.unsubscribe();
        this.docSub = null;
        if (this.fsObserver) {
            try {
                this.fsObserver.disconnect();
            } catch {
                /* ignore */
            }
            this.fsObserver = null;
        }
        if (this.flushTimer) {
            clearTimeout(this.flushTimer);
            this.flushTimer = null;
        }
    }

    /** Record `path` as already-on-disk to prevent the next flush from rewriting it. */
    markClean(path: string, body: string): void {
        this.lastWritten.set(path, body);
    }

    // ── doc → folder ─────────────────────────────────────────────

    private scheduleFlush(): void {
        if (this.flushTimer) return;
        this.flushTimer = setTimeout(() => {
            this.flushTimer = null;
            void this.flushDocToFolder();
        }, this.opts.writeDebounceMs);
    }

    private async flushDocToFolder(): Promise<void> {
        const paths = this.doc.listFiles().map((p) => String(p));
        const seen = new Set<string>();
        for (const path of paths) {
            seen.add(path);
            if (this.skipped(path)) continue;
            const body = this.doc.getText(path);
            if (body == null) continue;
            if (this.lastWritten.get(path) === body) continue;
            this.writing++;
            try {
                await writePathToFolder(this.root, path, body);
                this.lastWritten.set(path, body);
            } catch (err) {
                this.opts.onError(
                    err instanceof Error ? err : new Error(String(err)),
                );
            } finally {
                this.writing--;
            }
        }
        // Files removed from the doc since last flush — delete on disk.
        for (const path of Array.from(this.lastWritten.keys())) {
            if (seen.has(path)) continue;
            this.writing++;
            try {
                await deletePathInFolder(this.root, path);
            } catch (err) {
                this.opts.onError(
                    err instanceof Error ? err : new Error(String(err)),
                );
            } finally {
                this.writing--;
            }
            this.lastWritten.delete(path);
        }
    }

    // ── folder → doc ─────────────────────────────────────────────

    private async onFsChanges(records: FsChangeRecord[]): Promise<void> {
        if (this.writing > 0) return; // our own writes, skip
        for (const r of records) {
            const path = r.relativePathComponents.join("/");
            if (!path || this.skipped(path)) continue;
            if (r.type === "modified" || r.type === "appeared") {
                try {
                    const handle = await resolveFileHandle(this.root, path);
                    if (!handle) continue;
                    const fresh = await readFileText(handle);
                    if (this.lastWritten.get(path) === fresh) continue;
                    this.doc.setText(path, fresh);
                    this.lastWritten.set(path, fresh);
                } catch (err) {
                    this.opts.onError(
                        err instanceof Error
                            ? err
                            : new Error(String(err)),
                    );
                }
            } else if (r.type === "disappeared") {
                try {
                    this.doc.deleteFile(path);
                    this.lastWritten.delete(path);
                } catch {
                    /* doc may already lack the path; ignore */
                }
            }
        }
    }

    private skipped(path: string): boolean {
        if (path === MANIFEST_NAME) return true;
        return this.opts.skipPrefixes.some(
            (p) => path === p || path.startsWith(p),
        );
    }
}

/** Read every file in `root` (skipping the manifest + the skip set) and seed `doc.setText`. */
export async function seedDocFromFolder(
    root: FileSystemDirectoryHandle,
    doc: LoomDoc,
    opts: { skipPrefixes?: string[] } = {},
): Promise<void> {
    const skipPrefixes = opts.skipPrefixes ?? defaultSkipPrefixes;
    await walkAndSeed(root, "", doc, skipPrefixes);
}

const defaultSkipPrefixes = [
    ".git/",
    "node_modules/",
    "dist/",
    "target/",
    ".loom-data/",
];

async function walkAndSeed(
    dir: FileSystemDirectoryHandle,
    prefix: string,
    doc: LoomDoc,
    skipPrefixes: string[],
): Promise<void> {
    const iter = (
        dir as unknown as {
            values(): AsyncIterable<
                FileSystemFileHandle | FileSystemDirectoryHandle
            >;
        }
    ).values();
    for await (const entry of iter) {
        const path = prefix ? `${prefix}/${entry.name}` : entry.name;
        if (path === MANIFEST_NAME) continue;
        if (skipPrefixes.some((p) => path === p || path.startsWith(p)))
            continue;
        if (entry.kind === "directory") {
            await walkAndSeed(
                entry as FileSystemDirectoryHandle,
                path,
                doc,
                skipPrefixes,
            );
        } else {
            try {
                const body = await readFileText(entry as FileSystemFileHandle);
                doc.setText(path, body);
            } catch {
                /* ignore unreadable files */
            }
        }
    }
}

// ── path helpers ─────────────────────────────────────────────────

async function writePathToFolder(
    root: FileSystemDirectoryHandle,
    path: string,
    body: string,
): Promise<void> {
    const segments = path.split("/").filter(Boolean);
    const name = segments.pop();
    if (!name) throw new Error(`invalid path: ${path}`);
    let cursor = root;
    for (const seg of segments) {
        cursor = await cursor.getDirectoryHandle(seg, { create: true });
    }
    const handle = await cursor.getFileHandle(name, { create: true });
    await writeFileText(handle, body);
}

async function deletePathInFolder(
    root: FileSystemDirectoryHandle,
    path: string,
): Promise<void> {
    const segments = path.split("/").filter(Boolean);
    const name = segments.pop();
    if (!name) return;
    let cursor = root;
    for (const seg of segments) {
        try {
            cursor = await cursor.getDirectoryHandle(seg);
        } catch {
            return; // already gone
        }
    }
    try {
        await cursor.removeEntry(name);
    } catch {
        /* already gone */
    }
}

async function resolveFileHandle(
    root: FileSystemDirectoryHandle,
    path: string,
): Promise<FileSystemFileHandle | null> {
    const segments = path.split("/").filter(Boolean);
    const name = segments.pop();
    if (!name) return null;
    let cursor = root;
    for (const seg of segments) {
        try {
            cursor = await cursor.getDirectoryHandle(seg);
        } catch {
            return null;
        }
    }
    try {
        return await cursor.getFileHandle(name);
    } catch {
        return null;
    }
}
