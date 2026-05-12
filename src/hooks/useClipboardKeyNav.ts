import { useCallback, useEffect, useRef, type RefObject } from "react";
import { listen } from "@tauri-apps/api/event";
import { focusWindowImmediately } from "@/hooks/useInputFocus";
import { getVisibleCategories } from "@/lib/constants";
import { logError } from "@/lib/logger";
import { useClipboardStore } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";
import type { VirtuosoHandle } from "react-virtuoso";

const NAV_KEYS_DEFAULT = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Enter", "Delete"]);
const NAV_KEYS_SEARCH = new Set(["ArrowUp", "ArrowDown"]);

type NavSource = "default" | "search-input";

interface UseClipboardKeyNavOptions {
  searchInputRef: RefObject<HTMLInputElement | null>;
  virtuosoRef: RefObject<VirtuosoHandle | null>;
}

export function useClipboardKeyNav({ searchInputRef, virtuosoRef }: UseClipboardKeyNavOptions) {
  const focusSearchInFlightRef = useRef<Promise<void> | null>(null);
  const setActiveIndex = useClipboardStore((s) => s.setActiveIndex);
  const pasteContent = useClipboardStore((s) => s.pasteContent);
  const pasteAsPlainText = useClipboardStore((s) => s.pasteAsPlainText);
  const deleteItem = useClipboardStore((s) => s.deleteItem);

  const focusSearchInput = useCallback(() => {
    const target = searchInputRef.current;
    if (!target) return;
    if (document.activeElement === target) return;
    if (focusSearchInFlightRef.current) return;

    const applyFocus = () => {
      const input = searchInputRef.current;
      if (!input) return;
      input.focus();
    };

    const task = (async () => {
      // 后端键盘钩子触发时窗口可能未聚焦，先抢回窗口焦点再聚焦输入框。
      if (!document.hasFocus()) {
        await focusWindowImmediately();
      }
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => {
          applyFocus();
          resolve();
        });
      });
    })()
      .catch((error) => {
        logError("Failed to focus search input:", error);
      })
      .finally(() => {
        focusSearchInFlightRef.current = null;
      });

    focusSearchInFlightRef.current = task;
  }, [searchInputRef]);

  const handleNavKey = useCallback(
    (key: string, shift: boolean, source: NavSource = "default") => {
      if (!useUISettings.getState().keyboardNavigation) return;
      if (useClipboardStore.getState().batchMode) return;

      switch (key) {
        case "ArrowLeft": {
          if (!useUISettings.getState().showCategoryFilter) break;
          if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
          const { selectedCategory: sc, setSelectedCategory: ssc } = useClipboardStore.getState();
          const cats = getVisibleCategories(useUISettings.getState().enabledMonitorTypes);
          const curIdx = cats.findIndex((g) => g.value === sc);
          if (curIdx > 0) ssc(cats[curIdx - 1].value);
          break;
        }
        case "ArrowRight": {
          if (!useUISettings.getState().showCategoryFilter) break;
          if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
          const { selectedCategory: sc, setSelectedCategory: ssc } = useClipboardStore.getState();
          const cats = getVisibleCategories(useUISettings.getState().enabledMonitorTypes);
          const curIdx = cats.findIndex((g) => g.value === sc);
          if (curIdx < cats.length - 1) ssc(cats[curIdx + 1].value);
          break;
        }
        case "ArrowUp": {
          const { items: upItems, activeIndex: cur } = useClipboardStore.getState();
          if (upItems.length === 0) return;
          if (cur === 0) {
            setActiveIndex(-1);
            focusSearchInput();
            break;
          }
          if (cur === -1 && source === "search-input") {
            break;
          }
          if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
          let next = cur;
          if (cur > 0) next = cur - 1;
          else if (cur === -1) next = 0;
          if (next !== cur) {
            setActiveIndex(next);
            virtuosoRef.current?.scrollToIndex({ index: next, align: "center", behavior: "auto" });
          }
          break;
        }
        case "ArrowDown": {
          const { items: downItems, activeIndex: cur } = useClipboardStore.getState();
          if (downItems.length === 0) return;
          if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
          if (cur < downItems.length - 1) {
            const next = cur + 1;
            setActiveIndex(next);
            virtuosoRef.current?.scrollToIndex({ index: next, align: "center", behavior: "auto" });
          }
          break;
        }
        case "Enter": {
          const { activeIndex: idx, items: list } = useClipboardStore.getState();
          if (idx < 0 || idx >= list.length) return;
          const item = list[idx];
          if (shift) {
            pasteAsPlainText(item.id);
          } else {
            pasteContent(item.id);
          }
          break;
        }
        case "Delete": {
          const { activeIndex: idx, items: list } = useClipboardStore.getState();
          if (idx < 0 || idx >= list.length) return;
          deleteItem(list[idx].id);
          if (idx >= list.length - 1) {
            setActiveIndex(Math.max(0, list.length - 2));
          }
          break;
        }
      }
    },
    [deleteItem, focusSearchInput, pasteAsPlainText, pasteContent, setActiveIndex, virtuosoRef],
  );

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.isComposing) return;

      const target = e.target;
      const el = target instanceof HTMLElement ? target : null;
      const isEditable =
        el instanceof HTMLInputElement ||
        el instanceof HTMLTextAreaElement ||
        el?.isContentEditable;
      const isSearchInput =
        el instanceof HTMLInputElement &&
        el === searchInputRef.current;

      if (isEditable && !isSearchInput) return;

      const navKeys = isSearchInput
        ? NAV_KEYS_SEARCH
        : NAV_KEYS_DEFAULT;

      if (navKeys.has(e.key)) {
        e.preventDefault();
        handleNavKey(e.key, e.shiftKey, isSearchInput ? "search-input" : "default");
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleNavKey, searchInputRef]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<{ key: string; shift: boolean }>("keyboard-nav", (event) => {
      if (document.hasFocus()) return;
      handleNavKey(event.payload.key, event.payload.shift);
    }).then((fn) => {
      if (disposed) fn(); else unlisten = fn;
    });
    return () => { disposed = true; unlisten?.(); };
  }, [handleNavKey]);
}