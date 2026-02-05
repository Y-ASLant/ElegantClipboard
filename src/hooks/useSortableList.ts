import { useState, useCallback, useRef } from "react";
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  MouseSensor,
  useSensor,
  useSensors,
  DragOverlay,
  DragStartEvent,
  DragEndEvent,
  CollisionDetection,
  MeasuringConfiguration,
  MeasuringStrategy,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { restrictToVerticalAxis } from "@dnd-kit/modifiers";

export interface SortableItem {
  id: number;
  _sortId: string;
  is_pinned: boolean;
}

interface UseSortableListOptions<T extends SortableItem> {
  items: T[];
  onDragEnd: (oldIndex: number, newIndex: number) => void;
}

// Check if element or parents have data-drag-ignore attribute
function shouldHandleDrag(element: EventTarget | null): boolean {
  let cur = element as HTMLElement | null;
  while (cur) {
    if (cur.dataset && cur.dataset.dragIgnore === "true") {
      return false;
    }
    cur = cur.parentElement;
  }
  return true;
}

// Custom MouseSensor that ignores buttons and marked elements
class CustomMouseSensor extends MouseSensor {
  static activators = [
    {
      eventName: "onMouseDown" as const,
      handler: ({ nativeEvent: event }: { nativeEvent: MouseEvent }) => {
        return shouldHandleDrag(event.target);
      },
    },
  ];
}

// Optimized measuring configuration for better performance
const measuringConfig: MeasuringConfiguration = {
  droppable: {
    strategy: MeasuringStrategy.Always,
  },
};

export function useSortableList<T extends SortableItem>({
  items,
  onDragEnd,
}: UseSortableListOptions<T>) {
  const [activeId, setActiveId] = useState<string | null>(null);
  const itemsRef = useRef(items);

  // Keep itemsRef updated without triggering re-renders
  if (itemsRef.current !== items) {
    itemsRef.current = items;
  }

  const sensors = useSensors(
    useSensor(CustomMouseSensor, {
      activationConstraint: {
        distance: 3, // Match QuickClipboard for better responsiveness
      },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  // Optimized collision detection with early exit and memoization
  const customCollisionDetection: CollisionDetection = useCallback(
    (args) => {
      const { active } = args;
      const activeIdStr = String(active.id);

      // Use ref to avoid recreating when items change
      const currentItems = itemsRef.current;

      // Find active item once
      const activeItem = currentItems.find(
        (item) => item._sortId === activeIdStr || String(item.id) === activeIdStr
      );

      // Get all collisions first
      const collisions = closestCenter(args);
      if (!collisions.length || !activeItem) return collisions;

      // Filter out items with different pinned status
      return collisions.filter((collision) => {
        const collisionIdStr = String(collision.id);
        const overItem = currentItems.find(
          (item) => item._sortId === collisionIdStr || String(item.id) === collisionIdStr
        );
        if (!overItem) return true;
        return activeItem.is_pinned === overItem.is_pinned;
      });
    },
    [] // No dependencies - uses ref instead
  );

  const handleDragStart = useCallback((event: DragStartEvent) => {
    setActiveId(event.active.id as string);
  }, []);

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event;
      setActiveId(null);

      if (over && active.id !== over.id) {
        // Use ref for items to avoid stale closure
        const currentItems = itemsRef.current;
        const oldIndex = currentItems.findIndex((item) => item._sortId === active.id);
        const newIndex = currentItems.findIndex((item) => item._sortId === over.id);

        if (oldIndex !== -1 && newIndex !== -1) {
          onDragEnd(oldIndex, newIndex);
        }
      }
    },
    [onDragEnd]
  );

  const handleDragCancel = useCallback(() => {
    setActiveId(null);
  }, []);

  // Get currently dragged item
  const activeItem = activeId
    ? itemsRef.current.find(
        (item) => item._sortId === activeId || String(item.id) === activeId
      )
    : null;

  return {
    DndContext,
    SortableContext,
    DragOverlay,
    sensors,
    handleDragStart,
    handleDragEnd,
    handleDragCancel,
    activeId,
    activeItem,
    strategy: verticalListSortingStrategy,
    modifiers: [restrictToVerticalAxis],
    collisionDetection: customCollisionDetection,
    measuring: measuringConfig,
  };
}

export { useSortable } from "@dnd-kit/sortable";
export { CSS } from "@dnd-kit/utilities";
