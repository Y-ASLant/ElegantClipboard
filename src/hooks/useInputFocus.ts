import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

const FOCUS_DEBOUNCE_DELAY = 50;

let currentFocusState: "normal" | "focused" = "normal";
let focusDebounceTimer: ReturnType<typeof setTimeout> | null = null;
let blurDebounceTimer: ReturnType<typeof setTimeout> | null = null;

// 窗口失焦时重置状态
if (typeof window !== "undefined") {
  window.addEventListener("blur", () => {
    currentFocusState = "normal";
  });
}

async function debouncedEnableFocus() {
  if (blurDebounceTimer) {
    clearTimeout(blurDebounceTimer);
    blurDebounceTimer = null;
  }

  if (currentFocusState === "focused") {
    return;
  }

  if (focusDebounceTimer) {
    clearTimeout(focusDebounceTimer);
  }

  focusDebounceTimer = setTimeout(async () => {
    try {
      await invoke("focus_clipboard_window");
      currentFocusState = "focused";
    } catch (error) {
      console.error("启用窗口焦点失败:", error);
    }
    focusDebounceTimer = null;
  }, FOCUS_DEBOUNCE_DELAY);
}

async function debouncedRestoreFocus() {
  if (focusDebounceTimer) {
    clearTimeout(focusDebounceTimer);
    focusDebounceTimer = null;
  }

  if (currentFocusState === "normal") {
    return;
  }

  if (blurDebounceTimer) {
    clearTimeout(blurDebounceTimer);
  }

  blurDebounceTimer = setTimeout(async () => {
    const activeElement = document.activeElement;
    const isInputFocused =
      activeElement &&
      (activeElement.tagName === "INPUT" ||
        activeElement.tagName === "TEXTAREA" ||
        (activeElement as HTMLElement).contentEditable === "true");

    // 如果有其他输入框获得焦点，不恢复
    if (isInputFocused) {
      return;
    }

    try {
      await invoke("restore_last_focus");
      currentFocusState = "normal";
    } catch (error) {
      console.error("恢复非聚焦模式失败:", error);
    }
    blurDebounceTimer = null;
  }, FOCUS_DEBOUNCE_DELAY);
}

/**
 * 动态焦点切换 Hook。
 * 输入框获得焦点时临时启用窗口焦点，失去焦点时恢复非聚焦模式。
 * 参考 QuickClipboard 的 useInputFocus 实现。
 */
export function useInputFocus<T extends HTMLElement>() {
  const inputRef = useRef<T>(null);

  useEffect(() => {
    const element = inputRef.current;
    if (!element) return;

    const handleFocus = () => {
      debouncedEnableFocus();
    };

    const handleBlur = () => {
      debouncedRestoreFocus();
    };

    element.addEventListener("focus", handleFocus);
    element.addEventListener("blur", handleBlur);

    const checkInitialFocus = setTimeout(() => {
      if (document.activeElement === element) {
        debouncedEnableFocus();
      }
    }, 0);

    return () => {
      element.removeEventListener("focus", handleFocus);
      element.removeEventListener("blur", handleBlur);
      clearTimeout(checkInitialFocus);
    };
  }, []);

  return inputRef;
}

/** 立即启用窗口焦点（跳过防抖） */
export async function focusWindowImmediately() {
  if (blurDebounceTimer) {
    clearTimeout(blurDebounceTimer);
    blurDebounceTimer = null;
  }
  if (focusDebounceTimer) {
    clearTimeout(focusDebounceTimer);
    focusDebounceTimer = null;
  }

  try {
    await invoke("focus_clipboard_window");
    currentFocusState = "focused";
  } catch (error) {
    console.error("立即启用窗口焦点失败:", error);
  }
}
