import { useCallback, useRef, useMemo, type RefObject } from "react";
import type { SortingStrategy } from "@dnd-kit/sortable";
import { OverlayScrollbarsComponent } from "overlayscrollbars-react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { ClipboardItemCard } from "@/components/ClipboardItemCard";
import { ScrollToTopButton } from "@/components/ScrollToTopButton";
import { Separator } from "@/components/ui/separator";
import type { ClipboardItem } from "@/stores/clipboard";
import type { OverlayScrollbars } from "overlayscrollbars";

export interface SortableClipboardItem extends ClipboardItem {
  _sortId: string;
}

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

const OVERLAY_SCROLLBARS_OPTIONS = {
  scrollbars: {
    theme: "os-theme-custom" as const,
    visibility: "auto" as const,
    autoHide: "scroll" as const,
    autoHideDelay: 1000,
  },
  overflow: {
    x: "hidden" as const,
    y: "scroll" as const,
  },
};

const VIRTUOSO_VIEWPORT_INCREASE = { top: 400, bottom: 400 };

const VIRTUOSO_SCROLL_SEEK_CONFIG = {
  enter: (velocity: number) => Math.abs(velocity) > 2000,
  exit: (velocity: number) => Math.abs(velocity) < 500,
};

const VIRTUOSO_COMPONENTS = { ScrollSeekPlaceholder };

const LIST_CONTAINER_MASK_STYLE: React.CSSProperties = {
  maskImage: 'linear-gradient(to bottom, transparent, rgba(0,0,0,0.4) 2px, rgba(0,0,0,0.8) 4px, black 6px, black calc(100% - 10px), rgba(0,0,0,0.7) calc(100% - 6px), rgba(0,0,0,0.3) calc(100% - 3px), transparent)',
  WebkitMaskImage: 'linear-gradient(to bottom, transparent, rgba(0,0,0,0.4) 2px, rgba(0,0,0,0.8) 4px, black 6px, black calc(100% - 10px), rgba(0,0,0,0.7) calc(100% - 6px), rgba(0,0,0,0.3) calc(100% - 3px), transparent)',
};

interface ClipboardVirtualizedListProps {
  renderedItems: SortableClipboardItem[];
  pinnedCount: number;
  cardMaxLines: number;
  cardDensity: string;
  showSlotBadges: boolean;
  sortableIds: string[];
  strategy: SortingStrategy;
  customScrollParent: HTMLElement | null;
  listContainerRef: RefObject<HTMLDivElement | null>;
  virtuosoRef: RefObject<VirtuosoHandle | null>;
  scrollerRef: RefObject<HTMLElement | null>;
  onCustomScrollParentChange: (element: HTMLElement | null) => void;
  onOverlayScrollbarsChange: (instance: OverlayScrollbars) => void;
  onToolbarMouseOver: (event: React.MouseEvent<HTMLDivElement>) => void;
  onToolbarMouseOut: (event: React.MouseEvent<HTMLDivElement>) => void;
  showScrollTop: boolean;
  hideScrollTopForToolbar: boolean;
  onScrollToTop: () => void;
  SortableContext: React.ComponentType<{ items: string[]; strategy: SortingStrategy; children: React.ReactNode }>;
}

export function ClipboardVirtualizedList({
  renderedItems,
  pinnedCount,
  cardMaxLines,
  cardDensity,
  showSlotBadges,
  sortableIds,
  strategy,
  customScrollParent,
  listContainerRef,
  virtuosoRef,
  scrollerRef,
  onCustomScrollParentChange,
  onOverlayScrollbarsChange,
  onToolbarMouseOver,
  onToolbarMouseOut,
  showScrollTop,
  hideScrollTopForToolbar,
  onScrollToTop,
  SortableContext,
}: ClipboardVirtualizedListProps) {
  const defaultItemHeight = useMemo(
    () => 20 + cardMaxLines * 20 + 20 + 8,
    [cardMaxLines],
  );

  // 使用 ref 追踪最新的 props，避免 itemContent 回调因 renderedItems 变化而重建
  // Virtuoso 内部会在 totalCount 变化时重新调用 itemContent，无需依赖数组触发
  const renderedItemsRef = useRef(renderedItems);
  renderedItemsRef.current = renderedItems;
  const pinnedCountRef = useRef(pinnedCount);
  pinnedCountRef.current = pinnedCount;
  const showSlotBadgesRef = useRef(showSlotBadges);
  showSlotBadgesRef.current = showSlotBadges;
  const cardDensityRef = useRef(cardDensity);
  cardDensityRef.current = cardDensity;

  const itemContent = useCallback(
    (index: number) => {
      const item = renderedItemsRef.current[index];
      if (!item) return null;

      const pc = pinnedCountRef.current;
      const showSeparator = index === pc && pc > 0;
      const density = cardDensityRef.current;
      const densityPb = density === "compact" ? "pb-1" : density === "spacious" ? "pb-3" : "pb-2";

      return (
        <div className={`px-2 ${densityPb}${index === 0 ? ' pt-1.5' : ''}`}>
          {showSeparator && <Separator className="mb-2" />}
          <ClipboardItemCard item={item} index={index} showBadge={showSlotBadgesRef.current} sortId={item._sortId} />
        </div>
      );
    },
    [], // 稳定回调：通过 ref 读取最新值，Virtuoso 通过 totalCount 变化触发重渲染
  );

  const computeItemKey = useCallback(
    (index: number) => renderedItemsRef.current[index]?._sortId || `item-${index}`,
    [],
  );

  return (
    <div ref={listContainerRef} onMouseOver={onToolbarMouseOver} onMouseOut={onToolbarMouseOut} className="h-full relative" style={LIST_CONTAINER_MASK_STYLE}>
      <OverlayScrollbarsComponent
        element="div"
        options={OVERLAY_SCROLLBARS_OPTIONS}
        events={{
          initialized: (instance: OverlayScrollbars) => {
            onOverlayScrollbarsChange(instance);
            const viewport = instance.elements().viewport;
            onCustomScrollParentChange(viewport);
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
              increaseViewportBy={VIRTUOSO_VIEWPORT_INCREASE}
              scrollSeekConfiguration={VIRTUOSO_SCROLL_SEEK_CONFIG}
              components={VIRTUOSO_COMPONENTS}
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
      <ScrollToTopButton visible={showScrollTop} forceHide={hideScrollTopForToolbar} onScrollToTop={onScrollToTop} />
    </div>
  );
}