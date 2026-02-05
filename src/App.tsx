import { useEffect, useState, useMemo, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ClipboardList } from "@/components/ClipboardList";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useClipboardStore, ClipboardItem } from "@/stores/clipboard";
import {
  Search16Regular,
  Delete16Regular,
  Settings16Regular,
  LockClosed16Regular,
  LockClosed16Filled,
} from "@fluentui/react-icons";
import debounce from "lodash.debounce";

function App() {
  const [isDark, setIsDark] = useState(false);
  const [clearDialogOpen, setClearDialogOpen] = useState(false);
  const [isPinned, setIsPinned] = useState(false);
  const { searchQuery, setSearchQuery, clearHistory } =
    useClipboardStore();
  const inputRef = useRef<HTMLInputElement>(null);

  // Load pinned state on mount
  useEffect(() => {
    invoke<boolean>("is_window_pinned").then(setIsPinned);
  }, []);

  // Show window after content is loaded (prevent white flash)
  useEffect(() => {
    const appWindow = getCurrentWindow();
    // Small delay to ensure content is rendered
    requestAnimationFrame(async () => {
      await appWindow.show();
      // Sync state to backend for Win+V toggle
      await invoke("set_window_visibility", { visible: true });
    });
  }, []);

  // Handle window focusable state based on input focus
  useEffect(() => {
    const appWindow = getCurrentWindow();
    const handleFocus = async () => {
      // Make window focusable when input is focused
      await appWindow.setFocusable(true);
      await appWindow.setFocus();
    };
    const handleBlur = async () => {
      // Make window non-focusable when input loses focus
      await appWindow.setFocusable(false);
    };

    const input = inputRef.current;
    if (input) {
      input.addEventListener("focus", handleFocus);
      input.addEventListener("blur", handleBlur);
      return () => {
        input.removeEventListener("focus", handleFocus);
        input.removeEventListener("blur", handleBlur);
      };
    }
  }, []);

  // Detect system dark mode
  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    setIsDark(mediaQuery.matches);

    const handler = (e: MediaQueryListEvent) => setIsDark(e.matches);
    mediaQuery.addEventListener("change", handler);
    return () => mediaQuery.removeEventListener("change", handler);
  }, []);

  // Apply dark class to html element
  useEffect(() => {
    document.documentElement.classList.toggle("dark", isDark);
  }, [isDark]);

  // NOTE: Click-outside detection is handled by the backend input_monitor module
  // because the window is set to non-focusable (focus: false), which means
  // onFocusChanged events never fire. The backend uses rdev to monitor global
  // mouse clicks and hides the window when a click is detected outside its bounds.

  // Request counter to handle out-of-order responses
  const searchRequestIdRef = useRef(0);

  // Search function with request ordering
  const executeSearch = useCallback(async (query: string) => {
    const requestId = ++searchRequestIdRef.current;
    
    try {
      const items = await invoke<ClipboardItem[]>("get_clipboard_items", {
        search: query || null,
        contentType: null,
        pinnedOnly: false,
        favoriteOnly: false,
        limit: 100,
        offset: 0,
      });
      
      // Only update if this is still the latest request
      if (requestId === searchRequestIdRef.current) {
        useClipboardStore.setState({ items, isLoading: false });
      }
    } catch (error) {
      if (requestId === searchRequestIdRef.current) {
        console.error("Search failed:", error);
        useClipboardStore.setState({ isLoading: false });
      }
    }
  }, []);

  // Debounced search with proper cleanup
  const debouncedSearch = useMemo(
    () => debounce((query: string) => {
      executeSearch(query);
    }, 300),
    [executeSearch]
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
    debouncedSearch(value);
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
