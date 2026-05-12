import { useEffect, useRef, useCallback, useMemo, useState, type RefObject } from "react";
import { CSS } from "@dnd-kit/utilities";
import { useShallow } from "zustand/react/shallow";
import { ClipboardListEmptyStates } from "@/components/ClipboardList/EmptyStates";
import { ClipboardVirtualizedList, type SortableClipboardItem } from "@/components/ClipboardList/VirtualizedList";
import { useClipboardKeyNav } from "@/hooks/useClipboardKeyNav";
import { useOptimisticReorder } from "@/hooks/useOptimisticReorder";
import { useScrollToTopState } from "@/hooks/useScrollToTopState";
import { useSortableList } from "@/hooks/useSortableList";
import { useClipboardStore } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";
import { ClipboardItemCard } from "./ClipboardItemCard";
import type { OverlayScrollbars } from "overlayscrollbars";
import type { VirtuosoHandle } from "react-virtuoso";

interface ClipboardListProps {
  searchInputRef: RefObject<HTMLInputElement | null>;
}

export function ClipboardList({ searchInputRef }: ClipboardListProps) {
  const listenerRef = useRef<(() => void) | null>(null);
  const scrollerRef = useRef<HTMLElement | null>(null);
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const osInstanceRef = useRef<OverlayScrollbars | null>(null);
  const [customScrollParent, setCustomScrollParent] =
    useState<HTMLElement | null>(null);
  const listContainerRef = useRef<HTMLDivElement>(null);
  const {
    items,
    isLoading,
    searchQuery,
    selectedCategory,
    fetchItems,
    setupListener,
    moveItem,
    togglePin,
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

  const { renderedItems, pinnedCount, handleDragEnd } = useOptimisticReorder({
    items: itemsWithSortId,
    moveItem,
    togglePin,
  });

  // 搜索/类型筛选时隐藏快捷粘贴序号（过滤后的顺序与快捷粘贴槽位顺序不一致）
  const showSlotBadges = !searchQuery && !selectedCategory;

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

  const {
    showScrollTop,
    hideScrollTopForToolbar,
    handleToolbarMouseOver,
    handleToolbarMouseOut,
  } = useScrollToTopState({
    customScrollParent,
    listContainerRef,
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

  useClipboardKeyNav({
    searchInputRef,
    virtuosoRef,
  });

  // 拖拽时添加全局光标样式
  useEffect(() => {
    if (!activeId) return;
    document.body.classList.add("dragging-cursor");
    return () => document.body.classList.remove("dragging-cursor");
  }, [activeId]);

  const sortableIds = useMemo(
    () => renderedItems.map((i) => i._sortId),
    [renderedItems],
  );

  if (renderedItems.length === 0) {
    return (
      <ClipboardListEmptyStates
        isLoading={isLoading}
        hasItems={false}
        searchQuery={searchQuery}
        onResetView={() => useClipboardStore.getState().resetView()}
      />
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
      <ClipboardVirtualizedList
        renderedItems={renderedItems}
        pinnedCount={pinnedCount}
        cardMaxLines={cardMaxLines}
        cardDensity={cardDensity}
        showSlotBadges={showSlotBadges}
        sortableIds={sortableIds}
        strategy={strategy}
        customScrollParent={customScrollParent}
        listContainerRef={listContainerRef}
        virtuosoRef={virtuosoRef}
        scrollerRef={scrollerRef}
        onCustomScrollParentChange={setCustomScrollParent}
        onOverlayScrollbarsChange={(instance) => {
          osInstanceRef.current = instance;
        }}
        onToolbarMouseOver={handleToolbarMouseOver}
        onToolbarMouseOut={handleToolbarMouseOut}
        showScrollTop={showScrollTop}
        hideScrollTopForToolbar={hideScrollTopForToolbar}
        onScrollToTop={() => scrollToTop(true)}
        SortableContext={SortableContext}
      />

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
