import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  formatTime,
  formatCharCount,
  formatSize,
  contentTypeConfig,
  getFileNameFromPath,
  parseFilePaths,
  isImageFile,
} from "./format";

describe("formatTime", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-12T14:30:00"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("formats today's time", () => {
    const result = formatTime("2026-06-12T10:15:00");
    expect(result).toBe("今天 10:15");
  });

  it("formats yesterday's time", () => {
    const result = formatTime("2026-06-11T08:45:00");
    expect(result).toBe("昨天 08:45");
  });

  it("formats older dates", () => {
    const result = formatTime("2026-05-01T16:00:00");
    expect(result).toBe("05-01 16:00");
  });

  it("formats relative time - just now", () => {
    const now = new Date("2026-06-12T14:30:00");
    vi.setSystemTime(now);
    const thirtySecAgo = new Date(now.getTime() - 30 * 1000).toISOString();
    expect(formatTime(thirtySecAgo, "relative")).toBe("刚刚");
  });

  it("formats relative time - minutes ago", () => {
    const now = new Date("2026-06-12T14:30:00");
    vi.setSystemTime(now);
    const fiveMinAgo = new Date(now.getTime() - 5 * 60 * 1000).toISOString();
    expect(formatTime(fiveMinAgo, "relative")).toBe("5 分钟前");
  });

  it("formats relative time - hours ago", () => {
    const now = new Date("2026-06-12T14:30:00");
    vi.setSystemTime(now);
    const threeHoursAgo = new Date(now.getTime() - 3 * 60 * 60 * 1000).toISOString();
    expect(formatTime(threeHoursAgo, "relative")).toBe("3 小时前");
  });

  it("formats relative time - days ago", () => {
    const now = new Date("2026-06-12T14:30:00");
    vi.setSystemTime(now);
    const tenDaysAgo = new Date(now.getTime() - 10 * 24 * 60 * 60 * 1000).toISOString();
    expect(formatTime(tenDaysAgo, "relative")).toBe("10 天前");
  });
});

describe("formatCharCount", () => {
  it("returns '0 字符' for null", () => {
    expect(formatCharCount(null)).toBe("0 字符");
  });

  it("returns '0 字符' for 0", () => {
    expect(formatCharCount(0)).toBe("0 字符");
  });

  it("formats small counts", () => {
    expect(formatCharCount(42)).toBe("42 字符");
  });

  it("formats counts with locale separators", () => {
    expect(formatCharCount(1234)).toBe("1,234 字符");
  });

  it("formats large counts in 万", () => {
    expect(formatCharCount(15000)).toBe("1.5万 字符");
  });

  it("formats exact 10000", () => {
    expect(formatCharCount(10000)).toBe("1.0万 字符");
  });
});

describe("formatSize", () => {
  it("formats bytes", () => {
    expect(formatSize(512)).toBe("512 B");
  });

  it("formats kilobytes", () => {
    expect(formatSize(2048)).toBe("2.0 KB");
  });

  it("formats megabytes", () => {
    expect(formatSize(1048576)).toBe("1.00 MB");
  });

  it("formats 1 KB exactly", () => {
    expect(formatSize(1024)).toBe("1.0 KB");
  });

  it("formats large files", () => {
    expect(formatSize(5242880)).toBe("5.00 MB");
  });
});

describe("contentTypeConfig", () => {
  it("has all content types", () => {
    expect(contentTypeConfig.text).toBeDefined();
    expect(contentTypeConfig.html).toBeDefined();
    expect(contentTypeConfig.rtf).toBeDefined();
    expect(contentTypeConfig.image).toBeDefined();
    expect(contentTypeConfig.files).toBeDefined();
  });

  it("has labels for each type", () => {
    expect(contentTypeConfig.text.label).toBe("文本");
    expect(contentTypeConfig.image.label).toBe("图片");
    expect(contentTypeConfig.files.label).toBe("文件");
  });
});

describe("getFileNameFromPath", () => {
  it("extracts filename from unix path", () => {
    expect(getFileNameFromPath("/home/user/file.txt")).toBe("file.txt");
  });

  it("extracts filename from windows path", () => {
    expect(getFileNameFromPath("C:\\Users\\test\\doc.pdf")).toBe("doc.pdf");
  });

  it("returns path if no separator", () => {
    expect(getFileNameFromPath("filename")).toBe("filename");
  });

  it("handles trailing slash - returns path as-is", () => {
    expect(getFileNameFromPath("/path/to/")).toBe("/path/to/");
  });
});

describe("parseFilePaths", () => {
  it("parses valid json array", () => {
    expect(parseFilePaths('["a.txt","b.txt"]')).toEqual(["a.txt", "b.txt"]);
  });

  it("returns empty for null", () => {
    expect(parseFilePaths(null)).toEqual([]);
  });

  it("returns empty for invalid json", () => {
    expect(parseFilePaths("not json")).toEqual([]);
  });

  it("returns empty for non-array json", () => {
    expect(parseFilePaths('{"key":"value"}')).toEqual([]);
  });
});

describe("isImageFile", () => {
  it("returns true for png", () => {
    expect(isImageFile("photo.png")).toBe(true);
  });

  it("returns true for jpg", () => {
    expect(isImageFile("image.JPG")).toBe(true);
  });

  it("returns true for gif", () => {
    expect(isImageFile("anim.gif")).toBe(true);
  });

  it("returns false for txt", () => {
    expect(isImageFile("doc.txt")).toBe(false);
  });

  it("returns false for unknown extension", () => {
    expect(isImageFile("file.xyz")).toBe(false);
  });
});
