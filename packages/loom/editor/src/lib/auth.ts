// Tiny wrapper around the `/api/auth/*` REST surface exposed by
// `loom-server` (see `docs/dev/loom-multiuser.md`). Persists the
// session token in `localStorage` under `loom.sessionToken` so the
// sync client can replay it as a bearer on the WebSocket upgrade.

const STORAGE_KEY = "loom.sessionToken";
const USERNAME_KEY = "loom.username";

export interface AuthResponse {
    username: string;
    sessionToken: string;
}

export interface AuthClientOptions {
    /** Origin of the relay, e.g. `http://127.0.0.1:7878`. */
    baseUrl: string;
}

export class LoomAuthClient {
    private readonly opts: AuthClientOptions;

    constructor(opts: AuthClientOptions) {
        this.opts = opts;
    }

    async register(username: string, password: string): Promise<AuthResponse> {
        const res = await this.post<AuthResponse>("/api/auth/register", {
            username,
            password,
        });
        this.persist(res);
        return res;
    }

    async login(username: string, password: string): Promise<AuthResponse> {
        const res = await this.post<AuthResponse>("/api/auth/login", {
            username,
            password,
        });
        this.persist(res);
        return res;
    }

    async change(
        username: string,
        oldPassword: string,
        newPassword: string,
    ): Promise<void> {
        await this.post("/api/auth/change", {
            username,
            oldPassword,
            newPassword,
        });
    }

    logout(): void {
        try {
            localStorage.removeItem(STORAGE_KEY);
            localStorage.removeItem(USERNAME_KEY);
        } catch {
            // SSR / private mode — nothing to clear.
        }
    }

    private async post<T>(path: string, body: unknown): Promise<T> {
        const res = await fetch(`${this.opts.baseUrl}${path}`, {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify(body),
        });
        if (!res.ok) {
            const text = await res.text().catch(() => res.statusText);
            throw new Error(`${path} failed: ${res.status} ${text}`);
        }
        return (await res.json()) as T;
    }

    private persist(res: AuthResponse): void {
        try {
            localStorage.setItem(STORAGE_KEY, res.sessionToken);
            localStorage.setItem(USERNAME_KEY, res.username);
        } catch {
            // SSR / private mode — token only lives in memory for
            // this session.
        }
    }
}

export function loadSessionToken(): string | null {
    try {
        return localStorage.getItem(STORAGE_KEY);
    } catch {
        return null;
    }
}

export function loadUsername(): string | null {
    try {
        return localStorage.getItem(USERNAME_KEY);
    } catch {
        return null;
    }
}
