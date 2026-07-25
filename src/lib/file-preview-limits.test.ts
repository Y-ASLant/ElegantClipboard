import { describe, it, expect, beforeEach } from "vitest";
import {
  DEFAULT_MAX_IMAGE_SIZE_KB,
  isUncPath,
  isFileTooLargeForPreview,
  previewLimitBytes,
  setMaxLocalPreviewBytesFromKb,
  shouldSkipFileImagePreview,
} from "@/lib/file-preview-limits";

describe("isUncPath", () => {
  it("returns true for UNC paths", () => {
    expect(isUncPath("\\\\server\\share\\file.png")).toBe(true);
    expect(isUncPath("\\\\NAS\\photos\\image.jpg")).toBe(true);
    expect(isUncPath("\\\\192.168.1.100\\data\\test.bmp")).toBe(true);
  });

  it("returns false for local paths", () => {
    expect(isUncPath("C:\\Users\\test\\file.png")).toBe(false);
    expect(isUncPath("D:\\photos\\image.jpg")).toBe(false);
    expect(isUncPath("/home/user/file.png")).toBe(false);
  });

  it("returns false for empty or relative paths", () => {
    expect(isUncPath("")).toBe(false);
    expect(isUncPath("file.png")).toBe(false);
    expect(isUncPath("./file.png")).toBe(false);
  });
});

describe("isFileTooLargeForPreview", () => {
  const MB = 1024 * 1024;

  beforeEach(() => {
    setMaxLocalPreviewBytesFromKb(DEFAULT_MAX_IMAGE_SIZE_KB);
  });

  describe("local paths (50MB limit)", () => {
    it("returns false for small files", () => {
      expect(isFileTooLargeForPreview("C:\\file.png", 10 * MB)).toBe(false);
    });

    it("returns false for files at exactly 50MB", () => {
      expect(isFileTooLargeForPreview("C:\\file.png", 50 * MB)).toBe(false);
    });

    it("returns true for files over 50MB", () => {
      expect(isFileTooLargeForPreview("C:\\file.png", 50 * MB + 1)).toBe(true);
    });
  });

  describe("UNC paths (10MB limit)", () => {
    it("returns false for small files on network", () => {
      expect(isFileTooLargeForPreview("\\\\server\\share\\file.png", 5 * MB)).toBe(false);
    });

    it("returns true for files over 10MB on network", () => {
      expect(isFileTooLargeForPreview("\\\\server\\share\\file.png", 10 * MB + 1)).toBe(true);
    });

    it("returns true when byteSize is missing on UNC", () => {
      expect(isFileTooLargeForPreview("\\\\server\\file.png")).toBe(true);
    });
  });

  describe("edge cases", () => {
    it("returns false for local paths when byteSize is undefined and default false", () => {
      expect(isFileTooLargeForPreview("C:\\file.png")).toBe(false);
    });

    it("returns true for local paths when byteSize is undefined and default true", () => {
      expect(isFileTooLargeForPreview("C:\\file.png", undefined, true)).toBe(true);
    });

    it("respects custom max_image_size_kb from settings", () => {
      setMaxLocalPreviewBytesFromKb(20480); // 20MB
      expect(isFileTooLargeForPreview("C:\\file.png", 20 * MB)).toBe(false);
      expect(isFileTooLargeForPreview("C:\\file.png", 20 * MB + 1)).toBe(true);
      expect(previewLimitBytes("C:\\file.png")).toBe(20 * MB);
    });

    it("uses safety cap when settings unlimited (0)", () => {
      setMaxLocalPreviewBytesFromKb(0);
      expect(previewLimitBytes("C:\\file.png")).toBe(100 * MB);
    });
  });
});

describe("shouldSkipFileImagePreview", () => {
  const MB = 1024 * 1024;

  beforeEach(() => {
    setMaxLocalPreviewBytesFromKb(DEFAULT_MAX_IMAGE_SIZE_KB);
  });

  it("returns true when backend marks too_large", () => {
    expect(shouldSkipFileImagePreview("C:\\small.png", 100, true)).toBe(true);
  });

  it("returns true for large file without backend flag", () => {
    expect(shouldSkipFileImagePreview("C:\\big.png", 60 * MB, false)).toBe(true);
  });

  it("returns false for small local file", () => {
    expect(shouldSkipFileImagePreview("C:\\small.png", 1 * MB, false)).toBe(false);
  });
});
