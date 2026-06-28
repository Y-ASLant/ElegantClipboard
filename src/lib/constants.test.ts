import { describe, it, expect } from "vitest";
import { TOOLBAR_BUTTON_REGISTRY, GROUPS } from "./constants";

describe("TOOLBAR_BUTTON_REGISTRY", () => {
  it("has clear button", () => {
    expect(TOOLBAR_BUTTON_REGISTRY.clear).toBeDefined();
    expect(TOOLBAR_BUTTON_REGISTRY.clear.label).toBe("清空历史");
  });

  it("has pin button", () => {
    expect(TOOLBAR_BUTTON_REGISTRY.pin).toBeDefined();
    expect(TOOLBAR_BUTTON_REGISTRY.pin.label).toBe("锁定窗口");
  });

  it("has batch button", () => {
    expect(TOOLBAR_BUTTON_REGISTRY.batch).toBeDefined();
    expect(TOOLBAR_BUTTON_REGISTRY.batch.label).toBe("批量选择");
  });

  it("has settings button", () => {
    expect(TOOLBAR_BUTTON_REGISTRY.settings).toBeDefined();
    expect(TOOLBAR_BUTTON_REGISTRY.settings.label).toBe("设置");
  });

  it("all buttons have descriptions", () => {
    for (const [key, value] of Object.entries(TOOLBAR_BUTTON_REGISTRY)) {
      expect(value.description).toBeTruthy();
      expect(value.label).toBeTruthy();
    }
  });
});

describe("GROUPS", () => {
  it("has 4 groups", () => {
    expect(GROUPS).toHaveLength(4);
  });

  it("first group is all", () => {
    expect(GROUPS[0].label).toBe("全部");
    expect(GROUPS[0].value).toBeNull();
  });

  it("second group is favorites", () => {
    expect(GROUPS[1].label).toBe("收藏");
    expect(GROUPS[1].value).toBe("__favorites__");
  });

  it("third group is text", () => {
    expect(GROUPS[2].label).toBe("文本");
    expect(GROUPS[2].value).toContain("text");
  });

  it("fourth group is other", () => {
    expect(GROUPS[3].label).toBe("其它");
    expect(GROUPS[3].value).toContain("image");
  });
});
