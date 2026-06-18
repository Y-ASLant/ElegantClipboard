import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { logError } from "./logger";

vi.mock("@/components/ui/toast", () => ({
  showToast: vi.fn(),
}));

describe("logError", () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    consoleSpy.mockRestore();
  });

  it("logs message without error", () => {
    logError("test message");
    expect(consoleSpy).toHaveBeenCalledWith("[ElegantClipboard] test message");
  });

  it("logs message with error", () => {
    const error = new Error("test error");
    logError("test message", error);
    expect(consoleSpy).toHaveBeenCalledWith("[ElegantClipboard] test message", error);
  });

  it("logs message with string error", () => {
    logError("test message", "string error");
    expect(consoleSpy).toHaveBeenCalledWith("[ElegantClipboard] test message", "string error");
  });

  it("logs message with null error", () => {
    logError("test message", null);
    expect(consoleSpy).toHaveBeenCalledWith("[ElegantClipboard] test message", null);
  });
});
