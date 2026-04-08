import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: ["relay.spec.ts", "production-readiness.spec.ts", "deployment.spec.ts"],
  timeout: 60_000,
  retries: 0,
  use: {
    headless: true,
  },
});
