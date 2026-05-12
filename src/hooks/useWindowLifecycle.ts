import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { focusWindowImmediately } from "@/hooks/useInputFocus";
import { logError } from "@/lib/logger";
import { useClipboardStore } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";

type FetchItems = (options?: { search?: string }) => Promise<void>;

interface UseWindowLifecycleOptions {
  autoResetState: boolean;
  searchAutoClear: boolean;
  searchAutoFocus: boolean;
  cardDensity: string;
  tagsViewOpen: boolean;
  selectedCategory: string | null;
  inputRef: RefObject<HTMLInputElement | null>;
  fetchItems: FetchItems;
  refresh: () => Promise<void>;
  resetView: () => Promise<void>;
  setBatchMode: (enabled: boolean) => void;
  setSearchQuery: (query: string) => void;
  setTagsViewOpen: (open: boolean) => void;
  dismissOverlays: () => boolean;
}

export function useWindowLifecycle({
  autoResetState,
  searchAutoClear,
  searchAutoFocus,
  cardDensity,
  tagsViewOpen,
  selectedCategory,
  inputRef,
  fetchItems,
  refresh,
  resetView,
  setBatchMode,
  setSearchQuery,
  setTagsViewOpen,
  dismissOverlays,
}: UseWindowLifecycleOptions) {
  const clipboardDirtyRef = useRef(false);
  const viewSwitchRef = useRef(false);
  const suppressTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [isPinned, setIsPinned] = useState(false);
  const [suppressTooltips, setSuppressTooltips] = useState(false);
  const [windowVisible, setWindowVisible] = useState<boolean | null>(null);

  useEffect(() => {
    if (!viewSwitchRef.current) { viewSwitchRef.current = true; return; }
    invoke("refresh_files_validity").then((changed) => {
      if (changed as number > 0) refresh();
    });
  }, [tagsViewOpen, selectedCategory, refresh]);

  useEffect(() => {
    document.documentElement.dataset.density = cardDensity;
  }, [cardDensity]);

  useEffect(() => {
    invoke<boolean>("is_window_pinned").then(setIsPinned);
    const kbNav = useUISettings.getState().keyboardNavigation;
    invoke("set_keyboard_nav_enabled", { enabled: kbNav }).catch((error) => {
      logError("Failed to sync keyboard navigation setting:", error);
    });
  }, []);

  useEffect(() => {
    const unlisten = listen("clipboard-updated", () => {
      clipboardDirtyRef.current = true;
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    const unlisten = listen("window-shown", () => {
      setWindowVisible(true);
      useUISettings.persist.rehydrate();
      invoke("refresh_files_validity").finally(() => {
        if (searchAutoClear) {
          setSearchQuery("");
          fetchItems({ search: "" });
        } else {
          refresh();
        }
      });
      clipboardDirtyRef.current = false;
      if (searchAutoFocus) {
        focusWindowImmediately().then(() => {
          inputRef.current?.focus();
        });
      }
      setSuppressTooltips(true);
      if (suppressTimerRef.current) clearTimeout(suppressTimerRef.current);
      suppressTimerRef.current = setTimeout(() => setSuppressTooltips(false), 400);
    });
    return () => {
      unlisten.then((fn) => fn());
      if (suppressTimerRef.current) clearTimeout(suppressTimerRef.current);
    };
  }, [fetchItems, inputRef, refresh, searchAutoClear, searchAutoFocus, setSearchQuery]);

  useEffect(() => {
    const unlisten = listen("window-hidden", () => {
      setWindowVisible(false);
      dismissOverlays();
      setBatchMode(false);
      if (autoResetState) {
        setTagsViewOpen(false);
        resetView();
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [autoResetState, dismissOverlays, resetView, setBatchMode, setTagsViewOpen]);

  const handleEscape = useCallback(async () => {
    if (dismissOverlays()) return;
    if (useClipboardStore.getState().batchMode) {
      setBatchMode(false);
      return;
    }
    try {
      await invoke("hide_window");
    } catch (error) {
      logError("Failed to hide window:", error);
    }
  }, [dismissOverlays, setBatchMode]);

  useEffect(() => {
    const unlisten = listen("escape-pressed", handleEscape);
    return () => { unlisten.then((fn) => fn()); };
  }, [handleEscape]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && e.isTrusted) {
        e.preventDefault();
        handleEscape();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [handleEscape]);

  return { isPinned, setIsPinned, suppressTooltips, windowVisible };
}