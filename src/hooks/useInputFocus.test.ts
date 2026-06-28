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

  it("exports focusWindowImmediately", async () => {
    const mod = await import("./useInputFocus");
    expect(typeof mod.focusWindowImmediately).toBe("function");
  });

  it("exports useInputFocus hook", async () => {
    const mod = await import("./useInputFocus");
    expect(typeof mod.useInputFocus).toBe("function");
  });
});
