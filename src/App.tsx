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


// 初始化主题
initTheme();

/** 关闭已打开的弹出层 */
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
  // 追踪窗口隐藏期间是否有剪贴板变化
  const clipboardDirtyRef = useRef(false);
  const segmentRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const segmentContainerRef = useRef<HTMLDivElement>(null);
  const [segmentIndicator, setSegmentIndicator] = useState({ left: 0, width: 0 });

  // 更新滑动指示器位置
  const updateIndicator = useCallback(() => {
    const idx = GROUPS.findIndex((g) => g.value === selectedGroup);
    const el = segmentRefs.current[idx];
    const container = segmentContainerRef.current;
    if (el && container) {
      const elRect = el.getBoundingClientRect();
      const containerRect = container.getBoundingClientRect();
      const left = elRect.left - containerRect.left;
      const width = elRect.width;
      setSegmentIndicator({ left, width });
    }
  }, [selectedGroup]);

  // 选中项变化时立即更新
  useLayoutEffect(updateIndicator, [updateIndicator]);

  // 窗口大小变化时重新计算指示器位置
  useEffect(() => {
    const container = segmentContainerRef.current;
    if (!container) return;
    const ro = new ResizeObserver(updateIndicator);
    ro.observe(container);
    return () => ro.disconnect();
  }, [updateIndicator]);

  // 应用卡片密度到根元素
  useEffect(() => {
    document.documentElement.dataset.density = cardDensity;
  }, [cardDensity]);

  // 加载锁定状态
  useEffect(() => {
    invoke<boolean>("is_window_pinned").then(setIsPinned);
  }, []);

  // 窗口出现时短暂抑制工具栏提示，防止闪烁
  const [suppressTooltips, setSuppressTooltips] = useState(false);
  const suppressTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 监听剪贴板变化，标记脏数据
  useEffect(() => {
    const unlisten = listen("clipboard-updated", () => {
      clipboardDirtyRef.current = true;
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // 窗口显示时按需刷新数据
  useEffect(() => {
    const unlisten = listen("window-shown", () => {
      // 重新读取设置（可能在设置窗口中更改）
      useUISettings.persist.rehydrate();
      if (searchAutoClear) {
        setSearchQuery("");
        fetchItems({ search: "" });
      } else if (clipboardDirtyRef.current) {
        // 有变化时重新获取以更新 files_valid
        refresh();
      }
      clipboardDirtyRef.current = false;
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

  // 窗口隐藏时关闭弹出层并可选重置状态
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

  // 根据输入框焦点切换窗口可聚焦状态
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let blurTimeoutId: ReturnType<typeof setTimeout> | null = null;

    const handleFocus = async () => {
      // 取消待处理的失焦，防止快速切换闪烁
      if (blurTimeoutId) {
        clearTimeout(blurTimeoutId);
        blurTimeoutId = null;
      }
      await appWindow.setFocusable(true);
      await appWindow.setFocus();
    };

    const handleBlur = async () => {
      // 延迟处理，允许窗口内交互（滚动条、卡片等）
      blurTimeoutId = setTimeout(async () => {
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

  // ESC 键处理：后端钩子 + DOM 监听双通道
  const handleEscape = useCallback(async () => {
    if (dismissOverlays()) return;
    try {
      await invoke("hide_window");
    } catch (error) {
      logError("Failed to hide window:", error);
    }
  }, []);

  // 通道1：后端键盘钩子
  useEffect(() => {
    const unlisten = listen("escape-pressed", handleEscape);
    return () => { unlisten.then((fn) => fn()); };
  }, [handleEscape]);

  // 通道2：DOM 键盘事件
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

  // 防抖搜索
  const debouncedSearch = useMemo(
    () => debounce(() => {
      fetchItems();
    }, 300),
    [fetchItems]
  );

  // 卸载时取消防抖
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
      {/* 顶栏：搜索 + 操作 */}
      <div
        className="flex items-center gap-2 p-2 shrink-0 select-none"
        data-tauri-drag-region
      >
        {/* 搜索栏 */}
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

        {/* 操作按钮 */}
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

      {/* 剪贴板列表 */}
      <div className="flex-1 overflow-hidden">
        <ClipboardList />
      </div>

      {/* 底部分组选择 */}
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

      {/* 清空历史确认对话框 */}
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

