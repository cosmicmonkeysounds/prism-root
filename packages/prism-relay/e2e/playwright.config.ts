import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: [
    "relay.spec.ts",
    "production-readiness.spec.ts",
    "deployment.spec.ts",
    "docker.spec.ts",
    "modular-auth.spec.ts",
  ],
  timeout: 120_000,
  retries: 0,
  use: {
    headless: true,
  },
});
