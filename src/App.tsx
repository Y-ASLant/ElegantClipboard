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
} from "@fluentui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
import { useInputFocus, focusWindowImmediately } from "@/hooks/useInputFocus";
import { GROUPS } from "@/lib/constants";
import { logError } from "@/lib/logger";
import { initTheme } from "@/lib/theme-applier";
import { cn } from "@/lib/utils";
import { useClipboardStore } from "@/stores/clipboard";
import { useGroupStore } from "@/stores/groups";
import type { Group } from "@/stores/groups";
import type { ToolbarButton } from "@/stores/ui-settings";
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
  // 分组对话框状态
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createName, setCreateName] = useState("");
  const [renameDialogOpen, setRenameDialogOpen] = useState(false);
  const [renameTarget, setRenameTarget] = useState<Group | null>(null);
  const [renameName, setRenameName] = useState("");
  const [deleteGroupDialogOpen, setDeleteGroupDialogOpen] = useState(false);
  const [deleteGroupTarget, setDeleteGroupTarget] = useState<Group | null>(null);

  const { searchQuery, selectedGroup, selectedGroupId, setSearchQuery, setSelectedGroup, setSelectedGroupId, fetchItems, clearHistory, refresh, resetView } = useClipboardStore();
  const { groups, fetchGroups, createGroup, renameGroup, deleteGroup } = useGroupStore();
  const autoResetState = useUISettings((s) => s.autoResetState);
  const searchAutoFocus = useUISettings((s) => s.searchAutoFocus);
  const searchAutoClear = useUISettings((s) => s.searchAutoClear);
  const cardDensity = useUISettings((s) => s.cardDensity);
  const showCategoryFilter = useUISettings((s) => s.showCategoryFilter);
  const toolbarButtons = useUISettings((s) => s.toolbarButtons);
  const inputRef = useInputFocus<HTMLInputElement>();
  // 追踪窗口隐藏期间是否有剪贴板变化
  const clipboardDirtyRef = useRef(false);
  const segmentRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const segmentContainerRef = useRef<HTMLDivElement>(null);
  const [segmentIndicator, setSegmentIndicator] = useState({ left: 0, width: 0 });
  // 分组下拉状态
  const [groupDropdownOpen, setGroupDropdownOpen] = useState(false);
  const groupDropdownRef = useRef<HTMLDivElement>(null);
  // 分组对话框输入框 ref（autoFocus 在 Tauri WebView portal 内不稳定，用 ref 代替）
  const createInputRef = useRef<HTMLInputElement>(null);
  const renameInputRef = useRef<HTMLInputElement>(null);

  // 初次加载时获取自定义分组
  useEffect(() => {
    fetchGroups();
  }, []);

  // 对话框打开后主动聚焦输入框（autoFocus 在 Tauri WebView portal 中不可靠）
  useEffect(() => {
    if (!createDialogOpen) return;
    const t = setTimeout(() => createInputRef.current?.focus(), 80);
    return () => clearTimeout(t);
  }, [createDialogOpen]);

  useEffect(() => {
    if (!renameDialogOpen) return;
    const t = setTimeout(() => renameInputRef.current?.focus(), 80);
    return () => clearTimeout(t);
  }, [renameDialogOpen]);

  // 点击外部 / Escape 关闭分组下拉
  useEffect(() => {
    if (!groupDropdownOpen) return;
    const onPointerDown = (e: MouseEvent) => {
      if (!groupDropdownRef.current?.contains(e.target as Node)) {
        setGroupDropdownOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setGroupDropdownOpen(false); };
    document.addEventListener('mousedown', onPointerDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onPointerDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [groupDropdownOpen]);

  // 更新滑动指示器（始终跟踪类型筛选，与分组无关）
  const updateIndicator = useCallback(() => {
    const idx = GROUPS.findIndex((g) => g.value === selectedGroup);
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

  // 隐藏分类栏时重置全部筛选状态
  useEffect(() => {
    if (!showCategoryFilter) {
      useClipboardStore.setState({ selectedGroup: null, selectedGroupId: null });
      useClipboardStore.getState().fetchItems();
    }
  }, [showCategoryFilter]);

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
    invoke("set_keyboard_nav_enabled", { enabled: kbNav }).catch(() => {});
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
      dismissOverlays();
      if (autoResetState) {
        resetView();
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [resetView, autoResetState]);

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
                  <LockClosed16Filled className="w-4 h-4" />
                ) : (
                  <LockClosed16Regular className="w-4 h-4" />
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent>{isPinned ? "解除锁定" : "锁定窗口"}</TooltipContent>
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
  }, [isPinned, openSettings, togglePinned]);

  return (
    <div className="h-screen flex flex-col bg-muted/40 overflow-hidden">
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
            placeholder="搜索剪贴板..."
            value={searchQuery}
            onChange={handleSearchChange}
            className={cn("pl-9 h-9 text-sm bg-background border shadow-sm", searchQuery && "pr-8")}
          />
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

      {/* 剪贴板列表 */}
      <div className="flex-1 overflow-hidden">
        <ClipboardList />
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

            {/* 分隔线 */}
            <div className="relative z-[1] w-px h-4 bg-border/50 mx-0.5 shrink-0" />

            {/* 分组下拉切换器（和 tab 共用同一容器） */}
            <div ref={groupDropdownRef} className="relative z-[1] shrink-0">
              <button
                onClick={() => setGroupDropdownOpen((o) => !o)}
                className="h-7 flex items-center gap-1 px-2 rounded-md bg-background shadow-sm text-xs font-medium text-foreground transition-all duration-200"
              >
                <span className="max-w-[80px] truncate">
                  {selectedGroupId === null
                    ? '默认'
                    : (groups.find((g) => g.id === selectedGroupId)?.name ?? '默认')}
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
                    <span>默认</span>
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
                    新建分组
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

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

      {/* 新建分组对话框 */}
      <Dialog open={createDialogOpen} onOpenChange={(open) => {
        setCreateDialogOpen(open);
        if (!open) setCreateName("");
      }}>
        <DialogContent showCloseButton={false}>
          <DialogHeader className="text-left">
            <DialogTitle>新建分组</DialogTitle>
          </DialogHeader>
          <Input
            ref={createInputRef}
            placeholder="分组名称"
            value={createName}
            onChange={(e) => setCreateName(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') handleCreateGroup(); }}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateDialogOpen(false)}>取消</Button>
            <Button onClick={handleCreateGroup} disabled={!createName.trim()}>创建</Button>
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
            <DialogTitle>编辑分组</DialogTitle>
          </DialogHeader>
          <Input
            ref={renameInputRef}
            placeholder="分组名称"
            value={renameName}
            onChange={(e) => setRenameName(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') handleRenameGroup(); }}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenameDialogOpen(false)}>取消</Button>
            <Button onClick={handleRenameGroup} disabled={!renameName.trim()}>确定</Button>
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
            <DialogTitle>删除分组</DialogTitle>
            <DialogDescription className="text-left">
              确定要删除分组“{deleteGroupTarget?.name ?? ""}”吗？该分组下的所有剪贴板记录将被同时删除（不可撤销）。
              {typeof deleteGroupTarget?.item_count === "number" && (
                <>
                  <br />
                  当前分组条目数：{deleteGroupTarget.item_count}
                </>
              )}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteGroupDialogOpen(false)}>
              取消
            </Button>
            <Button variant="destructive" onClick={confirmDeleteGroup} disabled={!deleteGroupTarget}>
              删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export default App;

