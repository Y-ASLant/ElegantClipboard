import { useEffect, useState, useMemo, useRef } from "react";
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
import { initTheme } from "@/lib/theme-applier";
import { useClipboardStore } from "@/stores/clipboard";

// Initialize theme once for this window (runs before component mounts)
initTheme();

function App() {
  const [clearDialogOpen, setClearDialogOpen] = useState(false);
  const [isPinned, setIsPinned] = useState(false);
  const { searchQuery, setSearchQuery, fetchItems, clearHistory, refresh } = useClipboardStore();
  const inputRef = useRef<HTMLInputElement>(null);

  // Load pinned state on mount
  useEffect(() => {
    invoke<boolean>("is_window_pinned").then(setIsPinned);
  }, []);

  // Refresh data when window is shown (files_valid is computed by backend)
  useEffect(() => {
    const unlisten = listen("window-shown", () => {
      refresh();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh]);

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

  // Handle ESC key (emitted by backend global keyboard hook, works without focus)
  useEffect(() => {
    const unlisten = listen("escape-pressed", async () => {
      // Check if any overlay (dialog, context menu, etc.) is open via DOM
      const hasOverlay = document.querySelector(
        '[role="dialog"], [data-radix-popper-content-wrapper]'
      );
      if (hasOverlay) {
        // Dispatch synthetic ESC to let Radix close the overlay
        document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
        return;
      }
      try {
        await invoke("hide_window");
      } catch (error) {
        console.error("Failed to hide window:", error);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

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
      console.error("Failed to open settings:", error);
    }
  };

  const togglePinned = async () => {
    const newState = !isPinned;
    try {
      await invoke("set_window_pinned", { pinned: newState });
      setIsPinned(newState);
    } catch (error) {
      console.error("Failed to toggle pinned state:", error);
    }
  };

  return (
    <div className="h-screen flex flex-col bg-muted/40 overflow-hidden">
      {/* Header: Search + Actions */}
      <div
        className="flex items-center gap-2 p-2 shrink-0 select-none"
        data-tauri-drag-region
      >
        {/* Search Bar Card */}
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
          style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
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
