// Sidebar panel for the multi-user (cloud) flow. Three stacked
// surfaces, gated on auth state:
//
//   1. Relay URL field (always visible, collapses behind a chevron).
//   2. Login / register form (when no session token).
//   3. Workspace list + create button (once authenticated).
//
// The "open in Remote editor" handoff is implicit — `useSession.active`
// drives `RemoteEditor.tsx`. Picking a workspace here will populate
// it; the dock's Remote panel surfaces the editor.

import { useEffect, useState } from "react";
import clsx from "clsx";
import { useSession } from "@/store/session";
import { pickDirectory } from "@/lib/fs";

export function CloudPanel() {
    const relayUrl = useSession((s) => s.relayUrl);
    const status = useSession((s) => s.status);
    const error = useSession((s) => s.error);
    const username = useSession((s) => s.username);
    const token = useSession((s) => s.token);

    return (
        <div className="h-full w-full flex flex-col bg-zinc-950 text-zinc-200 text-sm">
            <Section title="Relay">
                <RelayControl relayUrl={relayUrl} />
            </Section>

            {!token ? (
                <Section title="Sign in">
                    <AuthForm />
                </Section>
            ) : (
                <Section title="Signed in">
                    <SignedInBar username={username} />
                </Section>
            )}

            {token && (
                <Section title="Workspaces" className="flex-1 min-h-0">
                    <WorkspaceList />
                </Section>
            )}

            {token && <LinkSection />}

            <div className="mt-auto px-3 py-2 text-xs text-zinc-500 flex items-center gap-2">
                <StatusDot status={status} />
                <span>{statusLabel(status)}</span>
                {error && (
                    <span
                        className="ml-2 text-rose-400 truncate"
                        title={error}
                    >
                        {error}
                    </span>
                )}
            </div>
        </div>
    );
}

function Section({
    title,
    className,
    children,
}: {
    title: string;
    className?: string;
    children: React.ReactNode;
}) {
    return (
        <section
            className={clsx(
                "px-3 py-2 border-b border-white/5 flex flex-col gap-2",
                className,
            )}
        >
            <h2 className="text-[11px] uppercase tracking-wider text-zinc-500">
                {title}
            </h2>
            {children}
        </section>
    );
}

function RelayControl({ relayUrl }: { relayUrl: string }) {
    const setRelayUrl = useSession((s) => s.setRelayUrl);
    // Uncontrolled with `key={relayUrl}` so an external URL change
    // (e.g. log-in via a different host) remounts the input with the
    // new value instead of fighting React-side state during effect.
    const [draft, setDraft] = useState(relayUrl);

    return (
        <form
            key={relayUrl}
            className="flex gap-2"
            onSubmit={(e) => {
                e.preventDefault();
                setRelayUrl(draft.trim());
            }}
        >
            <input
                type="url"
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                placeholder="http://127.0.0.1:7878"
                className="flex-1 min-w-0 bg-zinc-900 border border-white/10 rounded px-2 py-1 text-xs"
            />
            <button
                type="submit"
                disabled={draft.trim() === relayUrl}
                className="px-2 py-1 text-xs bg-blue-600/80 disabled:bg-zinc-800 disabled:text-zinc-500 rounded"
            >
                Set
            </button>
        </form>
    );
}

function AuthForm() {
    const register = useSession((s) => s.register);
    const login = useSession((s) => s.login);
    const status = useSession((s) => s.status);
    const refreshWorkspaces = useSession((s) => s.refreshWorkspaces);

    const [mode, setMode] = useState<"login" | "register">("login");
    const [username, setUsername] = useState("");
    const [password, setPassword] = useState("");
    const [localError, setLocalError] = useState<string | null>(null);

    const submit = async (e: React.FormEvent) => {
        e.preventDefault();
        setLocalError(null);
        try {
            if (mode === "register") {
                await register(username, password);
            } else {
                await login(username, password);
            }
            await refreshWorkspaces().catch(() => {
                /* surfaced via store.error */
            });
        } catch (err) {
            setLocalError(err instanceof Error ? err.message : String(err));
        }
    };

    return (
        <form className="flex flex-col gap-2" onSubmit={submit}>
            <div className="flex gap-1 text-xs">
                <ModeTab
                    active={mode === "login"}
                    onClick={() => setMode("login")}
                    label="Log in"
                />
                <ModeTab
                    active={mode === "register"}
                    onClick={() => setMode("register")}
                    label="Register"
                />
            </div>
            <input
                type="text"
                placeholder="username"
                autoComplete="username"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="bg-zinc-900 border border-white/10 rounded px-2 py-1 text-xs"
            />
            <input
                type="password"
                placeholder="password"
                autoComplete={
                    mode === "register" ? "new-password" : "current-password"
                }
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="bg-zinc-900 border border-white/10 rounded px-2 py-1 text-xs"
            />
            <button
                type="submit"
                disabled={
                    status === "authenticating" ||
                    !username.trim() ||
                    !password
                }
                className="px-2 py-1 text-xs bg-blue-600/80 hover:bg-blue-600 disabled:bg-zinc-800 disabled:text-zinc-500 rounded"
            >
                {mode === "register" ? "Create account" : "Sign in"}
            </button>
            {localError && (
                <p className="text-xs text-rose-400">{localError}</p>
            )}
        </form>
    );
}

function ModeTab({
    active,
    onClick,
    label,
}: {
    active: boolean;
    onClick: () => void;
    label: string;
}) {
    return (
        <button
            type="button"
            onClick={onClick}
            className={clsx(
                "px-2 py-1 rounded",
                active
                    ? "bg-white/5 text-zinc-100"
                    : "text-zinc-500 hover:text-zinc-300",
            )}
        >
            {label}
        </button>
    );
}

function SignedInBar({ username }: { username: string | null }) {
    const logout = useSession((s) => s.logout);
    return (
        <div className="flex items-center justify-between">
            <span className="text-zinc-300 text-xs truncate">
                {username ?? "(unknown)"}
            </span>
            <button
                type="button"
                onClick={logout}
                className="text-xs text-zinc-400 hover:text-zinc-200 underline-offset-2 hover:underline"
            >
                Log out
            </button>
        </div>
    );
}

function WorkspaceList() {
    const workspaces = useSession((s) => s.workspaces);
    const active = useSession((s) => s.active);
    const refresh = useSession((s) => s.refreshWorkspaces);
    const open = useSession((s) => s.openWorkspace);
    const create = useSession((s) => s.createWorkspace);
    const remove = useSession((s) => s.removeWorkspace);

    const [newName, setNewName] = useState("");
    const [busy, setBusy] = useState(false);
    const [err, setErr] = useState<string | null>(null);

    useEffect(() => {
        void refresh();
    }, [refresh]);

    const onCreate = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!newName.trim()) return;
        setBusy(true);
        setErr(null);
        try {
            await create(newName.trim());
            setNewName("");
        } catch (e) {
            setErr(e instanceof Error ? e.message : String(e));
        } finally {
            setBusy(false);
        }
    };

    const onDelete = async (id: string) => {
        if (!window.confirm("Delete this workspace?")) return;
        try {
            await remove(id);
        } catch (e) {
            setErr(e instanceof Error ? e.message : String(e));
        }
    };

    return (
        <div className="flex flex-col gap-2 min-h-0">
            <ul className="flex flex-col gap-0.5 overflow-auto min-h-0">
                {workspaces.length === 0 && (
                    <li className="text-xs text-zinc-500">No workspaces yet.</li>
                )}
                {workspaces.map((w) => {
                    const isActive = active?.meta.id === w.id;
                    return (
                        <li
                            key={w.id}
                            className={clsx(
                                "group flex items-center gap-2 px-2 py-1 rounded cursor-pointer",
                                isActive
                                    ? "bg-blue-500/15 text-zinc-100"
                                    : "hover:bg-white/5",
                            )}
                            onClick={() => void open(w.id)}
                        >
                            <span className="flex-1 truncate text-xs">
                                {w.name}
                            </span>
                            <button
                                type="button"
                                title="Delete workspace"
                                onClick={(e) => {
                                    e.stopPropagation();
                                    void onDelete(w.id);
                                }}
                                className="opacity-0 group-hover:opacity-100 text-xs text-zinc-500 hover:text-rose-400"
                            >
                                ×
                            </button>
                        </li>
                    );
                })}
            </ul>

            <form
                className="flex gap-2"
                onSubmit={(e) => {
                    void onCreate(e);
                }}
            >
                <input
                    type="text"
                    placeholder="new workspace…"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    className="flex-1 min-w-0 bg-zinc-900 border border-white/10 rounded px-2 py-1 text-xs"
                />
                <button
                    type="submit"
                    disabled={busy || !newName.trim()}
                    className="px-2 py-1 text-xs bg-blue-600/80 hover:bg-blue-600 disabled:bg-zinc-800 disabled:text-zinc-500 rounded"
                >
                    Add
                </button>
            </form>
            {err && <p className="text-xs text-rose-400">{err}</p>}
        </div>
    );
}

function LinkSection() {
    const active = useSession((s) => s.active);
    const bindFolder = useSession((s) => s.bindFolder);
    const unbindFolder = useSession((s) => s.unbindFolder);
    const [busy, setBusy] = useState<null | "bind" | "unbind">(null);
    const [err, setErr] = useState<string | null>(null);

    // Linking a local folder only makes sense for a relay-hosted
    // workspace; the local-play workspace already *is* the folder.
    if (active?.kind !== "cloud") return null;
    const linked = active.linkedFolder;

    const onBind = async () => {
        setErr(null);
        let root: FileSystemDirectoryHandle;
        try {
            root = await pickDirectory();
        } catch {
            return;
        }
        setBusy("bind");
        try {
            await bindFolder(root);
        } catch (e) {
            setErr(e instanceof Error ? e.message : String(e));
        } finally {
            setBusy(null);
        }
    };

    const onUnbind = async () => {
        setBusy("unbind");
        setErr(null);
        try {
            await unbindFolder();
        } catch (e) {
            setErr(e instanceof Error ? e.message : String(e));
        } finally {
            setBusy(null);
        }
    };

    return (
        <Section title="Local folder">
            {linked ? (
                <div className="flex flex-col gap-2">
                    <div className="text-xs text-zinc-300 truncate" title={linked.root.name}>
                        Bound to <strong>{linked.root.name}</strong>
                    </div>
                    <button
                        type="button"
                        disabled={busy !== null}
                        onClick={() => void onUnbind()}
                        className="px-2 py-1 text-xs bg-zinc-800 hover:bg-zinc-700 disabled:bg-zinc-900 disabled:text-zinc-500 rounded"
                    >
                        {busy === "unbind" ? "Unbinding…" : "Unbind folder"}
                    </button>
                    <p className="text-[11px] text-zinc-500">
                        File edits round-trip to disk; outside changes flow
                        back into the workspace.
                    </p>
                </div>
            ) : (
                <div className="flex flex-col gap-2">
                    <button
                        type="button"
                        disabled={busy !== null}
                        onClick={() => void onBind()}
                        className="px-2 py-1 text-xs bg-blue-600/80 hover:bg-blue-600 disabled:bg-zinc-800 disabled:text-zinc-500 rounded"
                    >
                        {busy === "bind" ? "Binding…" : "Bind to local folder…"}
                    </button>
                    <p className="text-[11px] text-zinc-500">
                        Pick a folder on disk; files become real files and
                        stay in sync with this workspace.
                    </p>
                </div>
            )}
            {err && <p className="text-xs text-rose-400">{err}</p>}
        </Section>
    );
}

function StatusDot({ status }: { status: string }) {
    const color =
        status === "connected"
            ? "bg-emerald-400"
            : status === "connecting" || status === "authenticating"
              ? "bg-amber-400"
              : status === "error"
                ? "bg-rose-400"
                : "bg-zinc-600";
    return (
        <span
            aria-hidden
            className={clsx("inline-block w-2 h-2 rounded-full", color)}
        />
    );
}

function statusLabel(status: string): string {
    switch (status) {
        case "idle":
            return "Disconnected";
        case "authenticating":
            return "Authenticating…";
        case "authenticated":
            return "Signed in";
        case "connecting":
            return "Connecting…";
        case "connected":
            return "Live";
        case "error":
            return "Error";
        default:
            return status;
    }
}
