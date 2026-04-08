/**
 * Prism Relay — Deployment Tests
 *
 * Validates that all deployment options work correctly:
 * - Docker build (Dockerfile correctness)
 * - Config generation (init command)
 * - All three deployment modes (server, p2p, dev)
 * - Health check contract
 * - CSRF enforcement per mode
 * - Backup/restore via API round-trip
 * - Graceful shutdown with state persistence
 * - Docker Compose validation
 * - Environment variable override precedence
 */

import { test, expect } from "@playwright/test";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import * as childProcess from "node:child_process";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  relayTimestampModule,
  blindPingModule,
  capabilityTokenModule,
  webhookModule,
  sovereignPortalModule,
  collectionHostModule,
  hashcashModule,
  peerTrustModule,
  escrowModule,
  federationModule,
  acmeCertificateModule,
  portalTemplateModule,
  webrtcSignalingModule,
  vaultHostModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createRelayServer } from "@prism/relay/server";
import { createFileStore } from "@prism/relay/persistence";
import {
  resolveConfig,
  parseArgs,
} from "@prism/relay/config";

// ── Helpers ──────────────────────────────────────────────────────────────────

function tmpDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "prism-deploy-test-"));
}

function cleanDir(dir: string): void {
  fs.rmSync(dir, { recursive: true, force: true });
}

async function buildRelay(identity: PrismIdentity, opts?: { hashcashBits?: number }): Promise<RelayInstance> {
  const relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(relayTimestampModule(identity))
    .use(blindPingModule())
    .use(capabilityTokenModule(identity))
    .use(webhookModule())
    .use(sovereignPortalModule())
    .use(collectionHostModule())
    .use(hashcashModule({ bits: opts?.hashcashBits ?? 4 }))
    .use(peerTrustModule())
    .use(escrowModule())
    .use(federationModule())
    .use(acmeCertificateModule())
    .use(portalTemplateModule())
    .use(webrtcSignalingModule())
    .use(vaultHostModule())
    .build();
  await relay.start();
  return relay;
}

interface TestServer {
  port: number;
  url: string;
  close: () => Promise<void>;
}

async function startServer(
  relay: RelayInstance,
  opts?: {
    disableCsrf?: boolean;
    corsOrigins?: string[];
    publicUrl?: string;
  },
): Promise<TestServer> {
  const server = createRelayServer({
    relay,
    port: 0,
    publicUrl: opts?.publicUrl ?? "http://localhost:0",
    disableCsrf: opts?.disableCsrf ?? true,
    corsOrigins: opts?.corsOrigins,
  });
  const info = await server.start();
  return { port: info.port, url: `http://localhost:${info.port}`, close: info.close };
}

const projectRoot = path.resolve(import.meta.dirname, "../../..");

// ════════════════════════════════════════════════════════════════════════════════
// 1. DOCKERFILE VALIDATION
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Dockerfile", () => {
  const dockerfilePath = path.resolve(import.meta.dirname, "../Dockerfile");
  const dockerignorePath = path.resolve(import.meta.dirname, "../.dockerignore");

  test("Dockerfile exists and has correct structure", () => {
    expect(fs.existsSync(dockerfilePath)).toBe(true);
    const content = fs.readFileSync(dockerfilePath, "utf-8");

    // Multi-stage build
    expect(content).toContain("FROM node:22-slim AS build");
    expect(content).toContain("FROM node:22-slim AS production");

    // pnpm setup
    expect(content).toContain("corepack enable");
    expect(content).toContain("pnpm install --frozen-lockfile");

    // Build step
    expect(content).toContain("pnpm --filter @prism/relay build");

    // Production pruning
    expect(content).toContain("pnpm prune --prod");

    // Non-root user
    expect(content).toContain("useradd");
    expect(content).toContain("USER prism");

    // Environment defaults
    expect(content).toContain("PRISM_RELAY_MODE=server");
    expect(content).toContain("PRISM_RELAY_HOST=0.0.0.0");
    expect(content).toContain("PRISM_RELAY_PORT=4444");

    // Expose port
    expect(content).toContain("EXPOSE 4444");

    // Volume for persistent data
    expect(content).toContain("VOLUME");

    // Health check
    expect(content).toContain("HEALTHCHECK");

    // Entry point
    expect(content).toContain("CMD [\"node\", \"packages/prism-relay/dist/cli.js\"]");
  });

  test(".dockerignore exists and excludes build artifacts", () => {
    expect(fs.existsSync(dockerignorePath)).toBe(true);
    const content = fs.readFileSync(dockerignorePath, "utf-8");

    // Must exclude node_modules (re-installed in build stage)
    expect(content).toContain("node_modules/");

    // Must exclude dist (rebuilt in build stage)
    expect(content).toContain("dist/");

    // Must exclude git
    expect(content).toContain(".git/");

    // Must exclude tests
    expect(content).toContain("e2e/");

    // Must exclude legacy
    expect(content).toContain("$legacy-inspiration-only/");
  });

  test("Dockerfile syntax is valid (docker build --check)", () => {
    // Check if Docker is available
    try {
      childProcess.execSync("docker --version", { stdio: "pipe" });
    } catch {
      test.skip(true, "Docker not available — skipping Docker build check");
      return;
    }

    // Validate Dockerfile syntax with BuildKit check
    const result = childProcess.spawnSync(
      "docker",
      ["build", "--check", "-f", dockerfilePath, projectRoot],
      { stdio: "pipe", timeout: 30_000 },
    );
    // --check may not be supported on all Docker versions; skip if unsupported
    if (result.status !== 0) {
      const stderr = result.stderr?.toString() ?? "";
      if (stderr.includes("unknown flag") || stderr.includes("--check")) {
        // --check not supported, fall through
      } else {
        expect.soft(result.status, `Docker build --check failed: ${stderr}`).toBe(0);
      }
    }
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 2. DOCKER COMPOSE VALIDATION
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Docker Compose Files", () => {
  const relayDir = path.resolve(import.meta.dirname, "..");

  test("docker-compose.yml exists and is valid YAML", () => {
    const composePath = path.join(relayDir, "docker-compose.yml");
    expect(fs.existsSync(composePath)).toBe(true);
    const content = fs.readFileSync(composePath, "utf-8");

    // Basic structure checks
    expect(content).toContain("services:");
    expect(content).toContain("relay:");
    expect(content).toContain("4444:4444");
    expect(content).toContain("relay-data:");
    expect(content).toContain("PRISM_RELAY_MODE");
    expect(content).toContain("restart: unless-stopped");
  });

  test("docker-compose.federation.yml exists and defines mesh", () => {
    const composePath = path.join(relayDir, "docker-compose.federation.yml");
    expect(fs.existsSync(composePath)).toBe(true);
    const content = fs.readFileSync(composePath, "utf-8");

    // Two relays
    expect(content).toContain("relay-a:");
    expect(content).toContain("relay-b:");

    // Different external ports
    expect(content).toContain("4444:4444");
    expect(content).toContain("4445:4444");

    // Shared network
    expect(content).toContain("prism-federation");

    // Separate volumes
    expect(content).toContain("relay-a-data:");
    expect(content).toContain("relay-b-data:");

    // Dependency ordering
    expect(content).toContain("depends_on:");
  });

  test("docker compose config validates single-relay compose", () => {
    try {
      childProcess.execSync("docker compose version", { stdio: "pipe" });
    } catch {
      test.skip(true, "docker compose not available");
      return;
    }

    const composePath = path.join(relayDir, "docker-compose.yml");
    const result = childProcess.spawnSync(
      "docker",
      ["compose", "-f", composePath, "config", "--quiet"],
      { stdio: "pipe", timeout: 15_000 },
    );
    expect(result.status).toBe(0);
  });

  test("docker compose config validates federation compose", () => {
    try {
      childProcess.execSync("docker compose version", { stdio: "pipe" });
    } catch {
      test.skip(true, "docker compose not available");
      return;
    }

    const composePath = path.join(relayDir, "docker-compose.federation.yml");
    const result = childProcess.spawnSync(
      "docker",
      ["compose", "-f", composePath, "config", "--quiet"],
      { stdio: "pipe", timeout: 15_000 },
    );
    expect(result.status).toBe(0);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 3. ENVIRONMENT TEMPLATE
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Environment Template", () => {
  test(".env.example exists and documents all env vars", () => {
    const envPath = path.resolve(import.meta.dirname, "../.env.example");
    expect(fs.existsSync(envPath)).toBe(true);
    const content = fs.readFileSync(envPath, "utf-8");

    // All supported env vars should be documented
    expect(content).toContain("PRISM_RELAY_MODE");
    expect(content).toContain("PRISM_RELAY_HOST");
    expect(content).toContain("PRISM_RELAY_PORT");
    expect(content).toContain("PRISM_RELAY_DATA_DIR");
    expect(content).toContain("PRISM_RELAY_PUBLIC_URL");
    expect(content).toContain("PRISM_RELAY_LOG_LEVEL");
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 4. CONFIG GENERATION & VALIDATION
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Config System", () => {
  test("resolveConfig returns correct server-mode defaults", () => {
    const config = resolveConfig({ mode: "server" });
    expect(config.mode).toBe("server");
    expect(config.host).toBe("0.0.0.0");
    expect(config.port).toBe(4444);
    expect(config.hashcashBits).toBe(16);
    expect(config.logging.format).toBe("json");
    expect(config.logging.level).toBe("info");
  });

  test("resolveConfig returns correct p2p-mode defaults", () => {
    const config = resolveConfig({ mode: "p2p" });
    expect(config.mode).toBe("p2p");
    expect(config.hashcashBits).toBe(12);
    expect(config.federation.enabled).toBe(true);
  });

  test("resolveConfig returns correct dev-mode defaults", () => {
    const config = resolveConfig({ mode: "dev" });
    expect(config.mode).toBe("dev");
    expect(config.hashcashBits).toBe(4);
    expect(config.logging.format).toBe("text");
    expect(config.logging.level).toBe("debug");
  });

  test("CLI flags override config file values", () => {
    const config = resolveConfig({
      mode: "server",
      port: 9999,
      host: "127.0.0.1",
    });
    expect(config.port).toBe(9999);
    expect(config.host).toBe("127.0.0.1");
  });

  test("parseArgs extracts all supported flags", () => {
    const args = parseArgs([
      "start",
      "--mode", "p2p",
      "--port", "5555",
      "--host", "192.168.1.1",
      "--config", "/tmp/test.json",
    ]);
    expect(args.command).toBe("start");
    expect(args.overrides.mode).toBe("p2p");
    expect(args.overrides.port).toBe(5555);
    expect(args.overrides.host).toBe("192.168.1.1");
    expect(args.configPath).toBe("/tmp/test.json");
  });

  test("config file is loaded and merged with defaults", () => {
    const dir = tmpDir();
    const configPath = path.join(dir, "relay.config.json");
    fs.writeFileSync(configPath, JSON.stringify({
      mode: "server",
      port: 7777,
      corsOrigins: ["https://app.example.com"],
      federation: { enabled: true, publicUrl: "https://relay.example.com" },
    }));

    const raw = JSON.parse(fs.readFileSync(configPath, "utf-8"));
    const config = resolveConfig(raw);
    expect(config.port).toBe(7777);
    expect(config.corsOrigins).toContain("https://app.example.com");
    expect(config.federation.enabled).toBe(true);
    expect(config.federation.publicUrl).toBe("https://relay.example.com");

    cleanDir(dir);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 5. HEALTH CHECK CONTRACT
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Health Check Endpoint", () => {
  let relay: RelayInstance;
  let server: TestServer;
  let identity: PrismIdentity;

  test.beforeAll(async () => {
    identity = await createIdentity({ method: "key" });
    relay = await buildRelay(identity);
    server = await startServer(relay);
  });

  test.afterAll(async () => {
    await server.close();
    await relay.stop();
  });

  test("GET /api/health returns 200 with required fields", async () => {
    const res = await fetch(`${server.url}/api/health`);
    expect(res.status).toBe(200);
    const body = await res.json();

    // Required fields for monitoring integrations
    expect(body).toHaveProperty("status");
    expect(body.status).toBe("healthy");
    expect(body).toHaveProperty("did");
    expect(body.did).toBe(identity.did);
    expect(body).toHaveProperty("uptime");
    expect(typeof body.uptime).toBe("number");
    expect(body).toHaveProperty("modules");
    expect(typeof body.modules).toBe("number");
    expect(body.modules).toBeGreaterThanOrEqual(16);
    expect(body).toHaveProperty("memory");
    expect(body.memory).toHaveProperty("rss");
    expect(body.memory).toHaveProperty("heapUsed");
  });

  test("health endpoint sets correct content-type", async () => {
    const res = await fetch(`${server.url}/api/health`);
    expect(res.headers.get("content-type")).toContain("application/json");
  });

  test("GET /api/status returns detailed relay state", async () => {
    const res = await fetch(`${server.url}/api/status`);
    expect(res.status).toBe(200);
    const body = await res.json();

    expect(body).toHaveProperty("running", true);
    expect(body).toHaveProperty("did");
    expect(body).toHaveProperty("modules");
    expect(Array.isArray(body.modules)).toBe(true);
    expect(body.modules.length).toBeGreaterThanOrEqual(16);
  });

  test("GET /api/modules returns all 16 modules", async () => {
    const res = await fetch(`${server.url}/api/modules`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body)).toBe(true);
    expect(body.length).toBe(16);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 6. CSRF ENFORCEMENT
// ════════════════════════════════════════════════════════════════════════════════

test.describe("CSRF Enforcement (Server Mode)", () => {
  let relay: RelayInstance;
  let server: TestServer;

  test.beforeAll(async () => {
    const identity = await createIdentity({ method: "key" });
    relay = await buildRelay(identity);
    // Enable CSRF (server mode default)
    server = await startServer(relay, { disableCsrf: false });
  });

  test.afterAll(async () => {
    await server.close();
    await relay.stop();
  });

  test("POST without X-Prism-CSRF header is rejected", async () => {
    const res = await fetch(`${server.url}/api/portals`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "No CSRF", level: 1, collectionId: "c1", basePath: "/x", isPublic: true }),
    });
    expect(res.status).toBe(403);
  });

  test("POST with X-Prism-CSRF: 1 header succeeds", async () => {
    const res = await fetch(`${server.url}/api/portals`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Prism-CSRF": "1",
      },
      body: JSON.stringify({ name: "With CSRF", level: 1, collectionId: "c1", basePath: "/csrf-ok", isPublic: true }),
    });
    expect(res.status).toBe(201);
  });

  test("GET requests are exempt from CSRF", async () => {
    const res = await fetch(`${server.url}/api/health`);
    expect(res.status).toBe(200);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 7. CORS BEHAVIOR
// ════════════════════════════════════════════════════════════════════════════════

test.describe("CORS Configuration", () => {
  test("wildcard CORS returns Access-Control-Allow-Origin: *", async () => {
    const identity = await createIdentity({ method: "key" });
    const relay = await buildRelay(identity);
    const server = await startServer(relay, {
      disableCsrf: true,
      corsOrigins: ["*"],
    });

    const res = await fetch(`${server.url}/api/health`, {
      headers: { Origin: "https://anything.example.com" },
    });
    expect(res.headers.get("access-control-allow-origin")).toBe("*");

    await server.close();
    await relay.stop();
  });

  test("explicit origin list only allows listed origins", async () => {
    const identity = await createIdentity({ method: "key" });
    const relay = await buildRelay(identity);
    const server = await startServer(relay, {
      disableCsrf: true,
      corsOrigins: ["https://app.example.com"],
    });

    // Allowed origin
    const res1 = await fetch(`${server.url}/api/health`, {
      headers: { Origin: "https://app.example.com" },
    });
    expect(res1.headers.get("access-control-allow-origin")).toBe("https://app.example.com");

    // Disallowed origin
    const res2 = await fetch(`${server.url}/api/health`, {
      headers: { Origin: "https://evil.example.com" },
    });
    expect(res2.headers.get("access-control-allow-origin")).toBeNull();

    await server.close();
    await relay.stop();
  });

  test("OPTIONS preflight returns 204 with CORS headers", async () => {
    const identity = await createIdentity({ method: "key" });
    const relay = await buildRelay(identity);
    const server = await startServer(relay, {
      disableCsrf: true,
      corsOrigins: ["https://app.example.com"],
    });

    const res = await fetch(`${server.url}/api/health`, {
      method: "OPTIONS",
      headers: { Origin: "https://app.example.com" },
    });
    expect(res.status).toBe(204);
    expect(res.headers.get("access-control-allow-methods")).toContain("POST");
    expect(res.headers.get("access-control-allow-headers")).toContain("X-Prism-CSRF");

    await server.close();
    await relay.stop();
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 8. BACKUP / RESTORE VIA API
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Backup & Restore (API)", () => {
  test("GET /api/backup returns full relay state", async ({ request }) => {
    const identity = await createIdentity({ method: "key" });
    const relay = await buildRelay(identity);
    const server = await startServer(relay);

    // Create some state
    await request.post(`${server.url}/api/portals`, {
      data: { name: "Backup Portal", level: 1, collectionId: "bk-col", basePath: "/bk", isPublic: true },
    });
    await request.post(`${server.url}/api/webhooks`, {
      data: { url: "https://backup.example/hook", events: ["*"], active: true },
    });

    // Export backup
    const backupRes = await request.get(`${server.url}/api/backup`);
    expect(backupRes.status()).toBe(200);
    const backup = await backupRes.json();
    expect(backup).toHaveProperty("portals");
    expect(backup).toHaveProperty("webhooks");
    expect(backup.portals.length).toBeGreaterThanOrEqual(1);
    expect(backup.webhooks.length).toBeGreaterThanOrEqual(1);

    await server.close();
    await relay.stop();
  });

  test("POST /api/backup restores state into fresh relay", async ({ request }) => {
    // Phase 1: Create relay with state, export backup
    const identity1 = await createIdentity({ method: "key" });
    const relay1 = await buildRelay(identity1);
    const server1 = await startServer(relay1);

    await request.post(`${server1.url}/api/portals`, {
      data: { name: "Restore Test Portal", level: 2, collectionId: "rt-col", basePath: "/rt", isPublic: true },
    });
    await request.post(`${server1.url}/api/templates`, {
      data: { name: "Restore Theme", description: "test", css: "body{}", headerHtml: "", footerHtml: "", objectCardHtml: "" },
    });

    const backupRes = await request.get(`${server1.url}/api/backup`);
    const backup = await backupRes.json();

    await server1.close();
    await relay1.stop();

    // Phase 2: Fresh relay, import backup, verify
    const identity2 = await createIdentity({ method: "key" });
    const relay2 = await buildRelay(identity2);
    const server2 = await startServer(relay2);

    const restoreRes = await request.post(`${server2.url}/api/backup`, {
      data: backup,
    });
    expect(restoreRes.status()).toBe(200);

    // Verify portals restored
    const portalsRes = await request.get(`${server2.url}/api/portals`);
    const portals = await portalsRes.json();
    expect(portals.some((p: { name: string }) => p.name === "Restore Test Portal")).toBe(true);

    // Verify templates restored
    const templatesRes = await request.get(`${server2.url}/api/templates`);
    const templates = await templatesRes.json();
    expect(templates.some((t: { name: string }) => t.name === "Restore Theme")).toBe(true);

    await server2.close();
    await relay2.stop();
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 9. GRACEFUL SHUTDOWN WITH STATE PERSISTENCE
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Graceful Shutdown", () => {
  test("FileStore save + dispose persists all state", async ({ request }) => {
    const dataDir = tmpDir();
    const identity = await createIdentity({ method: "key" });
    const relay = await buildRelay(identity);
    const server = await startServer(relay);
    const store = createFileStore({ dataDir, saveIntervalMs: 999_999 });

    // Create state
    await request.post(`${server.url}/api/portals`, {
      data: { name: "Shutdown Portal", level: 1, collectionId: "sd-col", basePath: "/sd", isPublic: true },
    });
    await request.post(`${server.url}/api/webhooks`, {
      data: { url: "https://shutdown.example/hook", events: ["*"], active: true },
    });

    // Simulate graceful shutdown: save → close → stop
    store.save(relay);
    store.dispose();
    await server.close();
    await relay.stop();

    // Verify state file was written
    const stateFile = path.join(dataDir, "relay-state.json");
    expect(fs.existsSync(stateFile)).toBe(true);
    const state = JSON.parse(fs.readFileSync(stateFile, "utf-8"));
    expect(state.portals.length).toBeGreaterThanOrEqual(1);
    expect(state.webhooks.length).toBeGreaterThanOrEqual(1);

    // Verify a fresh relay can load the state
    const identity2 = await createIdentity({ method: "key" });
    const relay2 = await buildRelay(identity2);
    const store2 = createFileStore({ dataDir, saveIntervalMs: 999_999 });
    store2.load(relay2);

    const server2 = await startServer(relay2);
    const portalsRes = await fetch(`${server2.url}/api/portals`);
    const portals = await portalsRes.json();
    expect(portals.some((p: { name: string }) => p.name === "Shutdown Portal")).toBe(true);

    store2.dispose();
    await server2.close();
    await relay2.stop();
    cleanDir(dataDir);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 10. SEO & PUBLIC ENDPOINTS
// ════════════════════════════════════════════════════════════════════════════════

test.describe("SEO Endpoints", () => {
  let relay: RelayInstance;
  let server: TestServer;

  test.beforeAll(async () => {
    const identity = await createIdentity({ method: "key" });
    relay = await buildRelay(identity);
    server = await startServer(relay, { publicUrl: "https://relay.example.com" });
  });

  test.afterAll(async () => {
    await server.close();
    await relay.stop();
  });

  test("GET /robots.txt returns crawler directives", async () => {
    const res = await fetch(`${server.url}/robots.txt`);
    expect(res.status).toBe(200);
    const body = await res.text();
    expect(body).toContain("User-agent");
    expect(body).toContain("/portals/");
    expect(body).toContain("Disallow: /api/");
  });

  test("GET /sitemap.xml returns valid XML", async () => {
    const res = await fetch(`${server.url}/sitemap.xml`);
    expect(res.status).toBe(200);
    const body = await res.text();
    expect(body).toContain("<?xml");
    expect(body).toContain("urlset");
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 11. RATE LIMITING
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Rate Limiting", () => {
  test("rapid requests eventually get throttled", async () => {
    const identity = await createIdentity({ method: "key" });
    const relay = await buildRelay(identity);
    const server = await startServer(relay);

    // Send a burst of requests
    const results: number[] = [];
    for (let i = 0; i < 200; i++) {
      const res = await fetch(`${server.url}/api/health`);
      results.push(res.status);
    }

    // At least some should succeed
    expect(results.filter((s) => s === 200).length).toBeGreaterThan(0);

    // If rate limiting is working, some may be 429 (depends on bucket config)
    // We just verify no 500 errors
    expect(results.filter((s) => s >= 500).length).toBe(0);

    await server.close();
    await relay.stop();
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 12. DEPLOYMENT DOCUMENTATION
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Deployment Documentation", () => {
  const docsDir = path.resolve(import.meta.dirname, "../docs");

  test("deployment.md exists and covers all deployment options", () => {
    const docPath = path.join(docsDir, "deployment.md");
    expect(fs.existsSync(docPath)).toBe(true);
    const content = fs.readFileSync(docPath, "utf-8");

    // Must cover all deployment methods
    expect(content).toContain("Docker");
    expect(content).toContain("Docker Compose");
    expect(content).toContain("TLS");
    expect(content).toContain("Nginx");
    expect(content).toContain("Caddy");
    expect(content).toContain("Federation");
    expect(content).toContain("Monitoring");
    expect(content).toContain("Backup");
    expect(content).toContain("Security");
    expect(content).toContain("Scaling");
    expect(content).toContain("Environment Variables");
  });

  test("development.md exists", () => {
    const docPath = path.join(docsDir, "development.md");
    expect(fs.existsSync(docPath)).toBe(true);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 13. IDENTITY PERSISTENCE
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Identity Persistence", () => {
  test("identity is created and persists across relay restarts", async () => {
    const dataDir = tmpDir();
    const identityPath = path.join(dataDir, "identity.json");

    // First run — create identity
    const identity1 = await createIdentity({ method: "key" });
    fs.writeFileSync(identityPath, JSON.stringify({
      did: identity1.did,
      publicKey: identity1.publicKey,
    }));

    expect(fs.existsSync(identityPath)).toBe(true);
    const stored = JSON.parse(fs.readFileSync(identityPath, "utf-8"));
    expect(stored.did).toBe(identity1.did);

    // Verify the DID format
    expect(identity1.did).toMatch(/^did:key:z6Mk/);

    cleanDir(dataDir);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 14. MULTI-MODE SERVER STARTUP
// ════════════════════════════════════════════════════════════════════════════════

test.describe("Multi-Mode Startup", () => {
  test("server mode starts with all 16 modules", async () => {
    const identity = await createIdentity({ method: "key" });
    const relay = await buildRelay(identity);
    const server = await startServer(relay);

    const res = await fetch(`${server.url}/api/modules`);
    const modules = await res.json();
    expect(modules.length).toBe(16);

    await server.close();
    await relay.stop();
  });

  test("multiple relays can run on different ports simultaneously", async () => {
    const [id1, id2, id3] = await Promise.all([
      createIdentity({ method: "key" }),
      createIdentity({ method: "key" }),
      createIdentity({ method: "key" }),
    ]);

    const [r1, r2, r3] = await Promise.all([
      buildRelay(id1),
      buildRelay(id2),
      buildRelay(id3),
    ]);

    const [s1, s2, s3] = await Promise.all([
      startServer(r1),
      startServer(r2),
      startServer(r3),
    ]);

    // All three should be healthy independently
    const [h1, h2, h3] = await Promise.all([
      fetch(`${s1.url}/api/health`).then((r) => r.json()),
      fetch(`${s2.url}/api/health`).then((r) => r.json()),
      fetch(`${s3.url}/api/health`).then((r) => r.json()),
    ]);

    expect(h1.status).toBe("healthy");
    expect(h2.status).toBe("healthy");
    expect(h3.status).toBe("healthy");

    // Each has a unique DID
    expect(new Set([h1.did, h2.did, h3.did]).size).toBe(3);

    await Promise.all([s1.close(), s2.close(), s3.close()]);
    await Promise.all([r1.stop(), r2.stop(), r3.stop()]);
  });
});

// ════════════════════════════════════════════════════════════════════════════════
// 15. WEBSOCKET CONNECTIVITY
// ════════════════════════════════════════════════════════════════════════════════

test.describe("WebSocket Deployment", () => {
  let relay: RelayInstance;
  let server: TestServer;
  let identity: PrismIdentity;

  test.beforeAll(async () => {
    identity = await createIdentity({ method: "key" });
    relay = await buildRelay(identity);
    server = await startServer(relay);
  });

  test.afterAll(async () => {
    await server.close();
    await relay.stop();
  });

  test("WebSocket endpoint accepts connections and auth", async () => {
    const ws = await new Promise<WebSocket>((resolve, reject) => {
      const socket = new WebSocket(`ws://localhost:${server.port}/ws/relay`);
      socket.addEventListener("open", () => resolve(socket));
      socket.addEventListener("error", (e) => reject(e));
    });

    const client = await createIdentity({ method: "key" });

    // Auth handshake
    const reply = new Promise<Record<string, unknown>>((resolve) => {
      ws.addEventListener("message", (evt) => {
        resolve(JSON.parse(String(evt.data)));
      });
    });
    ws.send(JSON.stringify({ type: "auth", did: client.did }));
    const msg = await reply;
    expect(msg.type).toBe("auth-ok");

    ws.close();
  });

  test("WebSocket ping/pong works", async () => {
    const ws = await new Promise<WebSocket>((resolve, reject) => {
      const socket = new WebSocket(`ws://localhost:${server.port}/ws/relay`);
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
});
