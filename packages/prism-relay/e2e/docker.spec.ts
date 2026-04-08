/**
 * Prism Relay — Docker E2E Tests
 *
 * Builds and runs real Docker containers, then exercises the relay API
 * through the container's mapped ports. Tests cover:
 * - Image build
 * - Single container (server mode) — health, API, WebSocket
 * - Persistence across container restart
 * - Graceful shutdown with state persistence
 * - Federation between two containers
 * - Dev mode container (CORS/CSRF behavior)
 *
 * All tests are skipped if Docker is not available.
 */

import { test, expect } from "@playwright/test";
import * as childProcess from "node:child_process";
import * as path from "node:path";

// ── Helpers ──────────────────────────────────────────────────────────────────

const relayDir = path.resolve(import.meta.dirname, "..");
const composeFile = path.join(relayDir, "docker-compose.test.yml");
const projectName = `prism-e2e-${process.pid}`;

function isDockerAvailable(): boolean {
  try {
    childProcess.execSync("docker info", { stdio: "pipe", timeout: 5_000 });
    childProcess.execSync("docker compose version", { stdio: "pipe", timeout: 5_000 });
    return true;
  } catch {
    return false;
  }
}

const dockerReady = isDockerAvailable();

function compose(args: string, opts?: { timeout?: number }): string {
  const cmd = `docker compose -f ${composeFile} -p ${projectName} ${args}`;
  return childProcess.execSync(cmd, {
    stdio: "pipe",
    timeout: opts?.timeout ?? 300_000,
    cwd: relayDir,
  }).toString().trim();
}

function composeSpawn(args: string, opts?: { timeout?: number }): { status: number | null; stdout: string; stderr: string } {
  const parts = `compose -f ${composeFile} -p ${projectName} ${args}`.split(" ");
  const result = childProcess.spawnSync("docker", parts, {
    stdio: "pipe",
    timeout: opts?.timeout ?? 300_000,
    cwd: relayDir,
  });
  return {
    status: result.status,
    stdout: result.stdout?.toString().trim() ?? "",
    stderr: result.stderr?.toString().trim() ?? "",
  };
}

function getPort(service: string, internal = 4444): number {
  const output = compose(`port ${service} ${internal}`);
  // Output format: 0.0.0.0:XXXXX or [::]:XXXXX
  const match = output.match(/:(\d+)$/);
  if (!match) throw new Error(`Could not parse port from: ${output}`);
  return parseInt(match[1], 10);
}

async function waitForHealthy(url: string, timeoutMs = 60_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${url}/api/health`);
      if (res.ok) return;
    } catch {
      // Connection refused — container still starting
    }
    await new Promise((r) => setTimeout(r, 2_000));
  }
  throw new Error(`Container at ${url} did not become healthy within ${timeoutMs}ms`);
}

function cleanupProfile(profile: string): void {
  try {
    compose(`--profile ${profile} down --volumes --timeout 5`, { timeout: 30_000 });
  } catch {
    // Best-effort cleanup
  }
}

// ════════════════════════════════════════════════════════════════════════════════
// 1. DOCKER IMAGE BUILD
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Docker Image Build", () => {
  test.skip(!dockerReady, "Docker not available");

  test("builds the relay image successfully", async () => {
    test.slow(); // Image builds can take minutes
    const result = composeSpawn("--profile single build", { timeout: 600_000 });
    expect(result.status, `Build failed: ${result.stderr}`).toBe(0);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 2. SINGLE CONTAINER (SERVER MODE)
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Single Container (Server Mode)", () => {
  test.skip(!dockerReady, "Docker not available");

  let url: string;

  test.beforeAll(async () => {
    compose("--profile single up -d", { timeout: 600_000 });
    const port = getPort("relay");
    url = `http://localhost:${port}`;
    await waitForHealthy(url);
  });

  test.afterAll(() => {
    cleanupProfile("single");
  });

  test("GET /api/health returns healthy status", async () => {
    const res = await fetch(`${url}/api/health`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.status).toBe("healthy");
    expect(body).toHaveProperty("did");
    expect(body.did).toMatch(/^did:key:z6Mk/);
    expect(body).toHaveProperty("uptime");
    expect(typeof body.uptime).toBe("number");
    expect(body).toHaveProperty("modules");
    expect(body.modules).toBeGreaterThanOrEqual(16);
    expect(body).toHaveProperty("memory");
  });

  test("GET /api/status returns running relay", async () => {
    const res = await fetch(`${url}/api/status`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.running).toBe(true);
    expect(body).toHaveProperty("did");
    expect(body).toHaveProperty("modules");
    expect(Array.isArray(body.modules)).toBe(true);
  });

  test("GET /api/modules returns all 16 modules", async () => {
    const res = await fetch(`${url}/api/modules`);
    expect(res.status).toBe(200);
    const modules = await res.json();
    expect(Array.isArray(modules)).toBe(true);
    expect(modules.length).toBe(16);
  });

  test("portal CRUD through Docker container", async () => {
    // Create portal (server mode has CSRF enabled, but we use X-Prism-CSRF header)
    const createRes = await fetch(`${url}/api/portals`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Prism-CSRF": "1",
      },
      body: JSON.stringify({
        name: "Docker Test Portal",
        level: 1,
        collectionId: "docker-col",
        basePath: "/docker-test",
        isPublic: true,
      }),
    });
    expect(createRes.status).toBe(201);
    const portal = await createRes.json();
    expect(portal).toHaveProperty("id");
    expect(portal.name).toBe("Docker Test Portal");

    // Read portal back
    const getRes = await fetch(`${url}/api/portals`);
    expect(getRes.status).toBe(200);
    const portals = await getRes.json();
    expect(portals.some((p: { name: string }) => p.name === "Docker Test Portal")).toBe(true);
  });

  test("collection CRUD through Docker container", async () => {
    // Create collection
    const createRes = await fetch(`${url}/api/collections`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Prism-CSRF": "1",
      },
      body: JSON.stringify({ id: "docker-e2e-col" }),
    });
    expect(createRes.status).toBe(200);

    // List collections
    const listRes = await fetch(`${url}/api/collections`);
    expect(listRes.status).toBe(200);
    const collections = await listRes.json();
    expect(collections).toContain("docker-e2e-col");
  });

  test("WebSocket connect and auth handshake through Docker", async () => {
    const ws = await new Promise<WebSocket>((resolve, reject) => {
      const socket = new WebSocket(`ws://localhost:${new URL(url).port}/ws/relay`);
      socket.addEventListener("open", () => resolve(socket));
      socket.addEventListener("error", (e) => reject(e));
    });

    const reply = new Promise<Record<string, unknown>>((resolve) => {
      ws.addEventListener("message", (evt) => {
        resolve(JSON.parse(String(evt.data)));
      });
    });

    ws.send(JSON.stringify({ type: "auth", did: "did:key:z6MkTestDocker" }));
    const msg = await reply;
    expect(msg.type).toBe("auth-ok");

    ws.close();
  });

  test("WebSocket ping/pong through Docker", async () => {
    const ws = await new Promise<WebSocket>((resolve, reject) => {
      const socket = new WebSocket(`ws://localhost:${new URL(url).port}/ws/relay`);
      socket.addEventListener("open", () => resolve(socket));
      socket.addEventListener("error", (e) => reject(e));
    });

    const reply = new Promise<Record<string, unknown>>((resolve) => {
      ws.addEventListener("message", (evt) => {
        const msg = JSON.parse(String(evt.data));
        if (msg.type === "pong") resolve(msg);
      });
    });

    ws.send(JSON.stringify({ type: "ping" }));
    const msg = await reply;
    expect(msg.type).toBe("pong");

    ws.close();
  });

  test("SEO endpoints work through Docker", async () => {
    const robotsRes = await fetch(`${url}/robots.txt`);
    expect(robotsRes.status).toBe(200);
    const robots = await robotsRes.text();
    expect(robots).toContain("User-agent");

    const sitemapRes = await fetch(`${url}/sitemap.xml`);
    expect(sitemapRes.status).toBe(200);
    const sitemap = await sitemapRes.text();
    expect(sitemap).toContain("<?xml");
  });

  test("CSRF is enforced in server mode", async () => {
    const res = await fetch(`${url}/api/portals`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "No CSRF", level: 1, collectionId: "c1", basePath: "/x", isPublic: true }),
    });
    expect(res.status).toBe(403);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 3. PERSISTENCE ACROSS RESTART
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Persistence Across Restart", () => {
  test.skip(!dockerReady, "Docker not available");

  let url: string;

  test.beforeAll(async () => {
    compose("--profile persist up -d", { timeout: 600_000 });
    const port = getPort("relay-persist");
    url = `http://localhost:${port}`;
    await waitForHealthy(url);
  });

  test.afterAll(() => {
    cleanupProfile("persist");
  });

  test("state survives container restart", async () => {
    // Create portal
    const createRes = await fetch(`${url}/api/portals`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Prism-CSRF": "1",
      },
      body: JSON.stringify({
        name: "Persist Portal",
        level: 1,
        collectionId: "persist-col",
        basePath: "/persist",
        isPublic: true,
      }),
    });
    expect(createRes.status).toBe(201);

    // Create webhook
    const whRes = await fetch(`${url}/api/webhooks`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Prism-CSRF": "1",
      },
      body: JSON.stringify({
        url: "https://persist.example/hook",
        events: ["*"],
        active: true,
      }),
    });
    expect(whRes.status).toBe(201);

    // Restart container (compose restart sends SIGTERM then starts again)
    compose("--profile persist restart relay-persist", { timeout: 60_000 });
    // Port mapping may change after restart — re-discover
    const newPort = getPort("relay-persist");
    url = `http://localhost:${newPort}`;
    await waitForHealthy(url);

    // Verify portal survived
    const portalsRes = await fetch(`${url}/api/portals`);
    expect(portalsRes.status).toBe(200);
    const portals = await portalsRes.json();
    expect(portals.some((p: { name: string }) => p.name === "Persist Portal")).toBe(true);

    // Verify webhook survived
    const webhooksRes = await fetch(`${url}/api/webhooks`);
    expect(webhooksRes.status).toBe(200);
    const webhooks = await webhooksRes.json();
    expect(webhooks.some((w: { url: string }) => w.url === "https://persist.example/hook")).toBe(true);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 4. GRACEFUL SHUTDOWN
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Graceful Shutdown", () => {
  test.skip(!dockerReady, "Docker not available");

  test("SIGTERM triggers state save before exit", async () => {
    // Start a persist container
    compose("--profile persist up -d", { timeout: 600_000 });
    const port = getPort("relay-persist");
    const url = `http://localhost:${port}`;
    await waitForHealthy(url);

    // Create state
    await fetch(`${url}/api/portals`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Prism-CSRF": "1",
      },
      body: JSON.stringify({
        name: "Shutdown Portal",
        level: 1,
        collectionId: "shutdown-col",
        basePath: "/shutdown",
        isPublic: true,
      }),
    });

    // Stop gracefully (SIGTERM)
    compose("--profile persist stop relay-persist", { timeout: 30_000 });

    // Start again (same volume)
    compose("--profile persist start relay-persist", { timeout: 60_000 });
    const newPort = getPort("relay-persist");
    const newUrl = `http://localhost:${newPort}`;
    await waitForHealthy(newUrl);

    // State should have been saved on SIGTERM
    const portalsRes = await fetch(`${newUrl}/api/portals`);
    const portals = await portalsRes.json();
    expect(portals.some((p: { name: string }) => p.name === "Shutdown Portal")).toBe(true);

    cleanupProfile("persist");
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 5. FEDERATION (TWO CONTAINERS)
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Federation (Two Containers)", () => {
  test.skip(!dockerReady, "Docker not available");

  let urlA: string;
  let urlB: string;

  test.beforeAll(async () => {
    compose("--profile federation up -d", { timeout: 600_000 });
    const portA = getPort("relay-a");
    const portB = getPort("relay-b");
    urlA = `http://localhost:${portA}`;
    urlB = `http://localhost:${portB}`;
    await Promise.all([waitForHealthy(urlA), waitForHealthy(urlB)]);
  });

  test.afterAll(() => {
    cleanupProfile("federation");
  });

  test("both relays are healthy with distinct DIDs", async () => {
    const [healthA, healthB] = await Promise.all([
      fetch(`${urlA}/api/health`).then((r) => r.json()),
      fetch(`${urlB}/api/health`).then((r) => r.json()),
    ]);
    expect(healthA.status).toBe("healthy");
    expect(healthB.status).toBe("healthy");
    expect(healthA.did).not.toBe(healthB.did);
  });

  test("relay-a discovers relay-b via federation announce", async () => {
    // Announce relay-b to relay-a
    const announceRes = await fetch(`${urlA}/api/federation/announce`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Prism-CSRF": "1",
      },
      body: JSON.stringify({
        relayDid: (await fetch(`${urlB}/api/health`).then((r) => r.json())).did,
        url: "http://relay-b:4444",
      }),
    });
    expect(announceRes.status).toBe(200);

    // Check peers
    const peersRes = await fetch(`${urlA}/api/federation/peers`);
    expect(peersRes.status).toBe(200);
    const peers = await peersRes.json();
    expect(peers.length).toBeGreaterThanOrEqual(1);
    expect(peers.some((p: { url: string }) => p.url === "http://relay-b:4444")).toBe(true);
  });

  test("both relays serve modules independently", async () => {
    const [modsA, modsB] = await Promise.all([
      fetch(`${urlA}/api/modules`).then((r) => r.json()),
      fetch(`${urlB}/api/modules`).then((r) => r.json()),
    ]);
    expect(modsA.length).toBe(16);
    expect(modsB.length).toBe(16);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 6. DEV MODE CONTAINER
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Dev Mode Container", () => {
  test.skip(!dockerReady, "Docker not available");

  let url: string;

  test.beforeAll(async () => {
    compose("--profile dev up -d", { timeout: 600_000 });
    const port = getPort("relay-dev");
    url = `http://localhost:${port}`;
    await waitForHealthy(url);
  });

  test.afterAll(() => {
    cleanupProfile("dev");
  });

  test("health endpoint works in dev mode", async () => {
    const res = await fetch(`${url}/api/health`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.status).toBe("healthy");
  });

  test("CORS allows all origins in dev mode", async () => {
    const res = await fetch(`${url}/api/health`, {
      headers: { Origin: "https://anything.example.com" },
    });
    expect(res.headers.get("access-control-allow-origin")).toBe("*");
  });

  test("CSRF is disabled in dev mode", async () => {
    const res = await fetch(`${url}/api/portals`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: "Dev Portal",
        level: 1,
        collectionId: "dev-col",
        basePath: "/dev-test",
        isPublic: true,
      }),
    });
    // Should NOT get 403 — CSRF disabled in dev mode
    expect(res.status).toBe(201);
  });
});
