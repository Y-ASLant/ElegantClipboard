import { describe, it, expect, vi, beforeEach } from "vitest";
import { createLeaseManager } from "./lease-manager";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve(1)),
}));

describe("createLeaseManager", () => {
  let manager: ReturnType<typeof createLeaseManager>;

  beforeEach(() => {
    vi.clearAllMocks();
    manager = createLeaseManager("acquire_lease");
  });

  it("creates a manager with correct interface", () => {
    expect(typeof manager.acquire).toBe("function");
    expect(typeof manager.revoke).toBe("function");
    expect(typeof manager.isCurrent).toBe("function");
    expect(typeof manager.isWanted).toBe("function");
    expect(typeof manager.setWanted).toBe("function");
  });

  it("initial state is not wanted", () => {
    expect(manager.isWanted()).toBe(false);
  });

  it("acquire sets wanted to true", async () => {
    await manager.acquire();
    expect(manager.isWanted()).toBe(true);
  });

  it("acquire returns lease from invoke", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockResolvedValue(42);
    const lease = await manager.acquire();
    expect(lease).toBe(42);
  });

  it("isCurrent returns true for current lease", async () => {
    const lease = await manager.acquire();
    expect(manager.isCurrent(lease)).toBe(true);
  });

  it("isCurrent returns false for old lease", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockResolvedValueOnce(1);
    const lease1 = await manager.acquire();
    vi.mocked(invoke).mockResolvedValueOnce(2);
    await manager.acquire();
    expect(manager.isCurrent(lease1)).toBe(false);
  });

  it("revoke increments current lease", async () => {
    const lease = await manager.acquire();
    manager.revoke(lease);
    expect(manager.isCurrent(lease)).toBe(false);
    expect(manager.isWanted()).toBe(false);
  });

  it("revoke does nothing for wrong lease", async () => {
    await manager.acquire();
    manager.revoke(999);
    expect(manager.isWanted()).toBe(true);
  });

  it("setWanted updates wanted state", () => {
    manager.setWanted(true);
    expect(manager.isWanted()).toBe(true);
    manager.setWanted(false);
    expect(manager.isWanted()).toBe(false);
  });
});
