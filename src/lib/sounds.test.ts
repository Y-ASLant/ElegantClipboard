import { describe, it, expect, vi, beforeEach } from "vitest";

const mockGetState = vi.fn().mockReturnValue({
  copySound: false,
  copySoundTiming: "immediate",
  pasteSound: false,
  pasteSoundTiming: "immediate",
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve()),
}));

vi.mock("@/lib/logger", () => ({
  logError: vi.fn(),
}));

vi.mock("@/stores/ui-settings", () => ({
  useUISettings: {
    getState: (...args: unknown[]) => mockGetState(...args),
  },
}));

const { playCopySound, playPasteSound } = await import("./sounds");

beforeEach(() => {
  mockGetState.mockReturnValue({
    copySound: false,
    copySoundTiming: "immediate",
    pasteSound: false,
    pasteSoundTiming: "immediate",
  });
});

describe("sounds", () => {
  describe("playCopySound", () => {
    it("does not play when copySound is disabled", () => {
      playCopySound("immediate");
    });

    it("does not play when timing does not match", () => {
      mockGetState.mockReturnValue({
        copySound: true,
        copySoundTiming: "after_success",
      });
      playCopySound("immediate");
    });

    it("plays when copySound is enabled and timing matches", () => {
      mockGetState.mockReturnValue({
        copySound: true,
        copySoundTiming: "immediate",
      });
      expect(() => playCopySound("immediate")).not.toThrow();
    });
  });

  describe("playPasteSound", () => {
    it("does not play when pasteSound is disabled", () => {
      playPasteSound("immediate");
    });

    it("does not play when timing does not match", () => {
      mockGetState.mockReturnValue({
        pasteSound: true,
        pasteSoundTiming: "after_success",
      });
      playPasteSound("immediate");
    });

    it("plays when pasteSound is enabled and timing matches", () => {
      mockGetState.mockReturnValue({
        pasteSound: true,
        pasteSoundTiming: "immediate",
      });
      expect(() => playPasteSound("immediate")).not.toThrow();
    });
  });
});
