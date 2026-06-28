import { describe, it, expect } from "vitest";
import { cn } from "./utils";

describe("cn", () => {
  it("merges class names", () => {
    const result = cn("foo", "bar");
    expect(result).toBe("foo bar");
  });

  it("handles conditional classes", () => {
    const result = cn("foo", false && "bar", "baz");
    expect(result).toBe("foo baz");
  });

  it("deduplicates tailwind classes", () => {
    const result = cn("p-4", "p-8");
    expect(result).toBe("p-8");
  });

  it("handles undefined and null", () => {
    const result = cn("foo", undefined, null, "bar");
    expect(result).toBe("foo bar");
  });

  it("handles empty input", () => {
    const result = cn();
    expect(result).toBe("");
  });

  it("merges conflicting tailwind classes", () => {
    const result = cn("text-red-500", "text-blue-500");
    expect(result).toBe("text-blue-500");
  });
});
