import { invoke } from "@tauri-apps/api/core";

export interface LeaseManager {
  acquire: () => Promise<number>;
  revoke: (lease: number) => void;
  isCurrent: (lease: number) => boolean;
  isWanted: () => boolean;
  setWanted: (v: boolean) => void;
}

export function createLeaseManager(invokeCommand: string): LeaseManager {
  let currentLease = 0;
  let wanted = false;

  return {
    async acquire(): Promise<number> {
      const lease = await invoke<number>(invokeCommand);
      if (lease > currentLease) {
        currentLease = lease;
      }
      wanted = true;
      return lease;
    },
    revoke(lease: number): void {
      if (currentLease === lease) {
        currentLease += 1;
        wanted = false;
      }
    },
    isCurrent(lease: number): boolean {
      return currentLease === lease;
    },
    isWanted(): boolean {
      return wanted;
    },
    setWanted(v: boolean): void {
      wanted = v;
    },
  };
}
