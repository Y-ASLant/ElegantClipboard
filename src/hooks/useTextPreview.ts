// 文本悬浮预览 hook（统一版本，支持 ClipboardItemCard 和 TagItemRows 两种用法）

import { useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { useShallow } from "zustand/react/shallow";
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
import { getPreviewBounds } from "@/hooks/useImagePreview";
import { logError } from "@/lib/logger";
import { useClipboardStore } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";

// ============ 租约管理 ============

let textPreviewLease = 0;
let textPreviewWanted = false;

export function acquireTextPreviewLease(): number {
  textPreviewLease += 1;
  textPreviewWanted = true;
  return textPreviewLease;
}

export function revokeTextPreviewLease(lease: number): void {
  if (textPreviewLease === lease) {
    textPreviewLease += 1;
    textPreviewWanted = false;
  }
}

export function isTextPreviewLeaseCurrent(lease: number): boolean {
  return textPreviewLease === lease;
}

export function isTextPreviewWanted(): boolean {
  return textPreviewWanted;
}

// ============ 全局 window-hidden 清理注册表 ============

export const textPreviewCleanupCallbacks = new Set<() => void>();
let _windowHiddenListenerInit = false;

export function ensureWindowHiddenListener() {
  if (_windowHiddenListenerInit) return;
  _windowHiddenListenerInit = true;
  listen("window-hidden", () => {
    textPreviewCleanupCallbacks.forEach((cb) => cb());
  });
}

// ============ Hook ============

interface UseTextPreviewOptions {
  itemId: number;
  textContent: string | null | undefined;
  preview: string | null | undefined;
  isTextLikeContent: boolean;
  /** 是否正在拖拽（仅卡片使用，默认 false） */
  isDragging?: boolean;
  /** 是否检查 batchMode（仅卡片使用，默认 true） */
  checkBatchMode?: boolean;
}

export function useTextPreview({
  itemId,
  textContent,
  preview,
  isTextLikeContent,
  isDragging = false,
  checkBatchMode = true,
}: UseTextPreviewOptions) {
  const {
    textPreviewEnabled, hoverPreviewDelay, previewPosition, sharpCorners,
  } = useUISettings(useShallow((s) => ({
    textPreviewEnabled: s.textPreviewEnabled,
    hoverPreviewDelay: s.hoverPreviewDelay,
    previewPosition: s.previewPosition,
    sharpCorners: s.sharpCorners,
  })));
  const batchMode = useClipboardStore((s) => checkBatchMode ? s.batchMode : false);

  const textPreviewTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const textPreviewVisibleRef = useRef(false);
  const textPreviewAnchorRef = useRef<HTMLDivElement | null>(null);
  const textPreviewHoveringRef = useRef(false);
  const textPreviewReqIdRef = useRef(0);
  const textPreviewLeaseRef = useRef<number | null>(null);
  const textScrollEmitRafRef = useRef<number | null>(null);
  const textScrollPendingDeltaRef = useRef(0);

  const clearTextPreviewTimer = useCallback(() => {
    if (textPreviewTimerRef.current) {
      clearTimeout(textPreviewTimerRef.current);
      textPreviewTimerRef.current = null;
    }
  }, []);

  const hideTextPreview = useCallback(() => {
    textPreviewReqIdRef.current += 1;
    const closingLease = textPreviewLeaseRef.current;
    if (closingLease !== null) {
      revokeTextPreviewLease(closingLease);
      textPreviewLeaseRef.current = null;
    }
    clearTextPreviewTimer();
    textPreviewHoveringRef.current = false;
    if (textScrollEmitRafRef.current !== null) {
      cancelAnimationFrame(textScrollEmitRafRef.current);
      textScrollEmitRafRef.current = null;
    }
    textScrollPendingDeltaRef.current = 0;
    if (closingLease !== null) {
      textPreviewVisibleRef.current = false;
      invoke("hide_text_preview", { token: closingLease }).catch((error) => {
        logError("Failed to hide text preview:", error);
      });
    } else if (textPreviewVisibleRef.current) {
      textPreviewVisibleRef.current = false;
      invoke("hide_text_preview").catch((error) => {
        logError("Failed to hide text preview:", error);
      });
    }
  }, [clearTextPreviewTimer]);

  const resolveTextPreviewContent = useCallback(async (): Promise<string> => {
    const inlineText = textContent || preview || "";
    if (!isTextLikeContent) return "";
    if (textContent) return textContent;
    const cached = getCachedTextPreviewContent(itemId);
    if (cached) return cached;
    try {
      const detail = await invoke<ClipboardItemDetail | null>("get_clipboard_item", { id: itemId });
      const resolved = detail?.text_content || detail?.preview || inlineText;
      if (resolved) {
        setCachedTextPreviewContent(itemId, resolved);
      }
      return resolved;
    } catch (error) {
      logError("Failed to load full text content for preview:", error);
      return inlineText;
    }
  }, [isTextLikeContent, itemId, preview, textContent]);

  const showTextPreview = useCallback(async (reqId: number, lease: number) => {
    if (!textPreviewEnabled || !isTextLikeContent || !textPreviewAnchorRef.current) {
      return;
    }
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !isTextPreviewLeaseCurrent(lease)) return;
    const textContentResolved = await resolveTextPreviewContent();
    if (!textContentResolved) return;
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !isTextPreviewLeaseCurrent(lease)) return;

    const bounds = await getPreviewBounds(previewPosition, textPreviewAnchorRef.current);
    if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !isTextPreviewLeaseCurrent(lease)) return;
    const availableCssW = Math.max(260, Math.floor(bounds.maxW / bounds.scale));
    const availableCssH = Math.max(140, Math.floor(bounds.maxH / bounds.scale));
    const sampled = sampleTextPreview(textContentResolved);
    const desiredWidth = sampled.longestVisualCols * TEXT_PREVIEW_CHAR_WIDTH + TEXT_PREVIEW_HORIZONTAL_PADDING;
    const windowCssW = Math.min(
      availableCssW,
      Math.min(TEXT_PREVIEW_MAX_W, Math.max(TEXT_PREVIEW_MIN_W, desiredWidth)),
    );
    const charsPerLine = Math.max(
      TEXT_PREVIEW_MIN_CHARS_PER_LINE,
      Math.floor((windowCssW - 30) / TEXT_PREVIEW_CHAR_WIDTH),
    );
    const sampledWrappedLines = sampled.lineColumns.reduce((sum, lineCols) => {
      return sum + Math.max(1, Math.ceil(lineCols / charsPerLine));
    }, 0);
    let estimatedLines = sampledWrappedLines;
    if (sampled.truncated && sampled.processedCodeUnits < textContentResolved.length) {
      const remaining = textContentResolved.length - sampled.processedCodeUnits;
      const linesPerCodeUnit = sampledWrappedLines / Math.max(1, sampled.processedCodeUnits);
      estimatedLines += Math.max(1, Math.ceil(remaining * linesPerCodeUnit));
    }
    const estimatedCssH = Math.min(
      TEXT_PREVIEW_MAX_H,
      Math.max(TEXT_PREVIEW_MIN_H, estimatedLines * 21 + 40),
    );
    const windowCssH = Math.min(availableCssH, estimatedCssH);
    const winW = Math.max(1, Math.round(windowCssW * bounds.scale));
    const winH = Math.max(1, Math.round(windowCssH * bounds.scale));
    const winX = bounds.side === "left" ? bounds.anchorX - winW : bounds.anchorX;
    const centeredY = Math.round(bounds.cardCenterY - winH / 2);
    const winY = Math.max(bounds.monY, Math.min(centeredY, bounds.monBottom - winH));
    const align = bounds.side === "left" ? "right" : "left";
    const theme =
      document.documentElement.classList.contains("dark") ? "dark" : "light";

    try {
      invoke("hide_image_preview").catch((error) => {
        logError("Failed to hide image preview:", error);
      });
      const uiState = useUISettings.getState();
      await invoke("show_text_preview", {
        text: textContentResolved,
        winX,
        winY,
        winWidth: winW,
        winHeight: winH,
        align,
        theme,
        sharpCorners,
        windowEffect: uiState.windowEffect,
        fontFamily: uiState.previewFont || null,
        fontSize: uiState.previewFontSize,
        token: lease,
      });
      if (!textPreviewHoveringRef.current || reqId !== textPreviewReqIdRef.current || !isTextPreviewLeaseCurrent(lease)) {
        textPreviewVisibleRef.current = false;
        if (!isTextPreviewWanted()) {
          invoke("hide_text_preview", { token: lease }).catch((error) => {
            logError("Failed to hide text preview after stale show:", error);
          });
        }
        return;
      }
      textPreviewVisibleRef.current = true;
    } catch (error) {
      textPreviewVisibleRef.current = false;
      logError("Failed to show text preview:", error);
    }
  }, [textPreviewEnabled, isTextLikeContent, previewPosition, resolveTextPreviewContent, sharpCorners]);

  const handleTextMouseEnter = useCallback(() => {
    if (!textPreviewEnabled || !isTextLikeContent || batchMode) return;
    textPreviewHoveringRef.current = true;
    textPreviewReqIdRef.current += 1;
    const reqId = textPreviewReqIdRef.current;
    const lease = acquireTextPreviewLease();
    textPreviewLeaseRef.current = lease;
    clearTextPreviewTimer();
    textPreviewTimerRef.current = setTimeout(() => {
      void showTextPreview(reqId, lease);
    }, hoverPreviewDelay);
  }, [textPreviewEnabled, isTextLikeContent, batchMode, clearTextPreviewTimer, showTextPreview, hoverPreviewDelay]);

  const handleTextMouseLeave = useCallback(() => {
    hideTextPreview();
  }, [hideTextPreview]);

  const handleTextWheel = useCallback((e: React.WheelEvent<HTMLDivElement>) => {
    if (!e.ctrlKey || !textPreviewVisibleRef.current) return;
    e.preventDefault();
    e.stopPropagation();
    textScrollPendingDeltaRef.current += e.deltaY;

    if (textScrollEmitRafRef.current === null) {
      textScrollEmitRafRef.current = requestAnimationFrame(() => {
        textScrollEmitRafRef.current = null;
        const deltaY = textScrollPendingDeltaRef.current;
        textScrollPendingDeltaRef.current = 0;
        if (deltaY === 0 || !textPreviewVisibleRef.current) return;
        emitTo("text-preview", "text-preview-scroll", { deltaY }).catch((error) => {
          textPreviewVisibleRef.current = false;
          logError("Failed to emit text preview scroll:", error);
        });
      });
    }
  }, []);

  useEffect(() => {
    if (!textPreviewEnabled || !isTextLikeContent) {
      hideTextPreview();
    }
  }, [textPreviewEnabled, isTextLikeContent, hideTextPreview]);

  useEffect(() => {
    if (isDragging) {
      hideTextPreview();
    }
  }, [isDragging, hideTextPreview]);

  useEffect(() => {
    return () => {
      hideTextPreview();
    };
  }, [hideTextPreview]);

  // 主窗口隐藏时取消文本预览
  useEffect(() => {
    ensureWindowHiddenListener();
    textPreviewCleanupCallbacks.add(hideTextPreview);
    return () => { textPreviewCleanupCallbacks.delete(hideTextPreview); };
  }, [hideTextPreview]);

  return {
    textPreviewAnchorRef,
    handleTextMouseEnter,
    handleTextMouseLeave,
    handleTextWheel,
    hideTextPreview,
  };
}
