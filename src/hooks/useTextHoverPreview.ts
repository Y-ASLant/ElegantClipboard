import { useCallback, useEffect, useRef, type WheelEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { getPreviewBounds } from "@/components/CardContentRenderers";
import {
  type ClipboardItemDetail,
  sampleTextPreview,
  getCachedTextPreviewContent,
  setCachedTextPreviewContent,
  TEXT_PREVIEW_MIN_W,
  TEXT_PREVIEW_MAX_W,
  TEXT_PREVIEW_MIN_H,
  TEXT_PREVIEW_MAX_H,
  TEXT_PREVIEW_CHAR_WIDTH,
  TEXT_PREVIEW_HORIZONTAL_PADDING,
  TEXT_PREVIEW_MIN_CHARS_PER_LINE,
} from "@/components/text-preview";
import {
  acquireTextPreviewLease,
  revokeTextPreviewLease,
  isTextPreviewLeaseCurrent,
  isTextPreviewWanted,
  textPreviewCleanupCallbacks,
  ensureWindowHiddenListener,
} from "@/hooks/useTextPreview";
import { logError } from "@/lib/logger";
import type { ClipboardItem } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";

export function useTextHoverPreview(item: ClipboardItem) {
  const textPreviewEnabled = useUISettings((s) => s.textPreviewEnabled);
  const hoverPreviewDelay = useUISettings((s) => s.hoverPreviewDelay);
  const previewPosition = useUISettings((s) => s.previewPosition);
  const sharpCorners = useUISettings((s) => s.sharpCorners);

  const isTextLike =
    item.content_type === "text" || item.content_type === "html" || item.content_type === "rtf";

  const anchorRef = useRef<HTMLDivElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hoveringRef = useRef(false);
  const reqIdRef = useRef(0);
  const leaseRef = useRef<number | null>(null);
  const visibleRef = useRef(false);
  const scrollRafRef = useRef<number | null>(null);
  const scrollDeltaRef = useRef(0);

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const hidePreview = useCallback(() => {
    reqIdRef.current += 1;
    const closingLease = leaseRef.current;
    if (closingLease !== null) {
      revokeTextPreviewLease(closingLease);
      leaseRef.current = null;
    }
    clearTimer();
    hoveringRef.current = false;
    if (scrollRafRef.current !== null) {
      cancelAnimationFrame(scrollRafRef.current);
      scrollRafRef.current = null;
    }
    scrollDeltaRef.current = 0;
    if (closingLease !== null) {
      visibleRef.current = false;
      invoke("hide_text_preview", { token: closingLease }).catch((error) => {
        logError("Failed to hide text preview:", error);
      });
    } else if (visibleRef.current) {
      visibleRef.current = false;
      invoke("hide_text_preview").catch((error) => {
        logError("Failed to hide text preview:", error);
      });
    }
  }, [clearTimer]);

  const resolveContent = useCallback(async (): Promise<string> => {
    const inlineText = item.text_content || item.preview || "";
    if (!isTextLike) return "";
    if (item.text_content) return item.text_content;
    const cached = getCachedTextPreviewContent(item.id);
    if (cached) return cached;
    try {
      const detail = await invoke<ClipboardItemDetail | null>("get_clipboard_item", { id: item.id });
      const resolved = detail?.text_content || detail?.preview || inlineText;
      if (resolved) setCachedTextPreviewContent(item.id, resolved);
      return resolved;
    } catch (error) {
      logError("Failed to load full text content for preview:", error);
      return inlineText;
    }
  }, [isTextLike, item.id, item.preview, item.text_content]);

  const showPreview = useCallback(async (reqId: number, lease: number) => {
    if (!textPreviewEnabled || !isTextLike || !anchorRef.current) return;
    if (!hoveringRef.current || reqId !== reqIdRef.current || !isTextPreviewLeaseCurrent(lease)) return;
    const textContent = await resolveContent();
    if (!textContent) return;
    if (!hoveringRef.current || reqId !== reqIdRef.current || !isTextPreviewLeaseCurrent(lease)) return;

    const bounds = await getPreviewBounds(previewPosition, anchorRef.current);
    if (!hoveringRef.current || reqId !== reqIdRef.current || !isTextPreviewLeaseCurrent(lease)) return;

    const availableCssW = Math.max(260, Math.floor(bounds.maxW / bounds.scale));
    const availableCssH = Math.max(140, Math.floor(bounds.maxH / bounds.scale));
    const sampled = sampleTextPreview(textContent);
    const desiredWidth = sampled.longestVisualCols * TEXT_PREVIEW_CHAR_WIDTH + TEXT_PREVIEW_HORIZONTAL_PADDING;
    const windowCssW = Math.min(availableCssW, Math.min(TEXT_PREVIEW_MAX_W, Math.max(TEXT_PREVIEW_MIN_W, desiredWidth)));
    const charsPerLine = Math.max(TEXT_PREVIEW_MIN_CHARS_PER_LINE, Math.floor((windowCssW - 30) / TEXT_PREVIEW_CHAR_WIDTH));
    const sampledWrappedLines = sampled.lineColumns.reduce(
      (sum, lineCols) => sum + Math.max(1, Math.ceil(lineCols / charsPerLine)), 0,
    );
    let estimatedLines = sampledWrappedLines;
    if (sampled.truncated && sampled.processedCodeUnits < textContent.length) {
      const remaining = textContent.length - sampled.processedCodeUnits;
      const linesPerCodeUnit = sampledWrappedLines / Math.max(1, sampled.processedCodeUnits);
      estimatedLines += Math.max(1, Math.ceil(remaining * linesPerCodeUnit));
    }
    const estimatedCssH = Math.min(TEXT_PREVIEW_MAX_H, Math.max(TEXT_PREVIEW_MIN_H, estimatedLines * 21 + 40));
    const windowCssH = Math.min(availableCssH, estimatedCssH);
    const winW = Math.max(1, Math.round(windowCssW * bounds.scale));
    const winH = Math.max(1, Math.round(windowCssH * bounds.scale));
    const winX = bounds.side === "left" ? bounds.anchorX - winW : bounds.anchorX;
    const centeredY = Math.round(bounds.cardCenterY - winH / 2);
    const winY = Math.max(bounds.monY, Math.min(centeredY, bounds.monBottom - winH));
    const align = bounds.side === "left" ? "right" : "left";
    const theme = document.documentElement.classList.contains("dark") ? "dark" : "light";

    try {
      invoke("hide_image_preview").catch((error) => logError("Failed to hide image preview:", error));
      const uiState = useUISettings.getState();
      await invoke("show_text_preview", {
        text: textContent,
        winX, winY, winWidth: winW, winHeight: winH,
        align, theme, sharpCorners,
        windowEffect: uiState.windowEffect,
        fontFamily: uiState.previewFont || null,
        fontSize: uiState.previewFontSize,
        token: lease,
      });
      if (!hoveringRef.current || reqId !== reqIdRef.current || !isTextPreviewLeaseCurrent(lease)) {
        visibleRef.current = false;
        if (!isTextPreviewWanted()) {
          invoke("hide_text_preview", { token: lease }).catch((error) => logError("Failed to hide text preview:", error));
        }
        return;
      }
      visibleRef.current = true;
    } catch (error) {
      visibleRef.current = false;
      logError("Failed to show text preview:", error);
    }
  }, [textPreviewEnabled, isTextLike, previewPosition, resolveContent, sharpCorners]);

  const handleMouseEnter = useCallback(() => {
    if (!textPreviewEnabled || !isTextLike) return;
    hoveringRef.current = true;
    reqIdRef.current += 1;
    const reqId = reqIdRef.current;
    const lease = acquireTextPreviewLease();
    leaseRef.current = lease;
    clearTimer();
    timerRef.current = setTimeout(() => {
      void showPreview(reqId, lease);
    }, hoverPreviewDelay);
  }, [textPreviewEnabled, isTextLike, clearTimer, showPreview, hoverPreviewDelay]);

  const handleMouseLeave = useCallback(() => {
    hidePreview();
  }, [hidePreview]);

  const handleWheel = useCallback((e: WheelEvent<HTMLDivElement>) => {
    if (!e.ctrlKey || !visibleRef.current) return;
    e.preventDefault();
    e.stopPropagation();
    scrollDeltaRef.current += e.deltaY;
    if (scrollRafRef.current === null) {
      scrollRafRef.current = requestAnimationFrame(() => {
        scrollRafRef.current = null;
        const deltaY = scrollDeltaRef.current;
        scrollDeltaRef.current = 0;
        if (deltaY === 0 || !visibleRef.current) return;
        emitTo("text-preview", "text-preview-scroll", { deltaY }).catch((error) => {
          visibleRef.current = false;
          logError("Failed to emit text preview scroll:", error);
        });
      });
    }
  }, []);

  useEffect(() => {
    return () => { hidePreview(); };
  }, [hidePreview]);

  useEffect(() => {
    if (!textPreviewEnabled || !isTextLike) hidePreview();
  }, [textPreviewEnabled, isTextLike, hidePreview]);

  useEffect(() => {
    ensureWindowHiddenListener();
    textPreviewCleanupCallbacks.add(hidePreview);
    return () => { textPreviewCleanupCallbacks.delete(hidePreview); };
  }, [hidePreview]);

  return { anchorRef, handleMouseEnter, handleMouseLeave, handleWheel };
}