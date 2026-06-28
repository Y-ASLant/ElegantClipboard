import { showToast } from "@/components/ui/toast";

export function logError(message: string, error?: unknown): void {
  if (error === undefined) {
    console.error(`[ElegantClipboard] ${message}`);
  } else {
    console.error(`[ElegantClipboard] ${message}`, error);
  }

  const userMessage = message.replace(/[:：]\s*$/, "");
  showToast(userMessage, "error");
}
