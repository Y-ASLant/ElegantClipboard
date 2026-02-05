import { useEffect, useRef, useCallback, useMemo } from "react";
import { ClipboardMultiple16Regular } from "@fluentui/react-icons";
import { Virtuoso } from "react-virtuoso";
import { Separator } from "@/components/ui/separator";
import { useSortableList } from "@/hooks/useSortableList";
import { useClipboardStore, ClipboardItem } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";
import { ClipboardItemCard } from "./ClipboardItemCard";

interface SortableClipboardItem extends ClipboardItem {
  _sortId: string;
}

export function ClipboardList() {
  const listenerRef = useRef<(() => void) | null>(null);
  const { items, pinnedItems, isLoading, fetchItems, fetchPinnedItems, setupListener, moveItem, togglePin, checkFileValidity } =
    useClipboardStore();
  const { cardMaxLines } = useUISettings();

  useEffect(() => {
    // Fetch items then check file validity
    Promise.all([fetchItems(), fetchPinnedItems()]).then(() => {
      checkFileValidity();
    });
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

  const itemsWithSortId = useMemo((): SortableClipboardItem[] =>
    items.map((item) => ({ ...item, _sortId: `item-${item.id}` })),
  [items]);

  const pinnedItemsWithSortId = useMemo(
    () => itemsWithSortId.filter((item) => item.is_pinned),
    [itemsWithSortId]
  );

  const regularItemsWithSortId = useMemo(
    () => itemsWithSortId.filter((item) => !item.is_pinned),
    [itemsWithSortId]
  );

  // 合并所有卡片：置顶在前，非置顶在后
  const allItemsWithSortId = useMemo(
    () => [...pinnedItemsWithSortId, ...regularItemsWithSortId],
    [pinnedItemsWithSortId, regularItemsWithSortId]
  );

  const handleDragEnd = useCallback(
    async (oldIndex: number, newIndex: number) => {
      if (oldIndex === newIndex) return;
      const fromItem = allItemsWithSortId[oldIndex];
      const toItem = allItemsWithSortId[newIndex];
      if (!fromItem || !toItem) return;

      const pinnedCount = pinnedItemsWithSortId.length;
      const fromIsPinned = oldIndex < pinnedCount;
      const toIsPinned = newIndex < pinnedCount;

      // 跨区域拖拽：自动改变置顶状态
      if (fromIsPinned !== toIsPinned) {
        // 非置顶拖入置顶区域 -> 标记为置顶
        // 置顶拖入非置顶区域 -> 取消置顶
        await togglePin(fromItem.id);
      }
      
      // 同区域拖拽：移动位置
      if (fromIsPinned === toIsPinned) {
        await moveItem(fromItem.id, toItem.id);
      }
    },
    [allItemsWithSortId, pinnedItemsWithSortId.length, moveItem, togglePin]
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
    items: allItemsWithSortId,
    onDragEnd: handleDragEnd,
  });

  // 拖拽时接管滚轮事件 - QuickClipboard 优化
  useEffect(() => {
    if (!activeId) return;
    const handleWheel = (e: WheelEvent) => {
      e.preventDefault();
      const scrollerElement = document.querySelector('[data-virtuoso-scroller="true"]') as HTMLElement;
      if (scrollerElement) scrollerElement.scrollTop += e.deltaY;
    };
    document.addEventListener('wheel', handleWheel, { passive: false } as AddEventListenerOptions);
    return () => document.removeEventListener('wheel', handleWheel);
  }, [activeId]);

  // 拖拽时添加全局光标样式
  useEffect(() => {
    if (!activeId) return;
    document.body.classList.add('dragging-cursor');
    return () => document.body.classList.remove('dragging-cursor');
  }, [activeId]);

  const defaultItemHeight = useMemo(() =>
    20 + cardMaxLines * 20 + 20 + 8,
  [cardMaxLines]);

  const pinnedCount = pinnedItemsWithSortId.length;

  const itemContent = useCallback(
    (index: number) => {
      const item = allItemsWithSortId[index];
      if (!item) return null;
      
      // 计算显示序号：置顶区域从0开始，非置顶区域也从0开始
      const displayIndex = item.is_pinned ? index : index - pinnedCount;
      
      // 在置顶区域和非置顶区域之间添加分隔线
      const showSeparator = index === pinnedCount && pinnedCount > 0;
      
      return (
        <div className="px-2 pb-2">
          {showSeparator && <Separator className="mb-2" />}
          <ClipboardItemCard item={item} index={displayIndex} sortId={item._sortId} />
        </div>
      );
    },
    [allItemsWithSortId, pinnedCount]
  );

  const computeItemKey = useCallback(
    (index: number) => allItemsWithSortId[index]?._sortId || `item-${index}`,
    [allItemsWithSortId]
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

  if (items.length === 0 && pinnedItems.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center h-full">
        <div className="text-center space-y-4">
          <div className="w-16 h-16 rounded-full bg-muted flex items-center justify-center mx-auto">
            <ClipboardMultiple16Regular className="w-8 h-8 text-muted-foreground" />
          </div>
          <div className="space-y-1">
            <p className="text-sm font-medium">暂无剪贴板历史</p>
            <p className="text-sm text-muted-foreground">复制任意内容开始记录</p>
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
      <div className="h-full overflow-hidden">
        <SortableContext items={allItemsWithSortId.map((i) => i._sortId)} strategy={strategy}>
          <Virtuoso
            totalCount={allItemsWithSortId.length}
            itemContent={itemContent}
            computeItemKey={computeItemKey}
            defaultItemHeight={defaultItemHeight}
            increaseViewportBy={{ top: 400, bottom: 400 }}
            className="custom-scrollbar"
          />
        </SortableContext>
      </div>

      <DragOverlay dropAnimation={null} style={{ cursor: "grabbing" }}>
        {activeItemData && (
          <ClipboardItemCard item={activeItemData} index={-1} isDragOverlay={true} />
        )}
      </DragOverlay>
    </DndContext>
  );
}
