export function logError(message: string, error?: unknown): void {
  if (error === undefined) {
    console.error(`[ElegantClipboard] ${message}`);
    return;
  }

  console.error(`[ElegantClipboard] ${message}`, error);
}

