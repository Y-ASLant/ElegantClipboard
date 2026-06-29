import { useCallback, useEffect, useLayoutEffect, useState, useMemo, useRef } from "react";
import {
  Search16Regular,
  Dismiss16Regular,
  Delete16Regular,
  Edit16Regular,
  Settings16Regular,
  LockClosed16Regular,
  LockClosed16Filled,
  Add16Regular,
  ChevronDown16Regular,
  MultiselectLtr16Regular,
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import debounce from "lodash.debounce";
import { useShallow } from "zustand/react/shallow";
import { ClipboardList } from "@/components/ClipboardList";
import { Onboarding } from "@/components/Onboarding";
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
import { useInputFocus, focusWindowImmediately, releaseWebViewFocus } from "@/hooks/useInputFocus";
import { useTranslation } from "@/i18n";
import { GROUP_VALUES, getGroups } from "@/lib/constants";
import { logError } from "@/lib/logger";
import { initTheme } from "@/lib/theme-applier";
import { cn } from "@/lib/utils";
import { useClipboardStore } from "@/stores/clipboard";
import { useGroupStore } from "@/stores/groups";
import type { Group } from "@/stores/groups";
import type { ToolbarButton } from "@/stores/ui-settings";
import { loadUISettingsFromBackend, useUISettings } from "@/stores/ui-settings";

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
  const { t, locale } = useTranslation();
  const categoryGroups = useMemo(() => getGroups(), [locale]);
  const [clearDialogOpen, setClearDialogOpen] = useState(false);
  const [isPinned, setIsPinned] = useState(false);
  // 分组对话框状态
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createName, setCreateName] = useState("");
  const [renameDialogOpen, setRenameDialogOpen] = useState(false);
  const [renameTarget, setRenameTarget] = useState<Group | null>(null);
  const [renameName, setRenameName] = useState("");
  const [deleteGroupDialogOpen, setDeleteGroupDialogOpen] = useState(false);
  const [deleteGroupTarget, setDeleteGroupTarget] = useState<Group | null>(null);

  const { searchQuery, selectedGroup, selectedGroupId, setSearchQuery, setSelectedGroup, setSelectedGroupId, fetchItems, clearHistory, refresh, resetView, itemCount } = useClipboardStore(
    useShallow((s) => ({
      searchQuery: s.searchQuery,
      selectedGroup: s.selectedGroup,
      selectedGroupId: s.selectedGroupId,
      setSearchQuery: s.setSearchQuery,
      setSelectedGroup: s.setSelectedGroup,
      setSelectedGroupId: s.setSelectedGroupId,
      fetchItems: s.fetchItems,
      clearHistory: s.clearHistory,
      refresh: s.refresh,
      resetView: s.resetView,
      itemCount: s.items.length,
    })),
  );
  const batchMode = useClipboardStore((s) => s.batchMode);
  const selectedIds = useClipboardStore((s) => s.selectedIds);
  const setBatchMode = useClipboardStore((s) => s.setBatchMode);
  const batchDelete = useClipboardStore((s) => s.batchDelete);
  const [batchDeleteDialogOpen, setBatchDeleteDialogOpen] = useState(false);
  const { groups, fetchGroups, createGroup, renameGroup, deleteGroup } = useGroupStore();
  const autoResetState = useUISettings((s) => s.autoResetState);
  const searchAutoFocus = useUISettings((s) => s.searchAutoFocus);
  const searchAutoClear = useUISettings((s) => s.searchAutoClear);
  const cardDensity = useUISettings((s) => s.cardDensity);
  const showCategoryFilter = useUISettings((s) => s.showCategoryFilter);
  const toolbarButtons = useUISettings((s) => s.toolbarButtons);
  const windowAnimation = useUISettings((s) => s.windowAnimation);
  const onboardingCompleted = useUISettings((s) => s.onboardingCompleted);
  const setOnboardingCompleted = useUISettings((s) => s.setOnboardingCompleted);
  const inputRef = useInputFocus<HTMLInputElement>();
  // 追踪窗口隐藏期间是否有剪贴板变化
  const clipboardDirtyRef = useRef(false);
  const segmentRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const segmentContainerRef = useRef<HTMLDivElement>(null);
  const [segmentIndicator, setSegmentIndicator] = useState({ left: 0, width: 0 });
  // 窗口入场动画状态：null = 初始（不添加任何 class），true = 入场动画，false = 隐藏
  const [windowVisible, setWindowVisible] = useState<boolean | null>(null);
  // 分组下拉状态
  const [groupDropdownOpen, setGroupDropdownOpen] = useState(false);
  const groupDropdownRef = useRef<HTMLDivElement>(null);
  // 分组对话框输入框 ref（Tauri WebView 中 autoFocus 不稳定）
  const createInputRef = useRef<HTMLInputElement>(null);
  const renameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    void initTheme();
  }, []);

  // 初次加载时获取自定义分组
  useEffect(() => {
    fetchGroups();
  }, []);

  // 对话框打开后恢复窗口焦点并聚焦输入框
  useEffect(() => {
    if (!createDialogOpen) return;
    const t = setTimeout(async () => {
      await focusWindowImmediately();
      createInputRef.current?.focus();
    }, 80);
    return () => clearTimeout(t);
  }, [createDialogOpen]);

  useEffect(() => {
    if (!renameDialogOpen) return;
    const t = setTimeout(async () => {
      await focusWindowImmediately();
      renameInputRef.current?.focus();
    }, 80);
    return () => clearTimeout(t);
  }, [renameDialogOpen]);

  // 点击外部/Escape 关闭分组下拉
  useEffect(() => {
    if (!groupDropdownOpen) return;
    const onPointerDown = (e: MouseEvent) => {
      if (!groupDropdownRef.current?.contains(e.target as Node)) {
        setGroupDropdownOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setGroupDropdownOpen(false);
        e.stopImmediatePropagation(); // 仅关闭下拉，不触发窗口隐藏
      }
    };
    document.addEventListener('mousedown', onPointerDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onPointerDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [groupDropdownOpen]);

  // 更新滑动指示器（始终跟踪类型筛选，与分组无关）
  const updateIndicator = useCallback(() => {
    const idx = GROUP_VALUES.findIndex((g) => g.value === selectedGroup);
    const el = segmentRefs.current[idx];
    if (el) {
      setSegmentIndicator({ left: el.offsetLeft, width: el.offsetWidth });
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

  // 分类栏隐藏时重置筛选
  useEffect(() => {
    if (!showCategoryFilter) {
      // 同步前后端分组状态
      useClipboardStore.setState({ selectedGroup: null });
      setSelectedGroupId(null);
    }
  }, [showCategoryFilter, setSelectedGroupId]);

  // ---- 分组操作 handlers ----
  const handleCreateGroup = async () => {
    if (!createName.trim()) return;
    await createGroup(createName.trim());
    setCreateDialogOpen(false);
    setCreateName("");
  };

  const openRenameDialog = (group: Group) => {
    setRenameTarget(group);
    setRenameName(group.name);
    setRenameDialogOpen(true);
  };

  const handleRenameGroup = async () => {
    if (!renameTarget || !renameName.trim()) return;
    await renameGroup(renameTarget.id, renameName.trim());
    setRenameDialogOpen(false);
    setRenameTarget(null);
  };

  const requestDeleteGroup = (group: Group) => {
    setDeleteGroupTarget(group);
    setDeleteGroupDialogOpen(true);
  };

  const confirmDeleteGroup = async () => {
    if (!deleteGroupTarget) return;
    const group = deleteGroupTarget;
    await deleteGroup(group.id);
    if (selectedGroupId === group.id) {
      setSelectedGroupId(null);
    }
    setDeleteGroupDialogOpen(false);
    setDeleteGroupTarget(null);
  };

  // 应用卡片密度到根元素
  useEffect(() => {
    document.documentElement.dataset.density = cardDensity;
  }, [cardDensity]);

  // 加载锁定状态 & 同步键盘导航设置到后端
  useEffect(() => {
    invoke<boolean>("is_window_pinned").then(setIsPinned);
    const kbNav = useUISettings.getState().keyboardNavigation;
    invoke("set_keyboard_nav_enabled", { enabled: kbNav }).catch((error) => {
      logError("Failed to sync keyboard navigation setting:", error);
    });
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
      setWindowVisible(true);
      // 重新读取设置（可能在设置窗口中更改）
      void loadUISettingsFromBackend();
      if (searchAutoClear) {
        setSearchQuery("");
        fetchItems({ search: "" });
      } else if (clipboardDirtyRef.current) {
        // 有变化时刷新以更新 files_valid
        refresh();
      }
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
  }, [refresh, fetchItems, setSearchQuery, searchAutoFocus, searchAutoClear]);

  // 窗口隐藏时关闭弹出层并可选重置状态
  useEffect(() => {
    const unlisten = listen("window-hidden", () => {
      releaseWebViewFocus();
      setWindowVisible(false);
      dismissOverlays();
      setGroupDropdownOpen(false);
      setBatchMode(false);
      if (autoResetState) {
        resetView();
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [resetView, autoResetState]);

  // ESC 键处理（后端钩子 + DOM 双通道）
  const handleEscape = useCallback(async () => {
    // 先关闭新手引导
    if (!onboardingCompleted) {
      setOnboardingCompleted(true);
      return;
    }
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
  }, [setBatchMode, onboardingCompleted, setOnboardingCompleted]);

  // 通道1：后端键盘钩子
  useEffect(() => {
    const unlisten = listen("escape-pressed", handleEscape);
    return () => { unlisten.then((fn) => fn()); };
  }, [handleEscape]);

  // DOM 键盘事件通道
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

  const clearScopeText = useMemo(() => {
    if (selectedGroup === "text,html,rtf") {
      return t("app.clearHistoryConfirmText");
    }
    if (selectedGroup === "image,files,url") {
      return t("app.clearHistoryConfirmOther");
    }
    if (selectedGroup === "__favorites__") {
      return t("app.clearHistoryFavoritesBlocked");
    }
    return t("app.clearHistoryConfirmAll");
  }, [selectedGroup, t]);

  const handleClearHistory = async () => {
    if (selectedGroup === "__favorites__") {
      setClearDialogOpen(false);
      return;
    }
    await clearHistory(selectedGroup);
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
            <TooltipContent>{t("toolbar.clearHistory")}</TooltipContent>
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
                  <LockClosed16Filled className="w-4 h-4" />
                ) : (
                  <LockClosed16Regular className="w-4 h-4" />
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent>{isPinned ? t("toolbar.unpinWindow") : t("toolbar.pinWindow")}</TooltipContent>
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
            <TooltipContent>{batchMode ? t("toolbar.exitBatchSelect") : t("toolbar.batchSelect")}</TooltipContent>
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
            <TooltipContent>{t("toolbar.settings")}</TooltipContent>
          </Tooltip>
        );
      default:
        return null;
    }
  }, [isPinned, openSettings, togglePinned, batchMode, setBatchMode, t]);

  return (
    <div className={cn("h-screen flex flex-col bg-muted/40 overflow-hidden", windowAnimation && windowVisible === true && "window-enter", windowAnimation && windowVisible === false && "window-hidden")}>
      {/* 顶栏：搜索 + 操作 */}
      <div
        className="flex items-center gap-2 px-2 pt-2 pb-0.5 shrink-0 select-none"
        data-tauri-drag-region
      >
        {/* 搜索栏 */}
        <div className="relative flex-1" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
          <Search16Regular className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground z-10" />
          <Input
            ref={inputRef}
            type="text"
            placeholder={t("app.searchPlaceholder")}
            value={searchQuery}
            onChange={handleSearchChange}
            className={cn("pl-9 h-9 text-sm bg-background border shadow-sm", searchQuery && "pr-14")}
          />
          {searchQuery && (
            <div className="absolute right-8 top-1/2 -translate-y-1/2 flex items-center gap-1 z-10 pointer-events-none">
              <span className="text-xs text-muted-foreground tabular-nums">{t("app.searchResultCount", { count: itemCount })}</span>
            </div>
          )}
          {searchQuery && (
            <button
              onClick={() => { setSearchQuery(""); fetchItems({ search: "" }); }}
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
            style={{ WebkitAppRegion: 'no-drag', pointerEvents: suppressTooltips ? 'none' : undefined } as React.CSSProperties}
          >
            {toolbarButtons.map((btn) => renderToolbarButton(btn))}
          </div>
        )}
      </div>

      {/* 批量操作栏 */}
      {batchMode && (
        <div className="shrink-0 flex items-center justify-between px-3 py-1.5 bg-primary/5 border-b border-primary/20">
          <span className="text-xs text-muted-foreground">
            {t("app.batchSelected", { count: selectedIds.size })}
            <span className="ml-1.5 text-muted-foreground/60">{t("app.batchShiftHint")}</span>
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
              {t("app.batchMergePaste")}
            </button>
            <button
              onClick={() => setBatchDeleteDialogOpen(true)}
              disabled={selectedIds.size === 0}
              className="text-xs px-2 py-1 rounded bg-destructive/10 text-destructive hover:bg-destructive/20 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              {t("common.delete")}
            </button>
            <button
              onClick={() => setBatchMode(false)}
              className="text-xs px-2 py-1 rounded hover:bg-accent transition-colors text-muted-foreground hover:text-foreground"
            >
              {t("common.cancel")}
            </button>
          </div>
        </div>
      )}

      {/* 剪贴板列表 */}
      <div className="flex-1 overflow-hidden">
        <ClipboardList searchInputRef={inputRef} />
      </div>

      {/* 底部分组选择 */}
      {showCategoryFilter && (
        <div className="shrink-0 px-2 pb-2 pt-1 select-none">
          {/* 整个底标2区共用一个 bg-muted rounded-lg 容器 */}
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
            {categoryGroups.map((g, i) => (
              <button
                key={g.label}
                ref={(el) => { segmentRefs.current[i] = el; }}
                onClick={() => setSelectedGroup(g.value)}
                className={cn(
                  "relative z-1 flex-1 h-full rounded-md text-xs font-medium transition-colors duration-200",
                  selectedGroup === g.value
                    ? "text-foreground"
                    : "text-muted-foreground hover:text-foreground",
                )}
              >
                {g.label}
              </button>
            ))}

            {/* 分隔线 */}
            <div className="relative z-1 w-px h-4 bg-border/50 mx-0.5 shrink-0" />

            {/* 分组下拉切换器（和 tab 共用同一容器） */}
            <div ref={groupDropdownRef} className="relative z-1 shrink-0">
              <button
                onClick={() => setGroupDropdownOpen((o) => !o)}
                className="h-7 flex items-center gap-1 px-2 rounded-md bg-background shadow-sm text-xs font-medium text-foreground transition-all duration-200"
              >
                <span className="max-w-[80px] truncate">
                  {selectedGroupId === null
                    ? t("groups.defaultGroup")
                    : (groups.find((g) => g.id === selectedGroupId)?.name ?? t("groups.defaultGroup"))}
                </span>
                <ChevronDown16Regular
                  className={cn("w-3 h-3 transition-transform duration-150", groupDropdownOpen && "-rotate-180")}
                />
              </button>

              {/* 下拉面板 */}
              {groupDropdownOpen && (
                <div className="absolute bottom-full right-0 mb-1 z-50 min-w-[160px] rounded-md border bg-popover p-1 shadow-md">
                  {/* 默认选项 */}
                  <div
                    onClick={() => { setSelectedGroupId(null); setGroupDropdownOpen(false); }}
                    className={cn(
                      "flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-default hover:bg-accent hover:text-accent-foreground",
                      selectedGroupId === null && "bg-accent/50 text-foreground"
                    )}
                  >
                    <span>{t("groups.defaultGroup")}</span>
                  </div>

                  {/* 自定义分组列表 */}
                  {groups.length > 0 && (
                    <>
                      <div className="-mx-1 my-1 h-px bg-border" />
                      <div className="max-h-48 overflow-y-auto">
                        {groups.map((g) => (
                          <div
                            key={g.id}
                            onClick={() => { setSelectedGroupId(g.id); setGroupDropdownOpen(false); }}
                            className={cn(
                              "flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-default hover:bg-accent hover:text-accent-foreground group/row",
                              selectedGroupId === g.id && "bg-accent/50 text-foreground"
                            )}
                          >
                            <span className="flex-1 min-w-0 truncate">{g.name}</span>
                            <div
                              className="flex items-center gap-0.5 opacity-0 group-hover/row:opacity-100 transition-opacity"
                              onClick={(e) => e.stopPropagation()}
                            >
                              <button
                                onClick={() => { openRenameDialog(g); setGroupDropdownOpen(false); }}
                                className="w-5 h-5 flex items-center justify-center rounded hover:bg-muted"
                              >
                                <Edit16Regular className="w-3 h-3" />
                              </button>
                              <button
                                onClick={() => { requestDeleteGroup(g); setGroupDropdownOpen(false); }}
                                className="w-5 h-5 flex items-center justify-center rounded hover:bg-muted text-destructive"
                              >
                                <Delete16Regular className="w-3 h-3" />
                              </button>
                            </div>
                          </div>
                        ))}
                      </div>
                    </>
                  )}

                  <div className="-mx-1 my-1 h-px bg-border" />
                  <div
                    onClick={() => { setCreateDialogOpen(true); setGroupDropdownOpen(false); }}
                    className="flex items-center gap-2 rounded-sm px-2 py-1.5 text-xs cursor-default hover:bg-accent hover:text-accent-foreground"
                  >
                    <Add16Regular className="w-3.5 h-3.5" />
                    {t("groups.createGroup")}
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* 批量删除确认对话框 */}
      <Dialog open={batchDeleteDialogOpen} onOpenChange={setBatchDeleteDialogOpen}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left">
            <DialogTitle>{t("app.batchDeleteTitle")}</DialogTitle>
            <DialogDescription className="text-left">
              {t("app.batchDeleteDescription", { count: selectedIds.size })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setBatchDeleteDialogOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={async () => {
                setBatchDeleteDialogOpen(false);
                await batchDelete();
              }}
            >
              {t("common.delete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 清空历史确认对话框 */}
      <Dialog open={clearDialogOpen} onOpenChange={setClearDialogOpen}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left">
            <DialogTitle>{t("app.clearHistoryTitle")}</DialogTitle>
            <DialogDescription className="text-left">
              {clearScopeText}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setClearDialogOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={handleClearHistory}
              disabled={selectedGroup === "__favorites__"}
            >
              {t("common.clear")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 新建分组对话框 */}
      <Dialog open={createDialogOpen} onOpenChange={(open) => {
        setCreateDialogOpen(open);
        if (!open) setCreateName("");
      }}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left">
            <DialogTitle>{t("groups.createGroup")}</DialogTitle>
          </DialogHeader>
          <Input
            ref={createInputRef}
            placeholder={t("groups.groupNamePlaceholder")}
            value={createName}
            onChange={(e) => setCreateName(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') handleCreateGroup(); }}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateDialogOpen(false)}>{t("common.cancel")}</Button>
            <Button onClick={handleCreateGroup} disabled={!createName.trim()}>{t("common.create")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 重命名分组对话框 */}
      <Dialog open={renameDialogOpen} onOpenChange={(open) => {
        setRenameDialogOpen(open);
        if (!open) setRenameTarget(null);
      }}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left">
            <DialogTitle>{t("groups.editGroup")}</DialogTitle>
          </DialogHeader>
          <Input
            ref={renameInputRef}
            placeholder={t("groups.groupNamePlaceholder")}
            value={renameName}
            onChange={(e) => setRenameName(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') handleRenameGroup(); }}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenameDialogOpen(false)}>{t("common.cancel")}</Button>
            <Button onClick={handleRenameGroup} disabled={!renameName.trim()}>{t("common.confirm")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 删除分组确认对话框 */}
      <Dialog open={deleteGroupDialogOpen} onOpenChange={(open) => {
        setDeleteGroupDialogOpen(open);
        if (!open) setDeleteGroupTarget(null);
      }}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left">
            <DialogTitle>{t("groups.deleteGroup")}</DialogTitle>
            <DialogDescription className="text-left">
              {t("groups.deleteGroupConfirm", { name: deleteGroupTarget?.name ?? "" })}
              {typeof deleteGroupTarget?.item_count === "number" && (
                <>
                  <br />
                  {t("groups.deleteGroupItemCount", { count: deleteGroupTarget.item_count })}
                </>
              )}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteGroupDialogOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button variant="destructive" onClick={confirmDeleteGroup} disabled={!deleteGroupTarget}>
              {t("common.delete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 新手引导 */}
      {!onboardingCompleted && (
        <Onboarding onComplete={() => setOnboardingCompleted(true)} />
      )}
    </div>
  );
}

export default App;

