import { useState, useRef } from "react";
import { ArrowUp16Regular } from "@fluentui/react-icons";
import { logError } from "@/lib/logger";
import { cn } from "@/lib/utils";

const STORAGE_KEY = "scroll-to-top-pos";
const MARGIN = 12;

type Side = "left" | "right";
interface AnchoredPos { side: Side; bottom: number }

function loadAnchor(): AnchoredPos {
  try {
    const p = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "");
    if ((p.side === "left" || p.side === "right") && typeof p.bottom === "number") return p;
  } catch (error) {
    logError("Failed to parse scroll-to-top anchor, reset to default:", error);
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch (removeError) {
      logError("Failed to clear invalid scroll-to-top anchor:", removeError);
    }
  }
  return { side: "right", bottom: MARGIN };
}

function saveAnchor(a: AnchoredPos) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(a));
}

interface ScrollToTopButtonProps {
  visible: boolean;
  onScrollToTop: () => void;
}

export function ScrollToTopButton({ visible, onScrollToTop }: ScrollToTopButtonProps) {
  const [anchor, setAnchor] = useState<AnchoredPos>(loadAnchor);
  // dragOffset: 拖拽中临时的 right 值，null 表示未在拖拽
  const [dragOffset, setDragOffset] = useState<{ right: number; bottom: number } | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x0: number; y0: number; r0: number; b0: number; moved: boolean } | null>(null);
  const [snapping, setSnapping] = useState(false);

  // 根据 side 计算 right 值
  const getRightForSide = (side: Side): number => {
    if (side === "right") return MARGIN;
    if (!panelRef.current?.parentElement) return MARGIN;
    const pw = panelRef.current.parentElement.clientWidth;
    const sw = panelRef.current.offsetWidth;
    return pw - sw - MARGIN;
  };

  const onPointerDown = (e: React.PointerEvent) => {
    e.preventDefault();
    const r = getRightForSide(anchor.side);
    drag.current = { x0: e.clientX, y0: e.clientY, r0: r, b0: anchor.bottom, moved: false };
    setDragOffset({ right: r, bottom: anchor.bottom });
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d || !panelRef.current?.parentElement) return;
    const dx = e.clientX - d.x0, dy = e.clientY - d.y0;
    if (!d.moved && Math.abs(dx) < 3 && Math.abs(dy) < 3) return;
    d.moved = true;
    setSnapping(false);
    const pw = panelRef.current.parentElement.clientWidth;
    const sw = panelRef.current.offsetWidth;
    const ph = panelRef.current.parentElement.clientHeight;
    const sh = panelRef.current.offsetHeight;
    const right = Math.max(MARGIN, Math.min(d.r0 - dx, pw - sw - MARGIN));
    const bottom = Math.max(MARGIN, Math.min(d.b0 - dy, ph - sh - MARGIN));
    setDragOffset({ right, bottom });
  };

  const onPointerUp = (e: React.PointerEvent) => {
    if (!drag.current) return;
    (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    if (drag.current.moved && dragOffset && panelRef.current?.parentElement) {
      const pw = panelRef.current.parentElement.clientWidth;
      const sw = panelRef.current.offsetWidth;
      // 判断吸附方向
      const side: Side = dragOffset.right > (pw - sw) / 2 ? "left" : "right";
      const newAnchor: AnchoredPos = { side, bottom: dragOffset.bottom };
      setSnapping(true);
      setAnchor(newAnchor);
      saveAnchor(newAnchor);
    }
    drag.current = null;
    // 吸附动画结束后清除拖拽偏移
    setTimeout(() => { setDragOffset(null); setSnapping(false); }, 300);
  };

  // 最终渲染位置
  const style = dragOffset
    ? { right: snapping ? getRightForSide(anchor.side) : dragOffset.right, bottom: dragOffset.bottom }
    : { right: getRightForSide(anchor.side), bottom: anchor.bottom };

  return (
    <div
      ref={panelRef}
      style={style}
      className={cn(
        "absolute z-10",
        snapping ? "transition-all duration-300 ease-out" : "transition-opacity duration-200",
        visible
          ? "opacity-100 pointer-events-auto"
          : "opacity-0 pointer-events-none",
      )}
    >
      <div className="flex flex-col items-center gap-0.5 rounded-lg border bg-background/95 backdrop-blur-sm shadow-md p-1">
        {/* 回到顶部 */}
        <button
          onClick={onScrollToTop}
          className="w-7 h-7 rounded-md flex items-center justify-center text-primary hover:bg-accent transition-colors"

        >
          <ArrowUp16Regular className="w-4 h-4" />
        </button>

        <div className="w-5 h-px bg-border" />

        {/* 拖拽手柄 (2x2 点阵) */}
        <div
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          className="w-7 h-5 rounded-md flex items-center justify-center cursor-grab active:cursor-grabbing text-muted-foreground/50 hover:text-muted-foreground hover:bg-accent transition-colors touch-none"

        >
          <svg width="10" height="9" viewBox="0 0 10 9" fill="currentColor">
            <circle cx="2.5" cy="2" r="1.2" />
            <circle cx="7.5" cy="2" r="1.2" />
            <circle cx="2.5" cy="7" r="1.2" />
            <circle cx="7.5" cy="7" r="1.2" />
          </svg>
        </div>
      </div>
    </div>
  );
}
