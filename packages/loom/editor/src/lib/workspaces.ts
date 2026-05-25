// REST client for `/api/workspaces/*` and `/api/tokens/*` on
// `loom-server`. Pairs with `auth.ts`; the session token is supplied
// per call (the store owns whichever token is active) so the wrapper
// stays stateless.

export interface WorkspaceMeta {
    id: string;
    name: string;
    owner: string;
    createdAt: string;
}

export interface IssueTokenRequest {
    workspaceId: string;
    permissions: string[];
    ttlSeconds?: number;
}

export interface IssueTokenResponse {
    token: string;
    tokenId: string;
    expiresAt: string | null;
}

export interface VerifyTokenResponse {
    valid: boolean;
    subject?: string;
    scope?: string;
    permissions?: string[];
    expiresAt?: string | null;
}

export interface WorkspaceClientOptions {
    /** Origin of the relay, e.g. `http://127.0.0.1:7878`. */
    baseUrl: string;
    /** Bearer session token from `LoomAuthClient`. */
    token: string;
}

export class LoomWorkspaceClient {
    private readonly opts: WorkspaceClientOptions;

    constructor(opts: WorkspaceClientOptions) {
        this.opts = opts;
    }

    async list(): Promise<WorkspaceMeta[]> {
        const { workspaces } = await this.request<{
            workspaces: WorkspaceMeta[];
        }>("GET", "/api/workspaces");
        return workspaces;
    }

    async create(name: string): Promise<WorkspaceMeta> {
        return await this.request<WorkspaceMeta>(
            "POST",
            "/api/workspaces",
            { name },
        );
    }

    async get(id: string): Promise<WorkspaceMeta> {
        return await this.request<WorkspaceMeta>(
            "GET",
            `/api/workspaces/${encodeURIComponent(id)}`,
        );
    }

    async remove(id: string): Promise<void> {
        await this.request<{ ok: boolean }>(
            "DELETE",
            `/api/workspaces/${encodeURIComponent(id)}`,
        );
    }

    /** Returns the raw CRDT snapshot bytes for `id`. */
    async getSnapshot(id: string): Promise<Uint8Array> {
        const res = await fetch(
            `${this.opts.baseUrl}/api/workspaces/${encodeURIComponent(id)}/snapshot`,
            {
                method: "GET",
                headers: { authorization: `Bearer ${this.opts.token}` },
            },
        );
        if (!res.ok) throw await toError(res);
        return new Uint8Array(await res.arrayBuffer());
    }

    /** Replaces the workspace's snapshot wholesale. */
    async putSnapshot(id: string, bytes: Uint8Array): Promise<void> {
        // Re-allocate over a plain `ArrayBuffer` — newer TS pins
        // `Uint8Array<ArrayBufferLike>` which the dom lib's `BodyInit`
        // union no longer accepts directly. `new Uint8Array(bytes)`
        // gives us a `Uint8Array<ArrayBuffer>` that does conform.
        const body = new Uint8Array(bytes);
        const res = await fetch(
            `${this.opts.baseUrl}/api/workspaces/${encodeURIComponent(id)}/snapshot`,
            {
                method: "POST",
                headers: {
                    authorization: `Bearer ${this.opts.token}`,
                    "content-type": "application/octet-stream",
                },
                body,
            },
        );
        if (!res.ok) throw await toError(res);
    }

    async issueToken(req: IssueTokenRequest): Promise<IssueTokenResponse> {
        return await this.request<IssueTokenResponse>(
            "POST",
            "/api/tokens/issue",
            req,
        );
    }

    async verifyToken(token: string): Promise<VerifyTokenResponse> {
        return await this.request<VerifyTokenResponse>(
            "POST",
            "/api/tokens/verify",
            { token },
        );
    }

    private async request<T>(
        method: "GET" | "POST" | "DELETE",
        path: string,
        body?: unknown,
    ): Promise<T> {
        const init: RequestInit = {
            method,
            headers: { authorization: `Bearer ${this.opts.token}` },
        };
        if (body !== undefined) {
            init.headers = {
                ...init.headers,
                "content-type": "application/json",
            };
            init.body = JSON.stringify(body);
        }
        const res = await fetch(`${this.opts.baseUrl}${path}`, init);
        if (!res.ok) throw await toError(res);
        return (await res.json()) as T;
    }
}

async function toError(res: Response): Promise<Error> {
    let detail = "";
    try {
        const body = await res.json();
        if (body && typeof body.error === "string") detail = body.error;
    } catch {
        try {
            detail = await res.text();
        } catch {
            /* swallow */
        }
    }
    const suffix = detail ? `: ${detail}` : "";
    return new Error(`${res.status} ${res.statusText}${suffix}`);
}

/**
 * Derive the WebSocket URL for the same relay origin.
 *
 *   relayUrl  ws/wss
 *   --------  -------
 *   http://x  ws://x/ws
 *   https://x wss://x/ws
 */
export function wsUrlFromRelay(baseUrl: string): string {
    const u = new URL(baseUrl);
    u.protocol = u.protocol === "https:" ? "wss:" : "ws:";
    u.pathname = u.pathname.replace(/\/?$/, "") + "/ws";
    return u.toString();
}
