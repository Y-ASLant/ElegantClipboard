import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30000,
  retries: 1,
  use: {
    baseURL: "http://localhost:14200",
    headless: true,
    viewport: { width: 360, height: 640 },
    actionTimeout: 5000,
  },
  webServer: {
    command: "npm run dev",
    port: 14200,
    reuseExistingServer: true,
    timeout: 30000,
  },
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
});
