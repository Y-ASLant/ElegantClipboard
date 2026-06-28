import { test, expect } from "@playwright/test";

// Tauri APIs (transformCallback, invoke, metadata) are unavailable outside
// the Tauri WebView runtime. Filter these expected errors in E2E tests
// running against a plain Vite dev server.
const TAURI_API_PATTERNS = [
  "transformCallback",
  "reading 'invoke'",
  "reading 'metadata'",
];

function isNonTauriError(msg: string): boolean {
  return !TAURI_API_PATTERNS.some((p) => msg.includes(p));
}

test.describe("App smoke tests", () => {
  test("page loads without errors", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (err) => {
      if (isNonTauriError(err.message)) errors.push(err.message);
    });

    await page.goto("/");
    await page.waitForTimeout(2000);

    expect(errors).toEqual([]);
  });

  test("renders main app container", async ({ page }) => {
    await page.goto("/");
    await page.waitForTimeout(1000);

    // Main app should render
    const app = page.locator('[data-tauri-drag-region], .h-screen');
    await expect(app.first()).toBeVisible();
  });

  test("search input is visible", async ({ page }) => {
    await page.goto("/");
    await page.waitForTimeout(1000);

    const searchInput = page.locator('input[placeholder*="搜索"]');
    await expect(searchInput).toBeVisible();
  });
});

test.describe("Settings page smoke tests", () => {
  test("settings page loads", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (err) => {
      if (isNonTauriError(err.message)) errors.push(err.message);
    });

    await page.goto("/settings");
    await page.waitForTimeout(2000);

    expect(errors).toEqual([]);
  });

  test("settings navigation is visible", async ({ page }) => {
    await page.goto("/settings");
    await page.waitForTimeout(1000);

    // Check for navigation items
    const nav = page.locator("nav");
    await expect(nav).toBeVisible();
  });
});
