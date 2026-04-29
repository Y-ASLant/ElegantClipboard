import { useEffect, useRef, useCallback, useMemo, useState, type RefObject } from "react";
import { CSS } from "@dnd-kit/utilities";
import {
  ClipboardMultiple16Regular,
  Search16Regular,
} from "@fluentui/react-icons";
import { listen } from "@tauri-apps/api/event";
import { OverlayScrollbarsComponent } from "overlayscrollbars-react";
import { Virtuoso, VirtuosoHandle } from "react-virtuoso";
import { useShallow } from "zustand/react/shallow";
import { ScrollToTopButton } from "@/components/ScrollToTopButton";
import { Separator } from "@/components/ui/separator";
import { focusWindowImmediately } from "@/hooks/useInputFocus";
import { useSortableList } from "@/hooks/useSortableList";
import { getVisibleCategories } from "@/lib/constants";
import { logError } from "@/lib/logger";
import { useClipboardStore, ClipboardItem } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";
import { ClipboardItemCard } from "./ClipboardItemCard";
import type { OverlayScrollbars } from "overlayscrollbars";

interface SortableClipboardItem extends ClipboardItem {
  _sortId: string;
}

interface ClipboardListProps {
  searchInputRef: RefObject<HTMLInputElement | null>;
}

function getPinnedBoundary(items: SortableClipboardItem[]): number {
  const firstUnpinnedIndex = items.findIndex((item) => !item.is_pinned);
  return firstUnpinnedIndex === -1 ? items.length : firstUnpinnedIndex;
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

export function ClipboardList({ searchInputRef }: ClipboardListProps) {
  const listenerRef = useRef<(() => void) | null>(null);
  const scrollerRef = useRef<HTMLElement | null>(null);
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const osInstanceRef = useRef<OverlayScrollbars | null>(null);
  const focusSearchInFlightRef = useRef<Promise<void> | null>(null);
  const [customScrollParent, setCustomScrollParent] =
    useState<HTMLElement | null>(null);
  const [showScrollTop, setShowScrollTop] = useState(false);
  const listContainerRef = useRef<HTMLDivElement>(null);
  const [hideScrollTopForToolbar, setHideScrollTopForToolbar] = useState(false);
  const [optimisticItems, setOptimisticItems] = useState<SortableClipboardItem[] | null>(null);
  const {
    items,
    isLoading,
    searchQuery,
    selectedCategory,
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
      selectedCategory: s.selectedCategory,
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
    // 组件挂载时加载数据
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

  const renderedItems = optimisticItems ?? itemsWithSortId;

  useEffect(() => {
    // 服务端确认顺序到达，清除乐观视图
    setOptimisticItems(null);
  }, [itemsWithSortId]);

  // 后端已按 is_pinned DESC 排序，直接找到分界点即可
  const pinnedCount = useMemo(
    () => getPinnedBoundary(renderedItems),
    [renderedItems],
  );

  // 搜索/类型筛选时隐藏快捷粘贴序号（过滤后的顺序与快捷粘贴槽位顺序不一致）
  const showSlotBadges = !searchQuery && !selectedCategory;

  const handleDragEnd = useCallback(
    async (oldIndex: number, newIndex: number) => {
      if (oldIndex === newIndex) return;
      const currentItems = renderedItems;
      const fromItem = currentItems[oldIndex];
      const toItem = currentItems[newIndex];
      if (!fromItem || !toItem) return;

      const currentPinnedCount = getPinnedBoundary(currentItems);
      const fromIsPinned = oldIndex < currentPinnedCount;
      const toIsPinned = newIndex < currentPinnedCount;

      // 先在 UI 上重排，让拖拽覆盖层直接落到目标位置
      setOptimisticItems(() => {
        const next = [...currentItems];
        const [moved] = next.splice(oldIndex, 1);
        if (!moved) return currentItems;
        next.splice(newIndex, 0, { ...moved, is_pinned: toIsPinned });
        return next;
      });

      try {
        if (fromIsPinned !== toIsPinned) {
          await togglePin(fromItem.id);
          await moveItem(fromItem.id, toItem.id);
        } else {
          await moveItem(fromItem.id, toItem.id);
        }
      } catch {
        // store 内部已记录错误
      } finally {
        setOptimisticItems(null);
      }
    },
    [renderedItems, moveItem, togglePin],
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
    items: renderedItems,
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

    // capture 阶段优先捕获
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

  // 工具栏与回到顶部按钮重叠检测：悬浮工具栏时若遮挡按钮则暂时隐藏
  const handleToolbarMouseOver = useCallback((e: React.MouseEvent) => {
    const toolbar = (e.target as HTMLElement).closest('[data-action-toolbar]');
    if (!toolbar) return;
    const btn = listContainerRef.current?.querySelector('[data-scroll-top-btn]');
    if (!btn) return;
    const tRect = toolbar.getBoundingClientRect();
    const bRect = btn.getBoundingClientRect();
    const overlaps = tRect.left < bRect.right && tRect.right > bRect.left &&
                     tRect.top < bRect.bottom && tRect.bottom > bRect.top;
    setHideScrollTopForToolbar(overlaps);
  }, []);

  const handleToolbarMouseOut = useCallback((e: React.MouseEvent) => {
    const toolbar = (e.target as HTMLElement).closest('[data-action-toolbar]');
    if (!toolbar) return;
    const related = e.relatedTarget as HTMLElement | null;
    if (related && toolbar.contains(related)) return;
    setHideScrollTopForToolbar(false);
  }, []);

  // 监听滚动位置，控制回到顶部按钮显示（节流）
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

  // 回到顶部（使用 Virtuoso scrollToIndex API）
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

  const focusSearchInput = useCallback(() => {
    const target = searchInputRef.current;
    if (!target) return;
    if (document.activeElement === target) return;
    if (focusSearchInFlightRef.current) return;

    const applyFocus = () => {
      const input = searchInputRef.current;
      if (!input) return;
      input.focus();
    };

    const task = (async () => {
      // 非前台窗口（后端钩子路径）下，先抢回窗口焦点再聚焦输入框
      if (!document.hasFocus()) {
        await focusWindowImmediately();
      }
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => {
          applyFocus();
          resolve();
        });
      });
    })()
      .catch((error) => {
        logError("Failed to focus search input:", error);
      })
      .finally(() => {
        focusSearchInFlightRef.current = null;
      });

    focusSearchInFlightRef.current = task;
  }, [searchInputRef]);

  // 键盘导航共用处理函数
  const handleNavKey = useCallback(
    (key: string, shift: boolean, source: "default" | "search-input" = "default") => {
      if (!useUISettings.getState().keyboardNavigation) return;
      if (useClipboardStore.getState().batchMode) return;

      switch (key) {
        case "ArrowLeft": {
          if (!useUISettings.getState().showCategoryFilter) break;
          if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
          const { selectedCategory: sc, setSelectedCategory: ssc } = useClipboardStore.getState();
          const cats = getVisibleCategories(useUISettings.getState().enabledMonitorTypes);
          const curIdx = cats.findIndex((g) => g.value === sc);
          if (curIdx > 0) ssc(cats[curIdx - 1].value);
          break;
        }
        case "ArrowRight": {
          if (!useUISettings.getState().showCategoryFilter) break;
          if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
          const { selectedCategory: sc, setSelectedCategory: ssc } = useClipboardStore.getState();
          const cats = getVisibleCategories(useUISettings.getState().enabledMonitorTypes);
          const curIdx = cats.findIndex((g) => g.value === sc);
          if (curIdx < cats.length - 1) ssc(cats[curIdx + 1].value);
          break;
        }
        case "ArrowUp": {
          const { items: upItems, activeIndex: cur } = useClipboardStore.getState();
          if (upItems.length === 0) return;
          if (cur === 0) {
            // 顶部再上移：退出列表高亮并回到搜索框
            setActiveIndex(-1);
            focusSearchInput();
            break;
          }
          // 搜索输入框内，且当前已无高亮时，ArrowUp 不再进入列表，避免与“回到搜索”形成抖动循环
          if (cur === -1 && source === "search-input") {
            break;
          }
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
    [setActiveIndex, pasteContent, pasteAsPlainText, deleteItem, focusSearchInput],
  );

  // DOM keydown（窗口聚焦时）
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.isComposing) return;

      const target = e.target;
      const el = target instanceof HTMLElement ? target : null;
      const isEditable =
        el instanceof HTMLInputElement ||
        el instanceof HTMLTextAreaElement ||
        el?.isContentEditable;
      const isSearchInput =
        el instanceof HTMLInputElement &&
        el === searchInputRef.current;

      // 普通输入控件保持原生键盘行为
      if (isEditable && !isSearchInput) return;

      // 搜索输入框仅透传上下导航，避免破坏左右移动光标/删除/回车输入语义
      const navKeys = isSearchInput
        ? ["ArrowUp", "ArrowDown"]
        : ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Enter", "Delete"];

      if (navKeys.includes(e.key)) {
        e.preventDefault();
        handleNavKey(e.key, e.shiftKey, isSearchInput ? "search-input" : "default");
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleNavKey, searchInputRef]);

  // Tauri 键盘钩子事件（窗口无需聚焦，聚焦时跳过避免重复）
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
    () => renderedItems.map((i) => i._sortId),
    [renderedItems],
  );

  const itemContent = useCallback(
    (index: number) => {
      const item = renderedItems[index];
      if (!item) return null;

      const showSeparator = index === pinnedCount && pinnedCount > 0;

      const densityPb = cardDensity === "compact" ? "pb-1" : cardDensity === "spacious" ? "pb-3" : "pb-2";
      return (
        <div className={`px-2 ${densityPb}${index === 0 ? ' pt-1.5' : ''}`}>
          {showSeparator && <Separator className="mb-2" />}
          <ClipboardItemCard item={item} index={index} showBadge={showSlotBadges} sortId={item._sortId} />
        </div>
      );
    },
    [renderedItems, pinnedCount, showSlotBadges, cardDensity],
  );

  const computeItemKey = useCallback(
    (index: number) => renderedItems[index]?._sortId || `item-${index}`,
    [renderedItems],
  );

  if (isLoading && renderedItems.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center h-full">
        <div className="text-center space-y-3">
          <div className="w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin mx-auto" />
          <p className="text-sm text-muted-foreground">加载中...</p>
        </div>
      </div>
    );
  }

  // 搜索无结果
  if (renderedItems.length === 0 && searchQuery) {
    return (
      <div className="flex-1 flex items-center justify-center h-full">
        <div className="text-center space-y-4">
          <div className="w-16 h-16 rounded-full bg-muted flex items-center justify-center mx-auto">
            <Search16Regular className="w-8 h-8 text-muted-foreground" />
          </div>
          <div className="space-y-1">
            <p className="text-sm font-medium">未找到匹配的内容</p>
            <p className="text-sm text-muted-foreground">试试其他关键词</p>
          </div>
          <button
            onClick={() => useClipboardStore.getState().resetView()}
            className="text-xs text-primary hover:text-primary/80 hover:underline transition-colors"
          >
            清除筛选
          </button>
        </div>
      </div>
    );
  }

  if (renderedItems.length === 0) {
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
      <div ref={listContainerRef} onMouseOver={handleToolbarMouseOver} onMouseOut={handleToolbarMouseOut} className="h-full relative" style={{ maskImage: 'linear-gradient(to bottom, transparent, rgba(0,0,0,0.4) 2px, rgba(0,0,0,0.8) 4px, black 6px, black calc(100% - 10px), rgba(0,0,0,0.7) calc(100% - 6px), rgba(0,0,0,0.3) calc(100% - 3px), transparent)', WebkitMaskImage: 'linear-gradient(to bottom, transparent, rgba(0,0,0,0.4) 2px, rgba(0,0,0,0.8) 4px, black 6px, black calc(100% - 10px), rgba(0,0,0,0.7) calc(100% - 6px), rgba(0,0,0,0.3) calc(100% - 3px), transparent)' }}>
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
                totalCount={renderedItems.length}
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
        <ScrollToTopButton visible={showScrollTop} forceHide={hideScrollTopForToolbar} onScrollToTop={() => scrollToTop(true)} />
      </div>

      <DragOverlay
        dropAnimation={{
          duration: 180,
          easing: "ease-out",
          // 拖放时保持卡片尺寸不变（仅平移，不缩放）
          keyframes: ({ transform }) => [
            {
              transform: CSS.Transform.toString({
                ...transform.initial,
                scaleX: 1,
                scaleY: 1,
              }),
            },
            {
              transform: CSS.Transform.toString({
                ...transform.final,
                scaleX: 1,
                scaleY: 1,
              }),
            },
          ],
        }}
        style={{ cursor: "grabbing" }}
      >
        {activeItemData && (
          <div className="shadow-xl">
            <ClipboardItemCard
              item={activeItemData}
              index={-1}
              isDragOverlay={true}
            />
          </div>
        )}
      </DragOverlay>
    </DndContext>
  );
}
