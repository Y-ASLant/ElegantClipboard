import { useCallback, useEffect, useLayoutEffect, useState, useMemo, useRef } from "react";
import {
  Search16Regular,
  Delete16Regular,
  Settings16Regular,
  LockClosed16Regular,
  LockClosed16Filled,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import debounce from "lodash.debounce";
import { ClipboardList } from "@/components/ClipboardList";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { GROUPS } from "@/lib/constants";
import { logError } from "@/lib/logger";
import { initTheme } from "@/lib/theme-applier";
import { cn } from "@/lib/utils";
import { useClipboardStore } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";


// Initialize theme once for this window (runs before component mounts)
initTheme();

/** Dismiss any open Radix overlay (context menu, dialog, etc.) via synthetic ESC */
function dismissOverlays(): boolean {
  const overlay = document.querySelector(
    '[role="dialog"], [data-radix-popper-content-wrapper]'
  );
  if (overlay) {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    return true;
  }
  return false;
}

function App() {
  const [clearDialogOpen, setClearDialogOpen] = useState(false);
  const [isPinned, setIsPinned] = useState(false);
  const { searchQuery, selectedGroup, setSearchQuery, setSelectedGroup, fetchItems, clearHistory, refresh, resetView } = useClipboardStore();
  const autoResetState = useUISettings((s) => s.autoResetState);
  const searchAutoFocus = useUISettings((s) => s.searchAutoFocus);
  const searchAutoClear = useUISettings((s) => s.searchAutoClear);
  const cardDensity = useUISettings((s) => s.cardDensity);
  const inputRef = useRef<HTMLInputElement>(null);
  const segmentRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const segmentContainerRef = useRef<HTMLDivElement>(null);
  const [segmentIndicator, setSegmentIndicator] = useState({ left: 0, width: 0 });

  // 更新滑动指示器位置 - 使用 getBoundingClientRect 获取高DPI精确值
  useLayoutEffect(() => {
    const idx = GROUPS.findIndex((g) => g.value === selectedGroup);
    const el = segmentRefs.current[idx];
    const container = segmentContainerRef.current;
    if (el && container) {
      const elRect = el.getBoundingClientRect();
      const containerRect = container.getBoundingClientRect();
      // 计算相对于容器的精确位置
      const left = elRect.left - containerRect.left;
      const width = elRect.width;
      setSegmentIndicator({ left, width });
    }
  }, [selectedGroup]);

  // Apply card density to root element
  useEffect(() => {
    document.documentElement.dataset.density = cardDensity;
  }, [cardDensity]);

  // Load pinned state on mount
  useEffect(() => {
    invoke<boolean>("is_window_pinned").then(setIsPinned);
  }, []);

  // Suppress toolbar tooltips briefly when window appears (prevents tooltip flash
  // when cursor happens to be over a toolbar button, e.g. opening via tray click)
  const [suppressTooltips, setSuppressTooltips] = useState(false);
  const suppressTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Refresh data when window is shown (files_valid is computed by backend)
  useEffect(() => {
    const unlisten = listen("window-shown", () => {
      // Re-read persisted settings (may have been changed in the settings window)
      useUISettings.persist.rehydrate();
      if (searchAutoClear) {
        setSearchQuery("");
        fetchItems({ search: "" });
      } else {
        refresh();
      }
      if (searchAutoFocus) {
        inputRef.current?.focus();
      }
      setSuppressTooltips(true);
      if (suppressTimerRef.current) clearTimeout(suppressTimerRef.current);
      suppressTimerRef.current = setTimeout(() => setSuppressTooltips(false), 400);
    });
    return () => {
      unlisten.then((fn) => fn());
      if (suppressTimerRef.current) clearTimeout(suppressTimerRef.current);
    };
  }, [refresh, fetchItems, setSearchQuery, searchAutoFocus, searchAutoClear]);

  // Dismiss overlays & optionally reset view state when window is hidden
  useEffect(() => {
    const unlisten = listen("window-hidden", () => {
      dismissOverlays();
      if (autoResetState) {
        resetView();
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [resetView, autoResetState]);

  // Window starts hidden (visible: false in tauri.conf.json, backend defaults to Hidden).
  // It will be shown only via hotkey (toggle_window_visibility) or tray click.
  // No need to show on startup — clipboard managers should start minimized to tray.

  // Handle window focusable state based on input focus
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let blurTimeoutId: ReturnType<typeof setTimeout> | null = null;

    const handleFocus = async () => {
      // Cancel pending blur if user re-focuses quickly (e.g., clicked scrollbar then back)
      if (blurTimeoutId) {
        clearTimeout(blurTimeoutId);
        blurTimeoutId = null;
      }
      // Make window focusable when input is focused
      await appWindow.setFocusable(true);
      await appWindow.setFocus();
    };

    const handleBlur = async () => {
      // Delay setFocusable(false) to allow in-window interactions (scrollbar, cards, etc.)
      // If user clicks scrollbar, we don't want to immediately disable focusable
      blurTimeoutId = setTimeout(async () => {
        // Check if focus moved outside the window (not just to scrollbar/card)
        if (document.activeElement === document.body || !document.hasFocus()) {
          await appWindow.setFocusable(false);
        }
        blurTimeoutId = null;
      }, 100);
    };

    const input = inputRef.current;
    if (input) {
      input.addEventListener("focus", handleFocus);
      input.addEventListener("blur", handleBlur);
      return () => {
        input.removeEventListener("focus", handleFocus);
        input.removeEventListener("blur", handleBlur);
        if (blurTimeoutId) clearTimeout(blurTimeoutId);
      };
    }
  }, []);

  // Handle ESC key — two paths for reliability:
  // 1. Backend keyboard hook emits "escape-pressed" (works when window is non-focusable)
  // 2. DOM keydown listener (works when window is focusable, e.g. after searchAutoFocus)
  const handleEscape = useCallback(async () => {
    if (dismissOverlays()) return;
    try {
      await invoke("hide_window");
    } catch (error) {
      logError("Failed to hide window:", error);
    }
  }, []);

  // Path 1: backend global keyboard hook (non-focusable window)
  useEffect(() => {
    const unlisten = listen("escape-pressed", handleEscape);
    return () => { unlisten.then((fn) => fn()); };
  }, [handleEscape]);

  // Path 2: DOM keydown (focusable window, e.g. search input focused)
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

  // NOTE: Click-outside detection is handled by the backend input_monitor module
  // because the window is set to non-focusable (focus: false), which means
  // onFocusChanged events never fire. The backend uses rdev to monitor global
  // mouse clicks and hides the window when a click is detected outside its bounds.

  // Debounced search — delegates to store's fetchItems which has its own _fetchId guard
  const debouncedSearch = useMemo(
    () => debounce(() => {
      fetchItems();
    }, 300),
    [fetchItems]
  );

  // Cleanup debounce on unmount
  useEffect(() => {
    return () => {
      debouncedSearch.cancel();
    };
  }, [debouncedSearch]);

  const handleSearchChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value;
    setSearchQuery(value);
    debouncedSearch();
  };

  const handleClearHistory = () => {
    clearHistory();
    setClearDialogOpen(false);
  };

  const openSettings = async () => {
    try {
      await invoke("open_settings_window");
    } catch (error) {
      logError("Failed to open settings:", error);
    }
  };

  const togglePinned = async () => {
    const newState = !isPinned;
    try {
      await invoke("set_window_pinned", { pinned: newState });
      setIsPinned(newState);
    } catch (error) {
      logError("Failed to toggle pinned state:", error);
    }
  };

  return (
    <div className="h-screen flex flex-col bg-muted/40 overflow-hidden">
      {/* Header: Search + Actions */}
      <div
        className="flex items-center gap-2 p-2 shrink-0 select-none"
        data-tauri-drag-region
      >
        {/* Search Bar */}
        <div className="relative flex-1" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
          <Search16Regular className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground z-10" />
          <Input
            ref={inputRef}
            type="text"
            placeholder="搜索剪贴板..."
            value={searchQuery}
            onChange={handleSearchChange}
            className="pl-9 h-9 text-sm bg-background border shadow-sm"
          />
        </div>

        {/* Action Buttons Card */}
        <div 
          className="flex items-center gap-0.5 h-9 px-1 bg-background border rounded-md shadow-sm" 
          style={{ WebkitAppRegion: 'no-drag', pointerEvents: suppressTooltips ? 'none' : undefined } as React.CSSProperties}
        >
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => setClearDialogOpen(true)}
                className="w-7 h-7 flex items-center justify-center text-muted-foreground hover:bg-accent hover:text-accent-foreground rounded transition-colors"
              >
                <Delete16Regular className="w-4 h-4" />
              </button>
            </TooltipTrigger>
            <TooltipContent>清空历史</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={togglePinned}
                className={`w-7 h-7 flex items-center justify-center rounded transition-colors ${
                  isPinned
                    ? "text-primary bg-primary/10"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                }`}
              >
                {isPinned ? (
                  <LockClosed16Filled className="w-4 h-4" />
                ) : (
                  <LockClosed16Regular className="w-4 h-4" />
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent>{isPinned ? "解除锁定" : "锁定窗口"}</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={openSettings}
                className="w-7 h-7 flex items-center justify-center text-muted-foreground hover:bg-accent hover:text-accent-foreground rounded transition-colors"
              >
                <Settings16Regular className="w-4 h-4" />
              </button>
            </TooltipTrigger>
            <TooltipContent>设置</TooltipContent>
          </Tooltip>
        </div>
      </div>

      {/* Clipboard List */}
      <div className="flex-1 overflow-hidden">
        <ClipboardList />
      </div>

      {/* Bottom Segment */}
      <div className="shrink-0 px-2 pb-2 pt-1 select-none">
        <div ref={segmentContainerRef} className="relative flex items-center h-8 p-0.5 bg-muted rounded-lg">
          {/* 滑动指示器 */}
          <div
            className="absolute left-0 top-0.5 h-[calc(100%-4px)] rounded-md bg-background shadow-sm will-change-transform transition-[transform,width,opacity] duration-200 ease-out"
            style={{
              transform: `translateX(${segmentIndicator.left}px)`,
              width: segmentIndicator.width,
              opacity: segmentIndicator.width > 0 ? 1 : 0,
            }}
          />
          {GROUPS.map((g, i) => (
            <button
              key={g.label}
              ref={(el) => { segmentRefs.current[i] = el; }}
              onClick={() => setSelectedGroup(g.value)}
              className={cn(
                "relative z-[1] flex-1 h-full rounded-md text-xs font-medium transition-colors duration-200",
                selectedGroup === g.value
                  ? "text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {g.label}
            </button>
          ))}
        </div>
      </div>

      {/* Clear History Dialog */}
      <Dialog open={clearDialogOpen} onOpenChange={setClearDialogOpen}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left">
            <DialogTitle>清空历史记录</DialogTitle>
            <DialogDescription className="text-left">
              确定要清空所有非置顶的历史记录吗？此操作不可撤销。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setClearDialogOpen(false)}>
              取消
            </Button>
            <Button variant="destructive" onClick={handleClearHistory}>
              清空
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export default App;

