import { useCallback, useEffect, useMemo, useState } from "react";
import type { SortableClipboardItem } from "@/components/ClipboardList/VirtualizedList";

type MoveItem = (fromId: number, toId: number) => Promise<void>;
type TogglePin = (id: number) => Promise<void>;

interface UseOptimisticReorderOptions {
  items: SortableClipboardItem[];
  moveItem: MoveItem;
  togglePin: TogglePin;
}

function getPinnedBoundary(items: SortableClipboardItem[]): number {
  const firstUnpinnedIndex = items.findIndex((item) => !item.is_pinned);
  return firstUnpinnedIndex === -1 ? items.length : firstUnpinnedIndex;
}

export function useOptimisticReorder({
  items,
  moveItem,
  togglePin,
}: UseOptimisticReorderOptions) {
  const [optimisticItems, setOptimisticItems] = useState<SortableClipboardItem[] | null>(null);
  const renderedItems = optimisticItems ?? items;

  useEffect(() => {
    setOptimisticItems(null);
  }, [items]);

  const pinnedCount = useMemo(
    () => getPinnedBoundary(renderedItems),
    [renderedItems],
  );

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
      } finally {
        setOptimisticItems(null);
      }
    },
    [renderedItems, moveItem, togglePin],
  );

  return {
    renderedItems,
    pinnedCount,
    handleDragEnd,
  };
}