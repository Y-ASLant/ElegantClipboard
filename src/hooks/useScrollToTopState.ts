import { useCallback, useEffect, useState, type MouseEvent, type RefObject } from "react";

interface UseScrollToTopStateOptions {
  customScrollParent: HTMLElement | null;
  listContainerRef: RefObject<HTMLDivElement | null>;
}

export function useScrollToTopState({
  customScrollParent,
  listContainerRef,
}: UseScrollToTopStateOptions) {
  const [showScrollTop, setShowScrollTop] = useState(false);
  const [hideScrollTopForToolbar, setHideScrollTopForToolbar] = useState(false);

  const handleToolbarMouseOver = useCallback((e: MouseEvent) => {
    const toolbar = (e.target as HTMLElement).closest('[data-action-toolbar]');
    if (!toolbar) return;
    const btn = listContainerRef.current?.querySelector('[data-scroll-top-btn]');
    if (!btn) return;
    const tRect = toolbar.getBoundingClientRect();
    const bRect = btn.getBoundingClientRect();
    const overlaps = tRect.left < bRect.right && tRect.right > bRect.left &&
                     tRect.top < bRect.bottom && tRect.bottom > bRect.top;
    setHideScrollTopForToolbar(overlaps);
  }, [listContainerRef]);

  const handleToolbarMouseOut = useCallback((e: MouseEvent) => {
    const toolbar = (e.target as HTMLElement).closest('[data-action-toolbar]');
    if (!toolbar) return;
    const related = e.relatedTarget as HTMLElement | null;
    if (related && toolbar.contains(related)) return;
    setHideScrollTopForToolbar(false);
  }, []);

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

  return {
    showScrollTop,
    hideScrollTopForToolbar,
    handleToolbarMouseOver,
    handleToolbarMouseOut,
  };
}