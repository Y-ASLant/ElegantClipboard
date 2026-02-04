import { useEffect, useState, useMemo } from "react";
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
import { useClipboardStore } from "@/stores/clipboard";
import {
  Search16Regular,
  Delete16Regular,
  Settings16Regular,
} from "@fluentui/react-icons";
import debounce from "lodash.debounce";

function App() {
  const [isDark, setIsDark] = useState(false);
  const [clearDialogOpen, setClearDialogOpen] = useState(false);
  const { searchQuery, setSearchQuery, clearHistory, fetchItems } =
    useClipboardStore();

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

  // Debounced search with proper cleanup
  const debouncedSearch = useMemo(
    () => debounce((query: string) => {
      fetchItems({ search: query || undefined });
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

  return (
    <div className="h-screen flex flex-col bg-background overflow-hidden">
      {/* Header: Search + Actions */}
      <div
        className="h-12 flex items-center gap-2 px-3 bg-background shrink-0 select-none"
        data-tauri-drag-region
      >
        {/* Search Bar */}
        <div className="relative flex-1" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
          <Search16Regular className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <Input
            type="text"
            placeholder="搜索剪贴板..."
            value={searchQuery}
            onChange={handleSearchChange}
            className="pl-9 h-8 text-sm"
          />
        </div>

        {/* Action Buttons */}
        <div className="flex items-center gap-0.5" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
          <button
            onClick={() => setClearDialogOpen(true)}
            className="w-8 h-8 flex items-center justify-center text-muted-foreground hover:bg-accent rounded-md transition-colors"
            title="清空历史"
          >
            <Delete16Regular className="w-4 h-4" />
          </button>
          <button
            onClick={openSettings}
            className="w-8 h-8 flex items-center justify-center text-muted-foreground hover:bg-accent rounded-md transition-colors"
            title="设置"
          >
            <Settings16Regular className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Clipboard List */}
      <div className="flex-1 overflow-hidden">
        <ClipboardList />
      </div>

      {/* Clear History Dialog */}
      <Dialog open={clearDialogOpen} onOpenChange={setClearDialogOpen}>
        <DialogContent showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>清空历史记录</DialogTitle>
            <DialogDescription>
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
