import { useEffect, useRef, useCallback, useMemo, useState } from "react";
import {
  ClipboardMultiple16Regular,
  Filter16Regular,
  Search16Regular,
  ArrowUp16Regular,
} from "@fluentui/react-icons";
import { listen } from "@tauri-apps/api/event";
import { OverlayScrollbarsComponent } from "overlayscrollbars-react";
import { Virtuoso, VirtuosoHandle } from "react-virtuoso";
import { useShallow } from "zustand/react/shallow";
import { Separator } from "@/components/ui/separator";
import { useSortableList } from "@/hooks/useSortableList";
import { GROUPS } from "@/lib/constants";
import { useClipboardStore, ClipboardItem } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";
import { ClipboardItemCard } from "./ClipboardItemCard";
import type { OverlayScrollbars } from "overlayscrollbars";

interface SortableClipboardItem extends ClipboardItem {
  _sortId: string;
}

// Virtuoso scrollSeek 占位符 — 快速滚动时替代完整卡片，接收精确高度避免布局抖动
const ScrollSeekPlaceholder = ({ height }: { height: number }) => (
  <div style={{ height }} className="px-2 pb-2">
    <div className="rounded-lg border bg-card overflow-hidden px-3 py-2.5 h-full">
      <div className="space-y-1.5">
        <div className="h-4 bg-muted rounded w-4/5" />
        <div className="h-3.5 bg-muted/70 rounded w-3/5" />
        <div className="h-3 bg-muted/50 rounded w-2/5" />
      </div>
      <div className="flex items-center gap-1.5 mt-1.5">
        <div className="h-3 bg-muted/40 rounded w-16" />
        <div className="h-3 bg-muted/40 rounded w-12" />
      </div>
    </div>
  </div>
);

export function ClipboardList() {
  const listenerRef = useRef<(() => void) | null>(null);
  const scrollerRef = useRef<HTMLElement | null>(null);
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const osInstanceRef = useRef<OverlayScrollbars | null>(null);
  const [customScrollParent, setCustomScrollParent] =
    useState<HTMLElement | null>(null);
  const [showScrollTop, setShowScrollTop] = useState(false);
  const {
    items,
    isLoading,
    searchQuery,
    selectedGroup,
    fetchItems,
    setupListener,
    moveItem,
    togglePin,
    setActiveIndex,
    pasteContent,
    pasteAsPlainText,
    deleteItem,
    _resetToken,
  } = useClipboardStore(
    useShallow((s) => ({
      items: s.items,
      isLoading: s.isLoading,
      searchQuery: s.searchQuery,
      selectedGroup: s.selectedGroup,
      fetchItems: s.fetchItems,
      setupListener: s.setupListener,
      moveItem: s.moveItem,
      togglePin: s.togglePin,
      setActiveIndex: s.setActiveIndex,
      pasteContent: s.pasteContent,
      pasteAsPlainText: s.pasteAsPlainText,
      deleteItem: s.deleteItem,
      _resetToken: s._resetToken,
    })),
  );
  const cardMaxLines = useUISettings((s) => s.cardMaxLines);
  const cardDensity = useUISettings((s) => s.cardDensity);

  useEffect(() => {
    // Fetch items (files_valid is computed by backend, no extra IPC needed)
    fetchItems();
    if (listenerRef.current) return;
    let mounted = true;
    setupListener().then((unlisten) => {
      if (mounted) listenerRef.current = unlisten;
      else unlisten();
    });
    return () => {
      mounted = false;
      if (listenerRef.current) {
        listenerRef.current();
        listenerRef.current = null;
      }
    };
  }, []);

  const itemsWithSortId = useMemo(
    (): SortableClipboardItem[] =>
      items.map((item) => ({ ...item, _sortId: `item-${item.id}` })),
    [items],
  );

  // 后端已按 is_pinned DESC 排序，直接计算置顶数即可
  const pinnedCount = useMemo(
    () => itemsWithSortId.filter((item) => item.is_pinned).length,
    [itemsWithSortId],
  );

  // 搜索/筛选时隐藏快捷粘贴序号（过滤后的顺序与快捷粘贴的全局顺序不一致）
  const showSlotBadges = !searchQuery && !selectedGroup;

  const handleDragEnd = useCallback(
    async (oldIndex: number, newIndex: number) => {
      if (oldIndex === newIndex) return;
      const fromItem = itemsWithSortId[oldIndex];
      const toItem = itemsWithSortId[newIndex];
      if (!fromItem || !toItem) return;

      const fromIsPinned = oldIndex < pinnedCount;
      const toIsPinned = newIndex < pinnedCount;

      // 跨区域拖拽：自动改变置顶状态，然后移动到目标位置
      if (fromIsPinned !== toIsPinned) {
        await togglePin(fromItem.id);
        await moveItem(fromItem.id, toItem.id);
      } else {
        // 同区域拖拽：移动位置
        await moveItem(fromItem.id, toItem.id);
      }
    },
    [itemsWithSortId, pinnedCount, moveItem, togglePin],
  );

  const {
    DndContext,
    SortableContext,
    DragOverlay,
    sensors,
    handleDragStart,
    handleDragEnd: onDragEnd,
    handleDragCancel,
    activeId,
    activeItem,
    strategy,
    modifiers,
    collisionDetection,
    measuring,
  } = useSortableList({
    items: itemsWithSortId,
    onDragEnd: handleDragEnd,
  });

  // 拖拽时接管滚轮事件 - QuickClipboard 优化
  useEffect(() => {
    if (!activeId) return;

    const handleWheel = (e: WheelEvent) => {
      e.preventDefault();
      if (scrollerRef.current) {
        scrollerRef.current.scrollTop += e.deltaY;
      }
    };

    // 使用 capture phase 确保在其他事件处理器之前捕获
    document.addEventListener("wheel", handleWheel, {
      passive: false,
      capture: true,
    });

    return () => {
      document.removeEventListener("wheel", handleWheel, {
        capture: true,
      });
    };
  }, [activeId]);

  // 监听滚动位置，控制回到顶部按钮的显示——节流避免滚动时大量 re-render
  useEffect(() => {
    if (!customScrollParent) return;
    let ticking = false;
    const handleScroll = () => {
      if (ticking) return;
      ticking = true;
      requestAnimationFrame(() => {
        setShowScrollTop(customScrollParent.scrollTop > 200);
        ticking = false;
      });
    };
    customScrollParent.addEventListener("scroll", handleScroll, { passive: true });
    return () => customScrollParent.removeEventListener("scroll", handleScroll);
  }, [customScrollParent]);

  // 回到顶部 - 使用 Virtuoso scrollToIndex API（虚拟列表直接操作 scrollTop 无法正确回到顶部）
  const scrollToTop = useCallback((smooth = false) => {
    virtuosoRef.current?.scrollToIndex({
      index: 0,
      align: "start",
      behavior: smooth ? "smooth" : "auto",
    });
  }, []);

  // 窗口重新打开时重置滚动位置
  useEffect(() => {
    if (_resetToken > 0) {
      scrollToTop();
    }
  }, [_resetToken, scrollToTop]);

  // 键盘导航：共用处理函数（DOM keydown 与 Tauri 低级钩子事件均调用此函数）
  const handleNavKey = useCallback(
    (key: string, shift: boolean) => {
      if (!useUISettings.getState().keyboardNavigation) return;

      switch (key) {
        case "ArrowLeft": {
          if (!useUISettings.getState().showCategoryFilter) break;
          if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
          const { selectedGroup, setSelectedGroup } = useClipboardStore.getState();
          const curIdx = GROUPS.findIndex((g) => g.value === selectedGroup);
          if (curIdx > 0) setSelectedGroup(GROUPS[curIdx - 1].value);
          break;
        }
        case "ArrowRight": {
          if (!useUISettings.getState().showCategoryFilter) break;
          if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
          const { selectedGroup, setSelectedGroup } = useClipboardStore.getState();
          const curIdx = GROUPS.findIndex((g) => g.value === selectedGroup);
          if (curIdx < GROUPS.length - 1) setSelectedGroup(GROUPS[curIdx + 1].value);
          break;
        }
        case "ArrowUp": {
          const { items: upItems, activeIndex: cur } = useClipboardStore.getState();
          if (upItems.length === 0) return;
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
    [setActiveIndex, pasteContent, pasteAsPlainText, deleteItem],
  );

  // 路径 1：DOM keydown（窗口自身聚焦时，如搜索框）
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (e.key === "Delete" && (tag === "INPUT" || tag === "TEXTAREA")) return;
      if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Enter", "Delete"].includes(e.key)) {
        e.preventDefault();
        handleNavKey(e.key, e.shiftKey);
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleNavKey]);

  // 路径 2：Tauri 事件（低级键盘钩子转发，窗口无需聚焦）
  // 窗口聚焦时 DOM keydown 已处理，跳过钩子事件避免重复触发
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

  // 拖拽时添加全局光标样式
  useEffect(() => {
    if (!activeId) return;
    document.body.classList.add("dragging-cursor");
    return () => document.body.classList.remove("dragging-cursor");
  }, [activeId]);

  const defaultItemHeight = useMemo(
    () => 20 + cardMaxLines * 20 + 20 + 8,
    [cardMaxLines],
  );

  const sortableIds = useMemo(
    () => itemsWithSortId.map((i) => i._sortId),
    [itemsWithSortId],
  );

  const itemContent = useCallback(
    (index: number) => {
      const item = itemsWithSortId[index];
      if (!item) return null;

      const showSeparator = index === pinnedCount && pinnedCount > 0;

      const densityPb = cardDensity === "compact" ? "pb-1" : cardDensity === "spacious" ? "pb-3" : "pb-2";
      return (
        <div className={`px-2 ${densityPb}`}>
          {showSeparator && <Separator className="mb-2" />}
          <ClipboardItemCard item={item} index={index} showBadge={showSlotBadges} sortId={item._sortId} />
        </div>
      );
    },
    [itemsWithSortId, pinnedCount, showSlotBadges, cardDensity],
  );

  const computeItemKey = useCallback(
    (index: number) => itemsWithSortId[index]?._sortId || `item-${index}`,
    [itemsWithSortId],
  );

  if (isLoading && items.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center h-full">
        <div className="text-center space-y-3">
          <div className="w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin mx-auto" />
          <p className="text-sm text-muted-foreground">加载中...</p>
        </div>
      </div>
    );
  }

  // 搜索/筛选无结果
  if (items.length === 0 && (searchQuery || selectedGroup)) {
    return (
      <div className="flex-1 flex items-center justify-center h-full">
        <div className="text-center space-y-4">
          <div className="w-16 h-16 rounded-full bg-muted flex items-center justify-center mx-auto">
            {searchQuery
              ? <Search16Regular className="w-8 h-8 text-muted-foreground" />
              : <Filter16Regular className="w-8 h-8 text-muted-foreground" />
            }
          </div>
          <div className="space-y-1">
            <p className="text-sm font-medium">
              {searchQuery ? "未找到匹配的内容" : "暂无此类型的内容"}
            </p>
            <p className="text-sm text-muted-foreground">
              {searchQuery ? "试试其他关键词" : "试试其他分类"}
            </p>
          </div>
        </div>
      </div>
    );
  }

  if (items.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center h-full">
        <div className="text-center space-y-4">
          <div className="w-16 h-16 rounded-full bg-muted flex items-center justify-center mx-auto">
            <ClipboardMultiple16Regular className="w-8 h-8 text-muted-foreground" />
          </div>
          <div className="space-y-1">
            <p className="text-sm font-medium">暂无剪贴板历史</p>
            <p className="text-sm text-muted-foreground">
              复制任意内容开始记录
            </p>
          </div>
        </div>
      </div>
    );
  }

  const activeItemData = activeItem as SortableClipboardItem | null;

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={collisionDetection}
      onDragStart={handleDragStart}
      onDragEnd={onDragEnd}
      onDragCancel={handleDragCancel}
      modifiers={modifiers}
      measuring={measuring}
    >
      <div className="h-full relative">
        <OverlayScrollbarsComponent
          element="div"
          options={{
            scrollbars: {
              theme: "os-theme-custom",
              visibility: "auto",
              autoHide: "scroll",
              autoHideDelay: 1000,
            },
            overflow: {
              x: "hidden",
              y: "scroll",
            },
          }}
          events={{
            initialized: (instance: OverlayScrollbars) => {
              osInstanceRef.current = instance;
              const viewport = instance.elements().viewport;
              setCustomScrollParent(viewport);
            },
          }}
          defer
          style={{ height: "100%" }}
        >
          <SortableContext
            items={sortableIds}
            strategy={strategy}
          >
            {customScrollParent && (
              <Virtuoso
                ref={virtuosoRef}
                totalCount={itemsWithSortId.length}
                itemContent={itemContent}
                computeItemKey={computeItemKey}
                defaultItemHeight={defaultItemHeight}
                increaseViewportBy={{ top: 400, bottom: 400 }}
                scrollSeekConfiguration={{
                  enter: (velocity) => Math.abs(velocity) > 2000,
                  exit: (velocity) => Math.abs(velocity) < 500,
                }}
                components={{ ScrollSeekPlaceholder }}
                customScrollParent={customScrollParent}
                scrollerRef={(ref) => {
                  if (ref instanceof HTMLElement) {
                    scrollerRef.current = ref;
                  }
                }}
              />
            )}
          </SortableContext>
        </OverlayScrollbarsComponent>
        {/* 回到顶部悬浮按钮 */}
        <button
          onClick={() => scrollToTop(true)}
          className={`absolute right-3 bottom-3 w-7 h-7 rounded-md bg-background border shadow-sm flex items-center justify-center text-muted-foreground hover:bg-accent hover:text-accent-foreground transition-all duration-200 z-10 ${
            showScrollTop
              ? "opacity-100 scale-100 pointer-events-auto"
              : "opacity-0 scale-75 pointer-events-none"
          }`}
          title="回到顶部"
        >
          <ArrowUp16Regular className="w-4 h-4" />
        </button>
      </div>

      <DragOverlay dropAnimation={null} style={{ cursor: "grabbing" }}>
        {activeItemData && (
          <ClipboardItemCard
            item={activeItemData}
            index={-1}
            isDragOverlay={true}
          />
        )}
      </DragOverlay>
    </DndContext>
  );
}
