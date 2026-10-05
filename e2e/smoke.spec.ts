import { expect, test } from "@playwright/test";
import { fixtureItem, installTauriFixture } from "./tauri-fixture";

test("copy stays pending until success and never reports success after native rejection", async ({ page }) => {
  let finish: (() => void) | undefined;
  let first = true;
  await installTauriFixture(page, {
    items: [fixtureItem(1, "text", "pending copy entry")],
    operation: async (command) => {
      if (command === "copy_to_clipboard") {
        if (first) { first = false; return new Promise<void>((resolve) => { finish = resolve; }); }
        throw { code: "clipboard_unavailable", detail: "private native diagnostic" };
      }
      return null;
    },
  });
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await expect(page.getByText("pending copy entry", { exact: true })).toBeVisible();
  const copy = page.getByRole("button", { name: "复制", exact: true });
  await copy.click();
  await expect.poll(() => Boolean(finish)).toBe(true);
  await expect(page.getByRole("status", { name: "已复制", exact: true })).toHaveCount(0);
  finish!();
  await expect(page.getByRole("status", { name: "已复制", exact: true })).toBeVisible();
  await expect(page.getByRole("status", { name: "已复制", exact: true })).toHaveCount(0);
  await copy.click();
  await expect(page.getByRole("alert")).toContainText("剪贴板");
  await expect(page.getByRole("alert")).not.toContainText("private native diagnostic");
  await expect(page.getByRole("status", { name: /已复制|已粘贴/ })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("keyboard paste uses authoritative failure handling for missing file resources", async ({ page }) => {
  await installTauriFixture(page, {
    items: [fixtureItem(1, "files", "E:\\fixture\\missing.exe")], missingIds: [1],
    operation: async (command) => {
      if (command === "paste_content") throw { code: "resource_missing", detail: "removed resource" };
      return null;
    },
  });
  await page.goto("/");
  await expect(page.getByText("(已失效)", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "复制", exact: true })).toBeDisabled();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("alert")).toContainText("粘贴");
  await expect(page.getByRole("alert")).not.toContainText("removed resource");
  await expect(page.getByRole("status", { name: /已复制|已粘贴/ })).toHaveCount(0);
});

test("cancelled native save is silent while a real rejection gets one localized failure", async ({ page }) => {
  let cancelled = false;
  await installTauriFixture(page, {
    items: [fixtureItem(1, "files", "E:\\fixture\\usable.exe")],
    operation: async (command) => {
      if (command === "save_file_as") { cancelled = true; return false; }
      if (command === "show_in_explorer") throw { code: "resource_missing", detail: "stale preview path" };
      return null;
    },
  });
  await page.goto("/");
  const title = page.locator("p").filter({ hasText: /^usable\.exe/ }).first();
  await expect(title).toBeVisible();
  await title.click({ button: "right" });
  const save = page.getByRole("menuitem", { name: "另存为", exact: true });
  await expect(save).not.toHaveAttribute("aria-disabled", "true");
  await save.click();
  await expect.poll(() => cancelled).toBe(true);
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("status", { name: /已复制|已粘贴/ })).toHaveCount(0);
  await title.click({ button: "right" });
  const explorer = page.getByRole("menuitem", { name: "在资源管理器中显示", exact: true });
  await expect(explorer).not.toHaveAttribute("aria-disabled", "true");
  await explorer.click();
  await expect(page.getByRole("alert")).toHaveCount(1);
  await expect(page.getByRole("alert")).toContainText("资源");
});

test("native shortcut failures share visible feedback and ignore malformed event data", async ({ page }) => {
  await installTauriFixture(page, { items: [fixtureItem(1, "text", "shortcut failure entry")] });
  await page.goto("/");
  await expect(page.getByText("shortcut failure entry", { exact: true })).toBeVisible();
  await page.evaluate(() => {
    // The fixture supplies this explicit event transport before app startup.
    const fixture = window as unknown as Window & { __fixtureEmit: (event: string, payload: unknown) => void };
    fixture.__fixtureEmit("clipboard-operation-failed", { operation: "paste", error: { code: "resource_missing", detail: "native secret" } });
  });
  await expect(page.getByRole("alert")).toHaveCount(1);
  await expect(page.getByRole("alert")).toContainText("粘贴");
  await expect(page.getByRole("alert")).not.toContainText("native secret");
  await page.getByRole("alert").click();
  await page.evaluate(() => {
    // Same typed fixture transport; payloads intentionally violate the native contract.
    const fixture = window as unknown as Window & { __fixtureEmit: (event: string, payload: unknown) => void };
    fixture.__fixtureEmit("clipboard-operation-failed", { operation: "paste", error: { code: "constructor", detail: "unsafe" } });
  });
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("failed keyboard deletion keeps the chosen item and both records", async ({ page }) => {
  const items = [fixtureItem(1, "text", "first entry"), fixtureItem(2, "text", "second entry")];
  let selected = 0;
  await installTauriFixture(page, {
    items,
    operation: async (command, args) => {
      if (command === "delete_clipboard_item") throw { code: "permission_denied", detail: "readonly" };
      if (command === "paste_content") selected = Number(args.id);
      return null;
    },
  });
  await page.goto("/");
  await expect(page.getByText("second entry", { exact: true })).toBeVisible();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Delete");
  await expect(page.getByRole("alert")).toContainText("删除");
  await expect(page.getByText("first entry", { exact: true })).toBeVisible();
  await expect(page.getByText("second entry", { exact: true })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect.poll(() => selected).toBe(2);
});

test("resource markers survive virtual scrolling and refresh without authorizing stale operations", async ({ page }) => {
  const missingIds = [1];
  await installTauriFixture(page, {
    items: [
      fixtureItem(1, "files", "E:\\fixture\\remembered.exe"),
      ...Array.from({ length: 80 }, (_, index) => fixtureItem(index + 2, "text", `scroll entry ${index}`)),
    ],
    missingIds,
  });
  await page.goto("/");
  const invalid = page.getByText("(已失效)", { exact: true });
  await expect(invalid).toBeVisible();

  let release!: () => void;
  let pending = new Promise<void>((resolve) => { release = resolve; });
  let checks = 0;
  await page.exposeFunction("__holdResourceCheck", async () => {
    checks++;
    await pending;
  });
  await page.evaluate(() => {
    // The fixture and gate are installed above, before wrapping this transport.
    const fixture = window as unknown as Window & {
      __fixtureInvoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
      __holdResourceCheck: () => Promise<void>;
      __resourceChecksCompleted?: number;
    };
    const invoke = fixture.__fixtureInvoke;
    fixture.__fixtureInvoke = async (command, args) => {
      if (command === "batch_get_item_file_status") await fixture.__holdResourceCheck();
      const result = await invoke(command, args);
      if (command === "batch_get_item_file_status") {
        fixture.__resourceChecksCompleted = (fixture.__resourceChecksCompleted ?? 0) + 1;
      }
      return result;
    };
  });

  const viewport = page.locator(".scroll-fade-container [data-overlayscrollbars-viewport]");
  await viewport.evaluate((element) => { element.scrollTop = element.scrollHeight; });
  await expect(invalid).toHaveCount(0);
  await expect(page.getByText("scroll entry 79", { exact: true })).toBeVisible();
  await viewport.evaluate((element) => { element.scrollTop = 0; });
  await expect.poll(() => checks).toBeGreaterThan(0);
  await expect(invalid).toBeVisible();
  const copy = page.getByRole("button", { name: "复制", exact: true }).first();
  await expect(copy).toBeDisabled();
  await page.screenshot({ path: "test-results/resource-status-remount.png" });
  release();
  // Wait for the completed result before starting a distinct window-show refresh.
  await expect.poll(async () => {
    const completed = await page.evaluate(() =>
      "__resourceChecksCompleted" in window ? window.__resourceChecksCompleted : 0,
    );
    return completed === checks;
  }).toBe(true);

  missingIds.length = 0;
  const beforeWindowShown = checks;
  pending = new Promise<void>((resolve) => { release = resolve; });
  await page.evaluate(() => {
    const fixture = window as unknown as Window & { __fixtureEmit: (event: string, payload: unknown) => void };
    fixture.__fixtureEmit("window-shown", null);
  });
  await expect.poll(() => checks).toBeGreaterThan(beforeWindowShown);
  await expect(invalid).toBeVisible();
  await expect(copy).toBeDisabled();
  release();
  await expect(invalid).toHaveCount(0);
  await expect(copy).toBeEnabled();

  const beforeMenuOpen = checks;
  pending = new Promise<void>((resolve) => { release = resolve; });
  await page.locator("p").filter({ hasText: /^remembered\.exe/ }).first().click({ button: "right" });
  await expect.poll(() => checks).toBeGreaterThan(beforeMenuOpen);
  const save = page.getByRole("menuitem", { name: "另存为", exact: true });
  await expect(save).toHaveAttribute("aria-disabled", "true");
  await expect(page.getByRole("menuitem", { name: "粘贴", exact: true })).toHaveAttribute("aria-disabled", "true");
  release();
  await expect(save).not.toHaveAttribute("aria-disabled", "true");
});
