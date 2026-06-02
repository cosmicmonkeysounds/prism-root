// `useSession` — Zustand store owning the multi-user backbone state:
// relay URL, authentication, the live `LoomSyncClient`, and the active
// workspace's `LoomDoc`. Sits alongside the existing FSA-rooted
// workspace store (`store/workspace.ts`); host components opt into
// the cloud flow by reading from this store instead.
//
// The actual CodeMirror binding lives in `components/cloud/RemoteEditor.tsx`
// via the existing `loomBinding` extension — the store just owns the
// wiring lifecycle (connect, subscribe, refresh, teardown).

import { create } from "zustand";
import {
    LoomAuthClient,
    loadSessionToken,
    loadUsername,
} from "@/lib/auth";
import { PresenceTracker, type PresenceState } from "@/lib/presence";
import { LoomSyncClient, type PlayFile, type PlayStatePayload } from "@/lib/sync";
import { LoomWorkspaceClient, wsUrlFromRelay } from "@/lib/workspaces";
import type { WorkspaceMeta } from "@/lib/workspaces";
import type { LoomDoc } from "@/loom-wasm/loom_wasm";
import {
    LoomFolderBridge,
    linkManifest,
    readManifest,
    seedDocFromFolder,
    unlinkManifest,
    type LoomProjectManifest,
} from "@/lib/project";
import { LocalPlayEngine } from "@/lib/local-play";
import {
    EXAMPLE_LABEL,
    EXAMPLE_PATH,
    EXAMPLE_SOURCE,
} from "@/lib/example-project";
import { useWorkspace } from "@/store/workspace";
import { readFileText, type FsEntry } from "@/lib/fs";

const FALLBACK_RELAY = "http://127.0.0.1:7878";
const RELAY_KEY = "loom.relayUrl";

/**
 * Default relay URL. Phase 8 — when the editor is served by the relay
 * itself (single-binary deployment) the relay lives at the same origin
 * as the page, so we point at `window.location.origin`. When we're
 * loaded from a Vite dev server (`:5173` / `:4173`) or somewhere else
 * that obviously isn't the relay, we fall back to the canonical
 * `127.0.0.1:7878` so `prism loom dev` works out of the box.
 */
function defaultRelayUrl(): string {
    // `prism loom dev` (and any other launcher) can pin the relay URL
    // explicitly via `VITE_LOOM_RELAY`. Highest precedence — covers
    // non-default ports and cross-machine setups.
    const envUrl = import.meta.env?.VITE_LOOM_RELAY as string | undefined;
    if (typeof envUrl === "string" && envUrl) return envUrl;
    if (typeof window === "undefined") return FALLBACK_RELAY;
    try {
        const origin = window.location.origin;
        if (!origin || origin === "null") return FALLBACK_RELAY;
        const url = new URL(origin);
        // Anything that looks like a Vite/preview dev server — fall
        // through to the canonical local relay.
        const devPorts = new Set(["5173", "4173", "3000", "8080"]);
        if (devPorts.has(url.port)) return FALLBACK_RELAY;
        if (url.protocol !== "http:" && url.protocol !== "https:") {
            return FALLBACK_RELAY;
        }
        return origin;
    } catch {
        return FALLBACK_RELAY;
    }
}

export type SessionStatus =
    | "idle"
    | "authenticating"
    | "authenticated"
    | "connecting"
    | "connected"
    | "error";

/**
 * `local` = a single-user, in-browser workspace whose play session runs
 * client-side via the wasm `LoomSession` (no relay, no account).
 * `cloud` = a relay-hosted, collaborative workspace whose play session
 * runs server-side over the WebSocket. Both flavours expose the same
 * `play` shape so every Runner panel is agnostic.
 */
export type WorkspaceKind = "local" | "cloud";

export interface ActiveWorkspace {
    kind: WorkspaceKind;
    meta: WorkspaceMeta;
    doc: LoomDoc;
    files: string[];
    activePath: string | null;
    peers: PresenceState[];
    /** Local folder bound to this workspace, when one is linked. */
    linkedFolder: LinkedFolder | null;
    /** Current play session (server- or client-hosted), if any. */
    play: PlayStatePayload | null;
}

export interface LinkedFolder {
    root: FileSystemDirectoryHandle;
    manifest: LoomProjectManifest;
}

interface SessionState {
    relayUrl: string;
    status: SessionStatus;
    error: string | null;
    username: string | null;
    token: string | null;

    workspaces: WorkspaceMeta[];
    active: ActiveWorkspace | null;

    // Long-lived clients (lazily constructed when first needed).
    sync: LoomSyncClient | null;
    presence: PresenceTracker;

    setRelayUrl: (url: string) => void;
    register: (username: string, password: string) => Promise<void>;
    login: (username: string, password: string) => Promise<void>;
    logout: () => void;

    /** Establish (or reuse) the WebSocket connection. */
    connect: () => Promise<void>;

    /** Pull the workspace list via REST. */
    refreshWorkspaces: () => Promise<void>;

    /** Mint a new workspace + open it. */
    createWorkspace: (name: string) => Promise<void>;

    /** Delete the given workspace (must be owner). */
    removeWorkspace: (id: string) => Promise<void>;

    /** Make `id` the active remote workspace. */
    openWorkspace: (id: string) => Promise<void>;

    /**
     * Ensure a local (in-browser) workspace is active so play works
     * without a relay. Targets the open local folder's `.loom` files,
     * falling back to the bundled example. No-op when a cloud workspace
     * is active, or when already on the same local target.
     */
    activateLocal: () => Promise<void>;

    /** Detach from the active workspace (keeps connection). */
    closeWorkspace: () => void;

    /** Refresh `active.files` from the underlying doc. */
    refreshActiveFiles: () => void;

    /** Set the active file path inside the open workspace. */
    setActivePath: (path: string | null) => void;

    /** Create an empty `.loom` file inside the active workspace. */
    createFile: (path: string) => void;

    /**
     * Bind a local folder to the active workspace. Writes a manifest
     * into the folder, seeds the doc from on-disk files, and starts
     * the bidirectional bridge. Idempotent.
     */
    bindFolder: (root: FileSystemDirectoryHandle) => Promise<void>;

    /** Tear down the bridge + clear the manifest's workspace binding. */
    unbindFolder: () => Promise<void>;

    /** Phase 7 — start a play session against the active workspace. */
    startPlay: () => void;
    /** Phase 7 — advance an active session by choice index. */
    sendChoice: (index: number, head?: string) => void;
    /** Phase 7 — tear the active session down. */
    stopPlay: () => void;
    // Phase 4 (Loom IDE redesign §5): branching controls.
    forkPlay: (opts?: { parent?: string; fromSnapshot?: string }) => void;
    snapshotPlay: (opts?: { head?: string; label?: string }) => void;
    restorePlay: (head: string, snapshot: string) => void;
    dropHead: (head: string) => void;
    setPrimaryHead: (head: string) => void;
    // Booth live-patch.
    boothSkip: (head?: string) => void;
    boothForce: (raw: string, head?: string) => void;
    boothReload: () => void;
}

let wasmPromise: Promise<typeof import("@/loom-wasm/loom_wasm")> | null = null;
async function loadWasm(): Promise<typeof import("@/loom-wasm/loom_wasm")> {
    if (!wasmPromise) {
        wasmPromise = (async () => {
            const mod = await import("@/loom-wasm/loom_wasm");
            await mod.default();
            return mod;
        })().catch((err) => {
            wasmPromise = null;
            throw err;
        });
    }
    return wasmPromise;
}

function persistedRelay(): string {
    try {
        return localStorage.getItem(RELAY_KEY) ?? defaultRelayUrl();
    } catch {
        return defaultRelayUrl();
    }
}

// ── local-workspace helpers ───────────────────────────────────────────
//
// The local play engine lives on the active record alongside the cloud
// `__bridge` / `__cleanup` so it tears down with the workspace.
type WithEngine = ActiveWorkspace & { __engine?: LocalPlayEngine };

function engineOf(active: ActiveWorkspace | null): LocalPlayEngine | undefined {
    return (active as WithEngine | null)?.__engine;
}

/** Recursively collect every `.loom` file in an FS tree. */
function collectLoomEntries(
    entry: FsEntry | null | undefined,
    out: FsEntry[] = [],
): FsEntry[] {
    if (!entry) return out;
    if (entry.kind === "file") {
        if (entry.path.toLowerCase().endsWith(".loom")) out.push(entry);
    } else {
        for (const child of entry.children ?? []) collectLoomEntries(child, out);
    }
    return out;
}

/** Strip the workspace root-name prefix so bundle paths are project-relative. */
function projectRelative(path: string, rootName: string): string {
    const prefix = `${rootName}/`;
    return path.startsWith(prefix) ? path.slice(prefix.length) : path;
}

/**
 * Cheap, synchronous description of the current local play target — its
 * stable id (so activation is idempotent) and `.loom` paths (so the
 * "Start play" enabled check has something to count). No disk reads.
 */
function localTarget(): { id: string; name: string; paths: string[] } {
    const ws = useWorkspace.getState();
    const root = ws.root;
    const entries = collectLoomEntries(root);
    if (root && entries.length > 0) {
        return {
            id: `local:${root.name}`,
            name: root.name,
            paths: entries.map((e) => projectRelative(e.path, root.name)),
        };
    }
    return { id: "local:example", name: EXAMPLE_LABEL, paths: [EXAMPLE_PATH] };
}

/**
 * Resolve the bundle to play locally: the open folder's `.loom` files
 * (live editor buffers win over on-disk content so you can play unsaved
 * edits), or the bundled example when no folder / no `.loom` files.
 */
async function gatherLocalSources(): Promise<{ files: PlayFile[]; label: string }> {
    const ws = useWorkspace.getState();
    const root = ws.root;
    const entries = collectLoomEntries(root);
    if (root && entries.length > 0) {
        const files: PlayFile[] = [];
        for (const e of entries) {
            const open = ws.openFiles[e.path];
            let source: string;
            if (open) {
                source = open.contents;
            } else {
                try {
                    source = await readFileText(e.handle as FileSystemFileHandle);
                } catch {
                    continue;
                }
            }
            files.push({ path: projectRelative(e.path, root.name), source });
        }
        if (files.length > 0) return { files, label: root.name };
    }
    return {
        files: [{ path: EXAMPLE_PATH, source: EXAMPLE_SOURCE }],
        label: EXAMPLE_LABEL,
    };
}

export const useSession = create<SessionState>((set, get) => {
    const persistedToken = loadSessionToken();
    const persistedUser = loadUsername();

    // Run a local-engine mutation and store the resulting play state.
    // Returns true when the active workspace is local (i.e. this call
    // owns the transport and the caller should NOT fall through to the
    // relay path).
    const applyLocal = (
        run: (engine: LocalPlayEngine) => PlayStatePayload,
    ): boolean => {
        const active = get().active;
        if (!active || active.kind !== "local") return false;
        const engine = engineOf(active);
        if (!engine) return true;
        try {
            const play = run(engine);
            set((s) =>
                s.active?.kind === "local"
                    ? { active: { ...s.active, play } }
                    : s,
            );
        } catch (err) {
            set({ error: errMsg(err) });
        }
        return true;
    };

    return {
        relayUrl: persistedRelay(),
        status: persistedToken ? "authenticated" : "idle",
        error: null,
        username: persistedUser,
        token: persistedToken,

        workspaces: [],
        active: null,

        sync: null,
        presence: new PresenceTracker(),

        setRelayUrl: (url) => {
            try {
                localStorage.setItem(RELAY_KEY, url);
            } catch {
                /* ignore */
            }
            const state = get();
            state.sync?.close();
            set({ relayUrl: url, sync: null, active: null });
        },

        register: async (username, password) => {
            set({ status: "authenticating", error: null });
            try {
                const client = new LoomAuthClient({
                    baseUrl: get().relayUrl,
                });
                const res = await client.register(username, password);
                set({
                    status: "authenticated",
                    username: res.username,
                    token: res.sessionToken,
                });
            } catch (err) {
                set({ status: "error", error: errMsg(err) });
                throw err;
            }
        },

        login: async (username, password) => {
            set({ status: "authenticating", error: null });
            try {
                const client = new LoomAuthClient({
                    baseUrl: get().relayUrl,
                });
                const res = await client.login(username, password);
                set({
                    status: "authenticated",
                    username: res.username,
                    token: res.sessionToken,
                });
            } catch (err) {
                set({ status: "error", error: errMsg(err) });
                throw err;
            }
        },

        logout: () => {
            const state = get();
            new LoomAuthClient({ baseUrl: state.relayUrl }).logout();
            state.sync?.close();
            set({
                status: "idle",
                error: null,
                username: null,
                token: null,
                workspaces: [],
                active: null,
                sync: null,
            });
        },

        connect: async () => {
            const state = get();
            if (state.sync) return;
            if (!state.token) {
                throw new Error("not authenticated");
            }
            set({ status: "connecting", error: null });
            const sync = new LoomSyncClient({
                url: wsUrlFromRelay(state.relayUrl),
                token: state.token,
                presence: state.presence,
                onPlayState: (play) => {
                    set((s) =>
                        s.active && s.active.meta.id === play.workspace
                            ? { active: { ...s.active, play } }
                            : s,
                    );
                },
                onError: (err) => set({ error: err.message }),
            });
            try {
                await sync.connect();
                set({ sync, status: "connected" });
            } catch (err) {
                set({ status: "error", error: errMsg(err) });
                throw err;
            }
        },

        refreshWorkspaces: async () => {
            const state = get();
            if (!state.token) return;
            const client = new LoomWorkspaceClient({
                baseUrl: state.relayUrl,
                token: state.token,
            });
            try {
                const workspaces = await client.list();
                set({ workspaces });
            } catch (err) {
                set({ error: errMsg(err) });
                throw err;
            }
        },

        createWorkspace: async (name) => {
            const state = get();
            if (!state.token) throw new Error("not authenticated");
            const client = new LoomWorkspaceClient({
                baseUrl: state.relayUrl,
                token: state.token,
            });
            const meta = await client.create(name);
            set((s) => ({ workspaces: [...s.workspaces, meta] }));
            await get().openWorkspace(meta.id);
        },

        removeWorkspace: async (id) => {
            const state = get();
            if (!state.token) throw new Error("not authenticated");
            if (state.active?.meta.id === id) state.sync?.unsubscribe(id);
            const client = new LoomWorkspaceClient({
                baseUrl: state.relayUrl,
                token: state.token,
            });
            await client.remove(id);
            set((s) => ({
                workspaces: s.workspaces.filter((w) => w.id !== id),
                active: s.active?.meta.id === id ? null : s.active,
            }));
        },

        openWorkspace: async (id) => {
            const state = get();
            if (!state.token) throw new Error("not authenticated");
            await get().connect();
            const sync = get().sync;
            if (!sync) throw new Error("sync not ready");

            // Close any previous workspace subscription so we don't
            // accumulate stale doc snapshots in memory.
            if (state.active && state.active.meta.id !== id) {
                sync.unsubscribe(state.active.meta.id);
            }

            const meta =
                state.workspaces.find((w) => w.id === id) ??
                (await new LoomWorkspaceClient({
                    baseUrl: state.relayUrl,
                    token: state.token,
                }).get(id));

            const wasm = await loadWasm();
            const doc = new wasm.LoomDoc();

            // Auto-refresh `files` whenever the doc commits (local or
            // remote). Keep a single subscription per active workspace.
            const handle = doc.subscribe(() => get().refreshActiveFiles());
            const cleanup = () => handle.unsubscribe();

            // Switching to a cloud workspace: tear down any local play
            // engine so its wasm session is freed.
            engineOf(get().active)?.dispose();

            const active: ActiveWorkspace = {
                kind: "cloud",
                meta,
                doc,
                files: [],
                activePath: null,
                peers: [],
                linkedFolder: null,
                play: null,
            };
            set({ active });

            sync.subscribe(id, doc);

            // Keep `active.peers` mirrored from the presence tracker.
            const presenceListener = (
                workspace: string,
                peers: PresenceState[],
            ) => {
                if (workspace !== id) return;
                set((s) =>
                    s.active && s.active.meta.id === id
                        ? { active: { ...s.active, peers } }
                        : s,
                );
            };
            const offPresence = state.presence.subscribe(presenceListener);

            // Store cleanups by hijacking the active record's `doc`
            // subscription chain — re-fired on closeWorkspace.
            (active as ActiveWorkspace & { __cleanup?: () => void }).__cleanup =
                () => {
                    cleanup();
                    offPresence();
                };

            // Pull current file list now in case the snapshot is
            // already cached (or empty — listFiles returns []).
            get().refreshActiveFiles();
        },

        activateLocal: async () => {
            const state = get();
            // Never clobber a live cloud workspace.
            if (state.active?.kind === "cloud") return;
            const target = localTarget();
            // Idempotent: already on this exact local target.
            if (
                state.active?.kind === "local" &&
                state.active.meta.id === target.id
            ) {
                return;
            }
            const wasm = await loadWasm();
            // Re-check after the await — a cloud workspace may have
            // opened while wasm loaded.
            if (get().active?.kind === "cloud") return;
            const doc = new wasm.LoomDoc();
            // The local play engine reads files fresh at play time, so
            // this doc is just a placeholder for the `ActiveWorkspace`
            // shape — seed the paths so `doc.listFiles()` stays in step
            // with `active.files`.
            for (const p of target.paths) doc.setText(p, "");
            const meta: WorkspaceMeta = {
                id: target.id,
                name: target.name,
                owner: get().username ?? "you",
                createdAt: "",
            };
            const active: ActiveWorkspace = {
                kind: "local",
                meta,
                doc,
                files: target.paths,
                activePath: target.paths[0] ?? null,
                peers: [],
                linkedFolder: null,
                play: null,
            };
            (active as WithEngine).__engine = new LocalPlayEngine();
            // Dispose any prior local engine before swapping it out.
            engineOf(state.active)?.dispose();
            set({ active });
        },

        bindFolder: async (root) => {
            const state = get();
            if (state.active?.kind !== "cloud") {
                throw new Error("open a cloud workspace before binding a folder");
            }
            const existing = await readManifest(root);
            const manifest = await linkManifest(
                root,
                state.active.meta.id,
                state.relayUrl,
            );
            // Seed only when this folder hasn't been Loom-aware before
            // — subsequent re-binds inherit the existing on-disk state
            // and let the bridge's bidirectional flow reconcile.
            if (!existing) {
                await seedDocFromFolder(root, state.active.doc);
            }
            const bridge = new LoomFolderBridge(root, state.active.doc, {
                onError: (err) => set({ error: err.message }),
            });
            bridge.start();
            set((s) => {
                if (!s.active) return s;
                // Replace any prior bridge.
                const prior = (
                    s.active as ActiveWorkspace & {
                        __bridge?: LoomFolderBridge;
                    }
                ).__bridge;
                prior?.dispose();
                const nextActive: ActiveWorkspace = {
                    ...s.active,
                    linkedFolder: { root, manifest },
                };
                (
                    nextActive as ActiveWorkspace & {
                        __bridge?: LoomFolderBridge;
                    }
                ).__bridge = bridge;
                return { active: nextActive };
            });
            get().refreshActiveFiles();
        },

        startPlay: () => {
            const state = get();
            const active = state.active;
            if (!active) return;
            if (active.kind === "local") {
                const engine = engineOf(active);
                if (!engine) return;
                // Read the live editor buffers / folder fresh, then run
                // the show entirely in the browser.
                void (async () => {
                    try {
                        const { files } = await gatherLocalSources();
                        const play = await engine.start(
                            files,
                            get().username ?? "local",
                        );
                        set((s) =>
                            s.active?.kind === "local"
                                ? {
                                      active: {
                                          ...s.active,
                                          play,
                                          files: files.map((f) => f.path),
                                      },
                                  }
                                : s,
                        );
                    } catch (err) {
                        set({ error: errMsg(err) });
                    }
                })();
                return;
            }
            if (!state.sync) return;
            const files = active.doc.listFiles().map((p) => {
                const path = String(p);
                return { path, source: active.doc.getText(path) ?? "" };
            });
            state.sync.startPlay(active.meta.id, files);
        },

        sendChoice: (index, head) => {
            if (applyLocal((e) => e.choose(index, head))) return;
            const state = get();
            if (!state.active || !state.sync) return;
            state.sync.sendChoice(state.active.meta.id, index, head);
        },

        stopPlay: () => {
            const state = get();
            const active = state.active;
            if (!active) return;
            if (active.kind === "local") {
                engineOf(active)?.dispose();
                set((s) =>
                    s.active?.kind === "local"
                        ? { active: { ...s.active, play: null } }
                        : s,
                );
                return;
            }
            if (!state.sync) return;
            state.sync.stopPlay(active.meta.id);
            set((s) =>
                s.active ? { active: { ...s.active, play: null } } : s,
            );
        },

        // ── Phase 4: branching ──────────────────────────────────────
        forkPlay: (opts) => {
            if (applyLocal((e) => e.fork(opts?.parent, opts?.fromSnapshot)))
                return;
            const state = get();
            if (!state.active || !state.sync) return;
            state.sync.forkPlay(state.active.meta.id, {
                parent: opts?.parent,
                from_snapshot: opts?.fromSnapshot,
            });
        },
        snapshotPlay: (opts) => {
            if (applyLocal((e) => e.snapshot(opts?.head, opts?.label))) return;
            const state = get();
            if (!state.active || !state.sync) return;
            state.sync.snapshotPlay(state.active.meta.id, opts);
        },
        restorePlay: (head, snapshot) => {
            if (applyLocal((e) => e.restore(head, snapshot))) return;
            const state = get();
            if (!state.active || !state.sync) return;
            state.sync.restorePlay(state.active.meta.id, head, snapshot);
        },
        dropHead: (head) => {
            if (applyLocal((e) => e.dropHead(head))) return;
            const state = get();
            if (!state.active || !state.sync) return;
            state.sync.dropHead(state.active.meta.id, head);
        },
        setPrimaryHead: (head) => {
            if (applyLocal((e) => e.setPrimary(head))) return;
            const state = get();
            if (!state.active || !state.sync) return;
            state.sync.setPrimaryHead(state.active.meta.id, head);
        },
        boothSkip: (head) => {
            if (applyLocal((e) => e.boothSkip(head))) return;
            const state = get();
            if (!state.active || !state.sync) return;
            state.sync.boothSkip(state.active.meta.id, head);
        },
        boothForce: (raw, head) => {
            if (!raw) return;
            if (applyLocal((e) => e.boothForce(raw, head))) return;
            const state = get();
            if (!state.active || !state.sync) return;
            state.sync.boothForce(state.active.meta.id, raw, head);
        },
        boothReload: () => {
            const state = get();
            const active = state.active;
            if (!active) return;
            if (active.kind === "local") {
                const engine = engineOf(active);
                if (!engine) return;
                void (async () => {
                    try {
                        const { files } = await gatherLocalSources();
                        const play = engine.boothReload(files);
                        set((s) =>
                            s.active?.kind === "local"
                                ? { active: { ...s.active, play } }
                                : s,
                        );
                    } catch (err) {
                        set({ error: errMsg(err) });
                    }
                })();
                return;
            }
            if (!state.sync) return;
            const files = active.doc.listFiles().map((p) => {
                const path = String(p);
                return { path, source: active.doc.getText(path) ?? "" };
            });
            state.sync.boothReload(active.meta.id, files);
        },

        unbindFolder: async () => {
            const state = get();
            if (!state.active?.linkedFolder) return;
            const root = state.active.linkedFolder.root;
            const bridge = (
                state.active as ActiveWorkspace & {
                    __bridge?: LoomFolderBridge;
                }
            ).__bridge;
            bridge?.dispose();
            await unlinkManifest(root).catch(() => {
                /* swallow — folder may be read-only */
            });
            set((s) => {
                if (!s.active) return s;
                const nextActive: ActiveWorkspace = {
                    ...s.active,
                    linkedFolder: null,
                };
                (
                    nextActive as ActiveWorkspace & {
                        __bridge?: LoomFolderBridge;
                    }
                ).__bridge = undefined;
                return { active: nextActive };
            });
        },

        closeWorkspace: () => {
            const state = get();
            const active = state.active;
            if (!active) return;
            const cleanup = (
                active as ActiveWorkspace & {
                    __cleanup?: () => void;
                }
            ).__cleanup;
            cleanup?.();
            const bridge = (
                active as ActiveWorkspace & {
                    __bridge?: LoomFolderBridge;
                }
            ).__bridge;
            bridge?.dispose();
            engineOf(active)?.dispose();
            const wasCloud = active.kind === "cloud";
            if (wasCloud) state.sync?.unsubscribe(active.meta.id);
            set({ active: null });
            // Closing a cloud workspace drops back to local play so the
            // editor is never left without an active workspace.
            if (wasCloud) void get().activateLocal();
        },

        refreshActiveFiles: () => {
            set((s) => {
                if (!s.active) return s;
                const files = s.active.doc.listFiles().sort();
                const activePath =
                    s.active.activePath && files.includes(s.active.activePath)
                        ? s.active.activePath
                        : (files[0] ?? null);
                if (
                    files.length === s.active.files.length &&
                    files.every((f, i) => f === s.active!.files[i]) &&
                    activePath === s.active.activePath
                ) {
                    return s;
                }
                return { active: { ...s.active, files, activePath } };
            });
        },

        setActivePath: (path) => {
            set((s) =>
                s.active ? { active: { ...s.active, activePath: path } } : s,
            );
        },

        createFile: (path) => {
            const state = get();
            if (!state.active) return;
            try {
                state.active.doc.setText(path, "");
                get().refreshActiveFiles();
                set((s) =>
                    s.active ? { active: { ...s.active, activePath: path } } : s,
                );
            } catch (err) {
                set({ error: errMsg(err) });
            }
        },
    };
});

function errMsg(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
}
