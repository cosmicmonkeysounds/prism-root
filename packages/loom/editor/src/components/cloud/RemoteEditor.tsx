// Editor view for a remote (multi-user) workspace. Lives alongside
// the local FSA-rooted editor (`components/editor/Editor.tsx`); the
// dock surfaces this panel only when `useSession.active` is non-null.
//
// The CodeMirror instance binds directly to the workspace's `LoomDoc`
// via the existing `loomBinding` extension — no Zustand round-trip
// for buffer text. The file list (left rail) reads from
// `useSession.active.files`, which `session.ts` keeps in sync with
// `doc.listFiles()` on every commit.

import { useMemo, useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView, keymap } from "@codemirror/view";
import { EditorState, type Extension } from "@codemirror/state";
import { indentUnit } from "@codemirror/language";
import { indentWithTab } from "@codemirror/commands";
import { indentationMarkers } from "@replit/codemirror-indentation-markers";

import clsx from "clsx";
import { extensionForPath } from "@/lib/language";
import { loomBinding } from "@/lib/cm-loro";
import {
    exportWorkspaceToFolder,
    importFolderIntoWorkspace,
} from "@/lib/export";
import { pickDirectory } from "@/lib/fs";
import { useSession } from "@/store/session";
import { useSettings } from "@/store/settings";

export function RemoteEditor() {
    const active = useSession((s) => s.active);
    const peers = useSession((s) => s.active?.peers ?? []);

    if (!active) {
        return (
            <div className="h-full grid place-items-center text-zinc-500 text-sm bg-zinc-950">
                <div className="text-center space-y-2 max-w-sm">
                    <div className="text-zinc-300">No workspace open</div>
                    <div className="text-xs text-zinc-500">
                        Sign in through the <strong>Cloud</strong> panel
                        and pick a workspace to start editing live.
                    </div>
                </div>
            </div>
        );
    }

    return (
        <div className="h-full w-full flex bg-zinc-950 text-zinc-200">
            <FileRail />
            <div className="flex-1 min-w-0 flex flex-col">
                <Toolbar peerCount={peers.length} />
                <div className="flex-1 min-h-0">
                    <RemoteCm />
                </div>
            </div>
        </div>
    );
}

function FileRail() {
    const files = useSession((s) => s.active?.files ?? []);
    const activePath = useSession((s) => s.active?.activePath ?? null);
    const setActivePath = useSession((s) => s.setActivePath);
    const createFile = useSession((s) => s.createFile);

    const [draft, setDraft] = useState("");

    return (
        <div className="w-48 shrink-0 border-r border-white/10 flex flex-col text-xs">
            <div className="px-2 py-2 text-[11px] uppercase tracking-wider text-zinc-500 border-b border-white/5">
                Files
            </div>
            <ul className="flex-1 overflow-auto py-1">
                {files.length === 0 && (
                    <li className="px-2 py-1 text-zinc-500">No files yet.</li>
                )}
                {files.map((path) => (
                    <li key={path}>
                        <button
                            type="button"
                            onClick={() => setActivePath(path)}
                            className={clsx(
                                "w-full text-left px-2 py-1 truncate",
                                path === activePath
                                    ? "bg-blue-500/15 text-zinc-100"
                                    : "hover:bg-white/5 text-zinc-300",
                            )}
                            title={path}
                        >
                            {path}
                        </button>
                    </li>
                ))}
            </ul>
            <form
                className="p-2 border-t border-white/5 flex gap-1"
                onSubmit={(e) => {
                    e.preventDefault();
                    const name = draft.trim();
                    if (!name) return;
                    createFile(name);
                    setDraft("");
                }}
            >
                <input
                    type="text"
                    placeholder="new file…"
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    className="flex-1 min-w-0 bg-zinc-900 border border-white/10 rounded px-2 py-1 text-xs"
                />
                <button
                    type="submit"
                    disabled={!draft.trim()}
                    className="px-2 py-1 text-xs bg-blue-600/80 hover:bg-blue-600 disabled:bg-zinc-800 disabled:text-zinc-500 rounded"
                >
                    +
                </button>
            </form>
        </div>
    );
}

function Toolbar({ peerCount }: { peerCount: number }) {
    const meta = useSession((s) => s.active?.meta);
    const doc = useSession((s) => s.active?.doc ?? null);
    const refresh = useSession((s) => s.refreshActiveFiles);
    const close = useSession((s) => s.closeWorkspace);
    const [busy, setBusy] = useState<null | "export" | "import">(null);
    const [status, setStatus] = useState<string | null>(null);

    if (!meta) return null;

    const onExport = async () => {
        if (!doc || busy) return;
        let dir: FileSystemDirectoryHandle;
        try {
            dir = await pickDirectory();
        } catch {
            return; // user cancelled
        }
        setBusy("export");
        setStatus("Exporting…");
        try {
            const res = await exportWorkspaceToFolder(doc, dir);
            setStatus(
                `Exported ${res.filesWritten} file${res.filesWritten === 1 ? "" : "s"}.`,
            );
        } catch (err) {
            setStatus(err instanceof Error ? err.message : String(err));
        } finally {
            setBusy(null);
        }
    };

    const onImport = async () => {
        if (!doc || busy) return;
        let dir: FileSystemDirectoryHandle;
        try {
            dir = await pickDirectory();
        } catch {
            return;
        }
        setBusy("import");
        setStatus("Importing…");
        try {
            const res = await importFolderIntoWorkspace(doc, dir, {
                skipPrefixes: [
                    ".git/",
                    "node_modules/",
                    "dist/",
                    "target/",
                ],
            });
            refresh();
            setStatus(
                `Imported ${res.filesRead} file${res.filesRead === 1 ? "" : "s"}.`,
            );
        } catch (err) {
            setStatus(err instanceof Error ? err.message : String(err));
        } finally {
            setBusy(null);
        }
    };

    return (
        <div className="h-9 px-3 flex items-center gap-3 border-b border-white/10 text-xs">
            <span className="font-medium text-zinc-200 truncate">
                {meta.name}
            </span>
            <span className="text-zinc-500 truncate" title={meta.id}>
                {meta.id}
            </span>
            {status && (
                <span
                    className="text-zinc-400 truncate"
                    title={status}
                >
                    {status}
                </span>
            )}
            <span className="ml-auto text-zinc-400">
                {peerCount === 0
                    ? "Just you"
                    : peerCount === 1
                      ? "1 peer"
                      : `${peerCount} peers`}
            </span>
            <ToolbarButton
                label="Import…"
                onClick={onImport}
                disabled={busy !== null}
                title="Read a local folder into this workspace"
            />
            <ToolbarButton
                label="Export…"
                onClick={onExport}
                disabled={busy !== null}
                title="Write this workspace's files to a local folder"
            />
            <button
                type="button"
                onClick={close}
                className="text-zinc-500 hover:text-zinc-200"
            >
                Close
            </button>
        </div>
    );
}

function ToolbarButton({
    label,
    onClick,
    disabled,
    title,
}: {
    label: string;
    onClick: () => void;
    disabled?: boolean;
    title?: string;
}) {
    return (
        <button
            type="button"
            onClick={onClick}
            disabled={disabled}
            title={title}
            className={clsx(
                "px-2 py-0.5 rounded border border-white/10",
                disabled
                    ? "text-zinc-600"
                    : "text-zinc-300 hover:text-zinc-100 hover:border-white/20",
            )}
        >
            {label}
        </button>
    );
}

function RemoteCm() {
    const active = useSession((s) => s.active);
    const activePath = active?.activePath ?? null;
    const settings = useSettings();

    const extensions = useMemo<Extension[]>(() => {
        if (!active || !activePath) return [];
        const ext: Extension[] = [...extensionForPath(activePath)];
        ext.push(EditorState.tabSize.of(settings.tabSize));
        ext.push(
            indentUnit.of(
                settings.indentWithTabs ? "\t" : " ".repeat(settings.tabSize),
            ),
        );
        ext.push(keymap.of([indentWithTab]));
        if (settings.wordWrap) ext.push(EditorView.lineWrapping);
        if (settings.showIndentGuides)
            ext.push(
                indentationMarkers({
                    highlightActiveBlock: true,
                    hideFirstIndent: false,
                }),
            );
        ext.push(
            EditorView.theme({
                "&": { fontSize: `${settings.fontSize}px` },
                ".cm-scroller": { fontFamily: settings.fontFamily },
            }),
        );
        // The Loro binding owns the buffer — `value` below is only the
        // initial seed; subsequent dispatches come from this extension.
        ext.push(loomBinding({ doc: active.doc, path: activePath }));
        return ext;
    }, [active, activePath, settings]);

    if (!active || !activePath) {
        return (
            <div className="h-full grid place-items-center text-zinc-500 text-sm">
                <div className="text-center space-y-2 max-w-sm">
                    <div>No file selected.</div>
                    <div className="text-xs text-zinc-600">
                        Pick or create one in the left rail.
                    </div>
                </div>
            </div>
        );
    }

    return (
        <CodeMirror
            key={`${active.meta.id}::${activePath}`}
            value={active.doc.getText(activePath) ?? ""}
            theme={settings.theme === "dark" ? oneDark : "light"}
            extensions={extensions}
            basicSetup={{
                lineNumbers: settings.lineNumbers,
                highlightActiveLine: settings.highlightActiveLine,
                highlightActiveLineGutter: settings.highlightActiveLine,
                foldGutter: settings.foldGutter,
                autocompletion: settings.autocompletion,
                bracketMatching: settings.bracketMatching,
                closeBrackets: settings.closeBrackets,
                indentOnInput: true,
                searchKeymap: true,
                highlightSelectionMatches: true,
                drawSelection: true,
                rectangularSelection: true,
                crosshairCursor: true,
                history: true,
                tabSize: settings.tabSize,
            }}
            height="100%"
            style={{ height: "100%" }}
        />
    );
}
