import { useEffect, useRef, useCallback } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useClipboardStore } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";
import { ClipboardItemCard } from "./ClipboardItemCard";
import { Separator } from "@/components/ui/separator";
import { ClipboardMultiple16Regular, Pin16Regular } from "@fluentui/react-icons";

export function ClipboardList() {
  const parentRef = useRef<HTMLDivElement>(null);
  const { items, pinnedItems, isLoading, fetchItems, fetchPinnedItems, setupListener } =
    useClipboardStore();
  const { cardMaxLines } = useUISettings();

  // Initial data fetch
  useEffect(() => {
    fetchItems();
    fetchPinnedItems();
    
    let mounted = true;
    let cleanup: (() => void) | undefined;
    
    setupListener().then((unlisten) => {
      if (mounted) {
        cleanup = unlisten;
      } else {
        unlisten();
      }
    });
    
    return () => {
      mounted = false;
      if (cleanup) cleanup();
    };
  }, [fetchItems, fetchPinnedItems, setupListener]);

  // Filter out pinned items from regular list
  const regularItems = items.filter((item) => !item.is_pinned);

  // Estimate item height based on cardMaxLines setting
  const estimateSize = useCallback(() => {
    return 20 + cardMaxLines * 20 + 20 + 8;
  }, [cardMaxLines]);

  // Virtual list for history items with proper key tracking
  const virtualizer = useVirtualizer({
    count: regularItems.length,
    getScrollElement: () => parentRef.current,
    estimateSize,
    overscan: 5,
    getItemKey: (index) => regularItems[index]?.id ?? index,
  });

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

  return (
    <div ref={parentRef} className="h-full overflow-y-auto overflow-x-hidden custom-scrollbar">
      <div className="p-2">
        {/* Pinned Section */}
        {pinnedItems.length > 0 && (
          <div className="mb-2">
            <div className="flex items-center gap-2 px-3 py-2">
              <Pin16Regular className="w-4 h-4 text-muted-foreground" />
              <span className="text-xs font-medium text-muted-foreground">
                已置顶 ({pinnedItems.length})
              </span>
            </div>
            <div className="space-y-2">
              {pinnedItems.map((item, idx) => (
                <ClipboardItemCard key={item.id} item={item} index={idx} />
              ))}
            </div>
            {regularItems.length > 0 && <Separator className="my-3" />}
          </div>
        )}

        {/* Virtualized History Section */}
        {regularItems.length > 0 && (
          <div
            style={{
              height: `${virtualizer.getTotalSize()}px`,
              width: "100%",
              position: "relative",
            }}
          >
            {virtualizer.getVirtualItems().map((virtualItem) => {
              const item = regularItems[virtualItem.index];
              return (
                <div
                  key={item.id}
                  data-index={virtualItem.index}
                  ref={virtualizer.measureElement}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translateY(${virtualItem.start}px)`,
                    paddingBottom: "8px",
                  }}
                >
                  <ClipboardItemCard item={item} index={virtualItem.index} />
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
