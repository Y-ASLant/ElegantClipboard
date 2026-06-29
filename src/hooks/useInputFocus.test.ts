import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve()),
}));

describe("useInputFocus", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("exports cancelPendingFocusRestore", async () => {
    const mod = await import("./useInputFocus");
    expect(typeof mod.cancelPendingFocusRestore).toBe("function");
  });

  it("releaseWebViewFocus blurs active element", async () => {
    const input = document.createElement("input");
    document.body.appendChild(input);
    input.focus();
    expect(document.activeElement).toBe(input);

    const mod = await import("./useInputFocus");
    mod.releaseWebViewFocus();
    expect(document.activeElement).not.toBe(input);

    document.body.removeChild(input);
  });

  it("exports useInputFocus hook", async () => {
    const mod = await import("./useInputFocus");
    expect(typeof mod.useInputFocus).toBe("function");
  });
});
