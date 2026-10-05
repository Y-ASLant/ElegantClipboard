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
