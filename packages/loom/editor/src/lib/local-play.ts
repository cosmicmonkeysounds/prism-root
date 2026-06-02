// `LocalPlayEngine` — the client-side play loop, with no relay.
//
// Wraps the wasm `LoomSession` (compiled from `loom_runtime::session`)
// so the editor can run a full Loom show entirely in the browser. Every
// method returns the same `PlayStatePayload` the relay's `play-state`
// envelope carries, so the Runner panels (Transcript / World / Timeline
// / Choices / Cast / Inspector / Ledger / Graph / Booth) consume the
// local and cloud paths through one shape.
//
// The Luau VM isn't available on wasm, so Lua-defined directives and
// `.luau` extensions degrade to logged envelopes (the runtime registry
// runs lenient there) — the narrative engine itself is full-fidelity
// Rust. See `packages/loom/wasm/src/play.rs`.

import type { LoomSession } from "@/loom-wasm/loom_wasm";
import type { PlayFile, PlayStatePayload } from "./sync";

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

export class LocalPlayEngine {
    private session: LoomSession | null = null;

    get running(): boolean {
        return this.session != null;
    }

    /** Build (or replace) the session from `files` and run to the first
     *  pause point. */
    async start(files: PlayFile[], starter = "local"): Promise<PlayStatePayload> {
        const wasm = await loadWasm();
        this.dispose();
        this.session = new wasm.LoomSession(files, starter);
        return this.read();
    }

    choose(index: number, head?: string): PlayStatePayload {
        return parse(this.require().choose(index, head));
    }

    fork(parent?: string, fromSnapshot?: string): PlayStatePayload {
        return parse(this.require().fork(parent, fromSnapshot));
    }

    snapshot(head?: string, label?: string): PlayStatePayload {
        return parse(this.require().snapshot(head, label));
    }

    restore(head: string, snapshot: string): PlayStatePayload {
        return parse(this.require().restore(head, snapshot));
    }

    dropHead(head: string): PlayStatePayload {
        return parse(this.require().dropHead(head));
    }

    setPrimary(head: string): PlayStatePayload {
        return parse(this.require().setPrimary(head));
    }

    boothSkip(head?: string): PlayStatePayload {
        return parse(this.require().boothSkip(head));
    }

    boothForce(raw: string, head?: string): PlayStatePayload {
        return parse(this.require().boothForce(raw, head));
    }

    boothReload(files: PlayFile[]): PlayStatePayload {
        return parse(this.require().boothReload(files));
    }

    read(): PlayStatePayload {
        return parse(this.require().state());
    }

    /** Free the underlying wasm session. Idempotent. */
    dispose(): void {
        try {
            this.session?.free();
        } catch {
            /* already freed */
        }
        this.session = null;
    }

    private require(): LoomSession {
        if (!this.session) throw new Error("no local play session");
        return this.session;
    }
}

function parse(json: string): PlayStatePayload {
    return JSON.parse(json) as PlayStatePayload;
}
