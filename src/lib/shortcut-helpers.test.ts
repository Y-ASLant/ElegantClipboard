import { describe, it, expect } from "vitest";
import { KEY_CODE_MAP } from "./shortcut-helpers";

describe("KEY_CODE_MAP", () => {
  it("maps Space", () => {
    expect(KEY_CODE_MAP.Space).toBe("Space");
  });

  it("maps Tab", () => {
    expect(KEY_CODE_MAP.Tab).toBe("Tab");
  });

  it("maps Enter", () => {
    expect(KEY_CODE_MAP.Enter).toBe("Enter");
  });

  it("maps Escape to Esc", () => {
    expect(KEY_CODE_MAP.Escape).toBe("Esc");
  });

  it("maps arrow keys", () => {
    expect(KEY_CODE_MAP.ArrowUp).toBe("Up");
    expect(KEY_CODE_MAP.ArrowDown).toBe("Down");
    expect(KEY_CODE_MAP.ArrowLeft).toBe("Left");
    expect(KEY_CODE_MAP.ArrowRight).toBe("Right");
  });

  it("maps Backspace", () => {
    expect(KEY_CODE_MAP.Backspace).toBe("Backspace");
  });

  it("maps Delete", () => {
    expect(KEY_CODE_MAP.Delete).toBe("Delete");
  });

  it("maps Home and End", () => {
    expect(KEY_CODE_MAP.Home).toBe("Home");
    expect(KEY_CODE_MAP.End).toBe("End");
  });

  it("maps PageUp and PageDown", () => {
    expect(KEY_CODE_MAP.PageUp).toBe("PageUp");
    expect(KEY_CODE_MAP.PageDown).toBe("PageDown");
  });

  it("maps Backquote", () => {
    expect(KEY_CODE_MAP.Backquote).toBe("`");
  });

  it("has 15 entries", () => {
    expect(Object.keys(KEY_CODE_MAP)).toHaveLength(15);
  });
});
