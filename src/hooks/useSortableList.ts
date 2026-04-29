import { useState, useCallback, useRef, useEffect } from "react";
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

export interface SortableItem {
  id: number;
  _sortId: string;
  is_pinned: boolean;
}

interface UseSortableListOptions<T extends SortableItem> {
  items: T[];
  onDragEnd: (oldIndex: number, newIndex: number) => void;
}

// 仅从拖拽手柄启动拖拽，确保点击/粘贴路径确定
function shouldHandleDrag(element: EventTarget | null): boolean {
  let cur = element as HTMLElement | null;
  let hasDragHandle = false;
  while (cur) {
    if (cur.dataset?.dragHandle === "true") {
      hasDragHandle = true;
    }
    // 忽略标记了 drag-ignore 或 no-drag 的元素
    if (cur.dataset && (cur.dataset.dragIgnore === "true" || cur.dataset.noDrag === "true")) {
      return false;
    }
    // 忽略滚动条元素
    if (cur.classList && (
      cur.classList.contains('os-scrollbar') ||
      cur.classList.contains('os-scrollbar-track') ||
      cur.classList.contains('os-scrollbar-handle')
    )) {
      return false;
    }
    cur = cur.parentElement;
  }
  return hasDragHandle;
}

// 自定义鼠标传感器：仅左键点击拖拽手柄时激活
class CustomMouseSensor extends MouseSensor {
  static activators = [
    {
      eventName: "onMouseDown" as const,
      handler: ({ nativeEvent: event }: { nativeEvent: MouseEvent }) => {
        if (event.button !== 0) {
          return false;
        }
        return shouldHandleDrag(event.target);
      },
    },
  ];
}

// 优化的测量配置
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

  // 保持 itemsRef 同步（通过 effect 避免渲染期写 ref）
  useEffect(() => {
    itemsRef.current = items;
  }, [items]);

  const sensors = useSensors(
    useSensor(CustomMouseSensor, {
      activationConstraint: {
        distance: 1, // 最小距离，立即响应拖拽
      },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  // 碰撞检测：允许跨区域拖拽（置顶 ↔ 普通）
  const customCollisionDetection: CollisionDetection = useCallback(
    (args) => {
      // 使用 closestCenter，不按置顶状态过滤
      return closestCenter(args);
    },
    []
  );

  const handleDragStart = useCallback((event: DragStartEvent) => {
    setActiveId(event.active.id as string);
  }, []);

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event;
      setActiveId(null);

      if (over && active.id !== over.id) {
        // 用 ref 避免闭包过期
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

  // 获取当前拖拽项
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
    modifiers: [],
    collisionDetection: customCollisionDetection,
    measuring: measuringConfig,
  };
}

export { useSortable } from "@dnd-kit/sortable";
export { CSS } from "@dnd-kit/utilities";
