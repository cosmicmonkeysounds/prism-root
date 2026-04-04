import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  retries: 0,
  use: {
    baseURL: "http://localhost:1420",
    headless: true,
  },
  webServer: {
    command: "pnpm --filter @prism/studio dev",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env["CI"],
    timeout: 30_000,
  },
});
