import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useState, useMemo, useRef } from "react";
import {
  Search16Regular,
  Dismiss16Regular,
  Delete16Regular,
  Settings16Regular,
  Pin16Filled,
  PinOff16Regular,
  MultiselectLtr16Regular,
  Star16Regular,
  Star16Filled,
  Tag16Regular,
  Tag16Filled,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { useShallow } from "zustand/react/shallow";
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
import { useInputFocus } from "@/hooks/useInputFocus";
import { useSearchDebounce } from "@/hooks/useSearchDebounce";
import { useWindowLifecycle } from "@/hooks/useWindowLifecycle";
import { LOGICAL_TYPE_BACKEND_MAP, getVisibleCategories } from "@/lib/constants";
import { logError } from "@/lib/logger";
import { initTheme } from "@/lib/theme-applier";
import { cn } from "@/lib/utils";
import { useClipboardStore } from "@/stores/clipboard";
import { useTranslateSettings } from "@/stores/translate-settings";
import type { ToolbarButton } from "@/stores/ui-settings";
import { useUISettings } from "@/stores/ui-settings";


// 初始化主题
initTheme();

// 加载翻译设置放到首帧之后，避免启动显示时和主列表初始化争抢资源
const scheduleTranslateSettingsLoad = () => {
  const load = () => {
    useTranslateSettings.getState().loadSettings();
  };
  if ("requestIdleCallback" in window) {
    window.requestIdleCallback(load, { timeout: 1500 });
    return;
  }
  globalThis.setTimeout(load, 800);
};
scheduleTranslateSettingsLoad();

const TagsView = lazy(() => import("@/components/TagsView").then((m) => ({ default: m.TagsView })));

const NO_DRAG_STYLE = { WebkitAppRegion: 'no-drag' } as React.CSSProperties;

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
  const [tagsViewOpen, setTagsViewOpen] = useState(false);

  const { searchQuery, selectedCategory, setSearchQuery, setSelectedCategory, fetchItems, clearHistory, refresh, resetView } = useClipboardStore(
    useShallow((s) => ({
      searchQuery: s.searchQuery,
      selectedCategory: s.selectedCategory,
      setSearchQuery: s.setSearchQuery,
      setSelectedCategory: s.setSelectedCategory,
      fetchItems: s.fetchItems,
      clearHistory: s.clearHistory,
      refresh: s.refresh,
      resetView: s.resetView,
    }))
  );
  const batchMode = useClipboardStore((s) => s.batchMode);
  const selectedIds = useClipboardStore((s) => s.selectedIds);
  const setBatchMode = useClipboardStore((s) => s.setBatchMode);
  const batchDelete = useClipboardStore((s) => s.batchDelete);
  const [batchDeleteDialogOpen, setBatchDeleteDialogOpen] = useState(false);
  const autoResetState = useUISettings((s) => s.autoResetState);
  const searchAutoFocus = useUISettings((s) => s.searchAutoFocus);
  const searchAutoClear = useUISettings((s) => s.searchAutoClear);
  const cardDensity = useUISettings((s) => s.cardDensity);
  const showCategoryFilter = useUISettings((s) => s.showCategoryFilter);
  const enabledMonitorTypes = useUISettings((s) => s.enabledMonitorTypes);
  const toolbarButtons = useUISettings((s) => s.toolbarButtons);
  const windowAnimation = useUISettings((s) => s.windowAnimation);
  const hideFavoritedFromMain = useUISettings((s) => s.hideFavoritedFromMain);
  const hideTaggedFromMain = useUISettings((s) => s.hideTaggedFromMain);
  const inputRef = useInputFocus<HTMLInputElement>();
  const segmentRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const segmentContainerRef = useRef<HTMLDivElement>(null);
  const [segmentIndicator, setSegmentIndicator] = useState({ left: 0, width: 0 });

  const visibleCategories = useMemo(
    () => getVisibleCategories(enabledMonitorTypes),
    [enabledMonitorTypes],
  );

  // 更新滑动指示器（始终跟踪类型筛选）
  const updateIndicator = useCallback(() => {
    const idx = visibleCategories.findIndex((g) => g.value === selectedCategory);
    const el = segmentRefs.current[idx];
    if (el) {
      setSegmentIndicator({ left: el.offsetLeft, width: el.offsetWidth });
    }
  }, [selectedCategory, visibleCategories]);

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

  // 分类栏隐藏时重置筛选
  useEffect(() => {
    if (!showCategoryFilter) {
      useClipboardStore.setState({ selectedCategory: null });
    }
  }, [showCategoryFilter]);

  // 当前选中的分类被隐藏时（监听类型变更），重置为全部
  useEffect(() => {
    const sc = useClipboardStore.getState().selectedCategory;
    if (sc && sc !== "__favorites__" && !visibleCategories.some((c) => c.value === sc)) {
      setSelectedCategory(null);
    }
  }, [visibleCategories, setSelectedCategory]);

  const { isPinned, setIsPinned, suppressTooltips, windowVisible } = useWindowLifecycle({
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
  });
  const toolbarStyle = useMemo<React.CSSProperties>(() => ({
    WebkitAppRegion: 'no-drag',
    pointerEvents: suppressTooltips ? 'none' : undefined,
  } as React.CSSProperties), [suppressTooltips]);

  // 隐藏设置变更时动态刷新主页列表
  const hideSettingsMountRef = useRef(true);
  useEffect(() => {
    if (hideSettingsMountRef.current) { hideSettingsMountRef.current = false; return; }
    refresh();
  }, [hideFavoritedFromMain, hideTaggedFromMain, refresh]);

  const { handleSearchChange, clearSearch } = useSearchDebounce({
    fetchItems,
    setSearchQuery,
  });

  const clearScopeText = useMemo(() => {
    if (selectedCategory === "__favorites__") {
      return "收藏视图下不支持清空操作。收藏项受保护，请在设置中使用“删除所有数据”进行全量删除。";
    }
    if (selectedCategory) {
      return "确定要清空当前分类内所有非置顶、非收藏、无标签的历史记录吗？此操作不可撤销。";
    }
    return "确定要清空所有非置顶、非收藏、无标签的历史记录吗？此操作不可撤销。";
  }, [selectedCategory]);

  const handleClearHistory = async () => {
    if (selectedCategory === "__favorites__") {
      setClearDialogOpen(false);
      return;
    }
    // 将前端逻辑类型映射为后端 contentType
    const mapping = selectedCategory ? LOGICAL_TYPE_BACKEND_MAP[selectedCategory] : undefined;
    const backendType = mapping ? mapping.backendType : selectedCategory;
    await clearHistory(backendType);
    setClearDialogOpen(false);
  };

  const openSettings = useCallback(async () => {
    try {
      await invoke("open_settings_window");
    } catch (error) {
      logError("Failed to open settings:", error);
    }
  }, []);

  const togglePinned = useCallback(async () => {
    const newState = !isPinned;
    try {
      await invoke("set_window_pinned", { pinned: newState });
      setIsPinned(newState);
    } catch (error) {
      logError("Failed to toggle pinned state:", error);
    }
  }, [isPinned]);

  const renderToolbarButton = useCallback((id: ToolbarButton) => {
    switch (id) {
      case "clear":
        return (
          <Tooltip key={id}>
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
        );
      case "pin":
        return (
          <Tooltip key={id}>
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
                  <Pin16Filled className="w-4 h-4" />
                ) : (
                  <PinOff16Regular className="w-4 h-4" />
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent>{isPinned ? "取消置顶" : "置顶窗口"}</TooltipContent>
          </Tooltip>
        );
      case "batch":
        return (
          <Tooltip key={id}>
            <TooltipTrigger asChild>
              <button
                onClick={() => setBatchMode(!batchMode)}
                className={`w-7 h-7 flex items-center justify-center rounded transition-colors ${
                  batchMode
                    ? "text-primary bg-primary/10"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                }`}
              >
                <MultiselectLtr16Regular className="w-4 h-4" />
              </button>
            </TooltipTrigger>
            <TooltipContent>{batchMode ? "退出批量选择" : "批量选择"}</TooltipContent>
          </Tooltip>
        );
      case "settings":
        return (
          <Tooltip key={id}>
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
        );
      default:
        return null;
    }
  }, [isPinned, openSettings, togglePinned, batchMode, setBatchMode]);

  return (
    <div className={cn(
      "h-screen flex flex-col bg-muted/40 overflow-hidden",
      windowAnimation && windowVisible === true && "window-enter",
      windowAnimation && windowVisible === false && "window-hidden",
    )}>
      {/* 顶栏：搜索 + 操作 */}
      <div
        className="flex items-center gap-1 px-2 pt-2 pb-0.5 shrink-0 select-none"
        data-tauri-drag-region
      >
        {/* 常驻按钮：收藏 & 标签 */}
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={() => {
                setTagsViewOpen(false);
                setSelectedCategory(selectedCategory === "__favorites__" ? null : "__favorites__");
              }}
              className={cn(
                "w-7 h-7 flex items-center justify-center rounded-md transition-colors shrink-0",
                selectedCategory === "__favorites__"
                  ? "text-primary"
                  : "text-muted-foreground hover:text-foreground"
              )}
              style={NO_DRAG_STYLE}
            >
              {selectedCategory === "__favorites__" ? (
                <Star16Filled className="w-4 h-4" />
              ) : (
                <Star16Regular className="w-4 h-4" />
              )}
            </button>
          </TooltipTrigger>
          <TooltipContent>{selectedCategory === "__favorites__" ? "退出收藏" : "我的收藏"}</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={() => {
                if (selectedCategory === "__favorites__") setSelectedCategory(null);
                setTagsViewOpen((o) => !o);
              }}
              className={cn(
                "w-7 h-7 flex items-center justify-center rounded-md transition-colors shrink-0",
                tagsViewOpen
                  ? "text-primary"
                  : "text-muted-foreground hover:text-foreground"
              )}
              style={NO_DRAG_STYLE}
            >
              {tagsViewOpen ? (
                <Tag16Filled className="w-4 h-4" />
              ) : (
                <Tag16Regular className="w-4 h-4" />
              )}
            </button>
          </TooltipTrigger>
          <TooltipContent>{tagsViewOpen ? "退出标签" : "标签管理"}</TooltipContent>
        </Tooltip>

        {/* 搜索栏 */}
        <div className="relative flex-1" style={NO_DRAG_STYLE}>
          <Search16Regular className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground z-10" />
          <Input
            ref={inputRef}
            type="text"
            placeholder="搜索剪贴板..."
            value={searchQuery}
            onChange={handleSearchChange}
            className={cn("pl-9 h-9 text-sm bg-background border shadow-sm", searchQuery && "pr-8")}
          />
          {searchQuery && (
            <button
              onClick={clearSearch}
              className="absolute right-2 top-1/2 -translate-y-1/2 w-5 h-5 flex items-center justify-center text-muted-foreground hover:text-foreground rounded-sm transition-colors z-10"
            >
              <Dismiss16Regular className="w-3.5 h-3.5" />
            </button>
          )}
        </div>

        {/* 操作按钮 */}
        {toolbarButtons.length > 0 && (
          <div 
            className="flex items-center gap-0.5 h-9 px-1 bg-background border rounded-md shadow-sm" 
            style={toolbarStyle}
          >
            {toolbarButtons.map((btn) => renderToolbarButton(btn))}
          </div>
        )}

        {/* 关闭窗口 */}
        <button
          onClick={() => invoke("hide_window").catch((e) => logError("Failed to hide window:", e))}
          className="w-7 h-7 flex items-center justify-center text-muted-foreground hover:bg-destructive/10 hover:text-destructive rounded-md transition-colors shrink-0"
          style={NO_DRAG_STYLE}
        >
          <Dismiss16Regular className="w-4 h-4" />
        </button>
      </div>

      {/* 批量操作栏 */}
      {batchMode && (
        <div className="shrink-0 flex items-center justify-between px-3 py-1.5 bg-primary/5 border-b border-primary/20">
          <span className="text-xs text-muted-foreground">
            已选择 <span className="font-medium text-foreground">{selectedIds.size}</span> 项
            <span className="ml-1.5 text-muted-foreground/60">Shift 连选</span>
          </span>
          <div className="flex items-center gap-1">
            <button
              onClick={async () => {
                try {
                  await invoke("merge_paste_content", { ids: Array.from(selectedIds) });
                  setBatchMode(false);
                } catch (error) {
                  logError("Merge paste failed:", error);
                }
              }}
              disabled={selectedIds.size < 2}
              className="text-xs px-2 py-1 rounded bg-primary/10 text-primary hover:bg-primary/20 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              合并粘贴
            </button>
            <button
              onClick={() => setBatchDeleteDialogOpen(true)}
              disabled={selectedIds.size === 0}
              className="text-xs px-2 py-1 rounded bg-destructive/10 text-destructive hover:bg-destructive/20 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              删除
            </button>
            <button
              onClick={() => setBatchMode(false)}
              className="text-xs px-2 py-1 rounded hover:bg-accent transition-colors text-muted-foreground hover:text-foreground"
            >
              取消
            </button>
          </div>
        </div>
      )}

      {/* 主内容区域 */}
      <div className="flex-1 overflow-hidden">
        {tagsViewOpen ? (
          <Suspense fallback={null}>
            <TagsView />
          </Suspense>
        ) : (
          <ClipboardList searchInputRef={inputRef} />
        )}
      </div>

      {/* 底部分类选择 */}
      {showCategoryFilter && !tagsViewOpen && (
        <div className="shrink-0 px-2 pb-2 pt-1 select-none">
          <div
            ref={segmentContainerRef}
            className="relative flex items-center h-8 p-0.5 bg-muted rounded-lg"
          >
            {/* 滑动指示器 */}
            <div
              className="absolute left-0 top-0.5 h-[calc(100%-4px)] rounded-md bg-background shadow-sm will-change-transform transition-[transform,width,opacity] duration-200 ease-out"
              style={{
                transform: `translateX(${segmentIndicator.left}px)`,
                width: segmentIndicator.width,
                opacity: segmentIndicator.width > 0 ? 1 : 0,
              }}
            />

            {/* 类型 tabs */}
            {visibleCategories.map((g, i) => (
              <button
                key={g.label}
                ref={(el) => { segmentRefs.current[i] = el; }}
                onClick={() => setSelectedCategory(g.value)}
                className={cn(
                  "relative z-1 flex-1 h-full rounded-md text-xs font-medium transition-colors duration-200",
                  selectedCategory === g.value
                    ? "text-foreground"
                    : "text-muted-foreground hover:text-foreground",
                )}
              >
                {g.label}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* 批量删除确认对话框 */}
      <Dialog open={batchDeleteDialogOpen} onOpenChange={setBatchDeleteDialogOpen}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left">
            <DialogTitle>批量删除</DialogTitle>
            <DialogDescription className="text-left">
              确定要删除选中的 {selectedIds.size} 条记录吗？此操作不可撤销。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setBatchDeleteDialogOpen(false)}>
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={async () => {
                setBatchDeleteDialogOpen(false);
                await batchDelete();
              }}
            >
              删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 清空历史确认对话框 */}
      <Dialog open={clearDialogOpen} onOpenChange={setClearDialogOpen}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left">
            <DialogTitle>清空历史记录</DialogTitle>
            <DialogDescription className="text-left">
              {clearScopeText}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setClearDialogOpen(false)}>
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={handleClearHistory}
              disabled={selectedCategory === "__favorites__"}
            >
              清空
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

    </div>
  );
}

export default App;

