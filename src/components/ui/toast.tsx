import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

export interface ToastItem {
  id: number;
  message: string;
  type: "error" | "success" | "info";
}

let nextId = 0;
const listeners = new Set<(toast: ToastItem) => void>();

export function showToast(message: string, type: ToastItem["type"] = "error") {
  const toast: ToastItem = { id: nextId++, message, type };
  listeners.forEach((fn) => fn(toast));
}

const DURATION = 3500;

export function Toaster() {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const timersRef = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const dismiss = useCallback((id: number) => {
    const timer = timersRef.current.get(id);
    if (timer) clearTimeout(timer);
    timersRef.current.delete(id);
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  useEffect(() => {
    const handler = (toast: ToastItem) => {
      setToasts((prev) => {
        const next = [...prev, toast];
        if (next.length > 3) return next.slice(-3);
        return next;
      });
      const timer = setTimeout(() => dismiss(toast.id), DURATION);
      timersRef.current.set(toast.id, timer);
    };
    listeners.add(handler);
    return () => {
      listeners.delete(handler);
      timersRef.current.forEach(clearTimeout);
      timersRef.current.clear();
    };
  }, [dismiss]);

  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-3 left-1/2 z-[9999] flex -translate-x-1/2 flex-col gap-1.5">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role="alert"
          onClick={() => dismiss(toast.id)}
          className={cn(
            "animate-in slide-in-from-bottom-2 fade-in cursor-pointer rounded-md px-3 py-2 text-xs shadow-md",
            "max-w-[320px] break-words",
            toast.type === "error" && "bg-destructive text-destructive-foreground",
            toast.type === "success" && "bg-primary text-primary-foreground",
            toast.type === "info" && "bg-primary text-primary-foreground",
          )}
        >
          {toast.message}
        </div>
      ))}
    </div>
  );
}
