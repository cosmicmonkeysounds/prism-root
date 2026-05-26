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
import { LoomSyncClient, type PlayStatePayload } from "@/lib/sync";
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

export interface ActiveWorkspace {
    meta: WorkspaceMeta;
    doc: LoomDoc;
    files: string[];
    activePath: string | null;
    peers: PresenceState[];
    /** Local folder bound to this workspace, when one is linked. */
    linkedFolder: LinkedFolder | null;
    /** Phase 7 — current server-hosted play session, if any. */
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
    sendChoice: (index: number) => void;
    /** Phase 7 — tear the active session down. */
    stopPlay: () => void;
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

export const useSession = create<SessionState>((set, get) => {
    const persistedToken = loadSessionToken();
    const persistedUser = loadUsername();

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

            const active: ActiveWorkspace = {
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

        bindFolder: async (root) => {
            const state = get();
            if (!state.active) throw new Error("no active workspace");
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
            if (!state.active || !state.sync) return;
            const files = state.active.doc.listFiles().map((p) => {
                const path = String(p);
                return { path, source: state.active!.doc.getText(path) ?? "" };
            });
            state.sync.startPlay(state.active.meta.id, files);
        },

        sendChoice: (index) => {
            const state = get();
            if (!state.active || !state.sync) return;
            state.sync.sendChoice(state.active.meta.id, index);
        },

        stopPlay: () => {
            const state = get();
            if (!state.active || !state.sync) return;
            state.sync.stopPlay(state.active.meta.id);
            set((s) =>
                s.active ? { active: { ...s.active, play: null } } : s,
            );
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
            if (!state.active) return;
            const cleanup = (
                state.active as ActiveWorkspace & {
                    __cleanup?: () => void;
                }
            ).__cleanup;
            cleanup?.();
            const bridge = (
                state.active as ActiveWorkspace & {
                    __bridge?: LoomFolderBridge;
                }
            ).__bridge;
            bridge?.dispose();
            state.sync?.unsubscribe(state.active.meta.id);
            set({ active: null });
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
