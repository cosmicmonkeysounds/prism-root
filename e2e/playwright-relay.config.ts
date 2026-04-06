import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  testMatch: "relay.spec.ts",
  timeout: 60_000,
  retries: 0,
  use: {
    headless: true,
  },
});
