import { useEffect, useRef, useCallback, useMemo } from "react";
import { Virtuoso } from "react-virtuoso";
import { useClipboardStore, ClipboardItem } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";
import { ClipboardItemCard } from "./ClipboardItemCard";
import { Separator } from "@/components/ui/separator";
import { ClipboardMultiple16Regular } from "@fluentui/react-icons";
import { useSortableList } from "@/hooks/useSortableList";

interface SortableClipboardItem extends ClipboardItem {
  _sortId: string;
}

export function ClipboardList() {
  const listenerRef = useRef<(() => void) | null>(null);
  const { items, pinnedItems, isLoading, fetchItems, fetchPinnedItems, setupListener, moveItem } =
    useClipboardStore();
  const { cardMaxLines } = useUISettings();

  useEffect(() => {
    fetchItems();
    fetchPinnedItems();
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

  const handleDragEnd = useCallback(
    async (oldIndex: number, newIndex: number) => {
      if (oldIndex === newIndex) return;
      const allSortableItems = [...pinnedItemsWithSortId, ...regularItemsWithSortId];
      const fromItem = allSortableItems[oldIndex];
      const toItem = allSortableItems[newIndex];
      if (fromItem && toItem && fromItem.is_pinned === toItem.is_pinned) {
        await moveItem(fromItem.id, toItem.id);
      }
    },
    [pinnedItemsWithSortId, regularItemsWithSortId, moveItem]
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

  const itemContent = useCallback(
    (index: number) => {
      const item = regularItemsWithSortId[index];
      if (!item) return null;
      return (
        <div className="px-2 pb-2">
          <ClipboardItemCard item={item} index={index} sortId={item._sortId} />
        </div>
      );
    },
    [regularItemsWithSortId]
  );

  const computeItemKey = useCallback(
    (index: number) => regularItemsWithSortId[index]?._sortId || `item-${index}`,
    [regularItemsWithSortId]
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
      <div className="h-full overflow-hidden flex flex-col">
        {pinnedItemsWithSortId.length > 0 && (
          <div className="flex-none px-2 pb-0">
            <SortableContext items={pinnedItemsWithSortId.map((i) => i._sortId)} strategy={strategy}>
              <div className="space-y-2">
                {pinnedItemsWithSortId.map((item, idx) => (
                  <ClipboardItemCard key={item.id} item={item} index={idx} sortId={item._sortId} />
                ))}
              </div>
            </SortableContext>
            {regularItemsWithSortId.length > 0 && <Separator className="my-3" />}
          </div>
        )}

        {regularItemsWithSortId.length > 0 && (
          <div className="flex-1 min-h-0 overflow-x-hidden">
            <SortableContext items={regularItemsWithSortId.map((i) => i._sortId)} strategy={strategy}>
              <Virtuoso
                totalCount={regularItemsWithSortId.length}
                itemContent={itemContent}
                computeItemKey={computeItemKey}
                defaultItemHeight={defaultItemHeight}
                increaseViewportBy={{ top: 400, bottom: 400 }}
                className="custom-scrollbar"
              />
            </SortableContext>
          </div>
        )}
      </div>

      <DragOverlay dropAnimation={null} style={{ cursor: "grabbing" }}>
        {activeItemData && (
          <ClipboardItemCard item={activeItemData} index={-1} isDragOverlay={true} />
        )}
      </DragOverlay>
    </DndContext>
  );
}
