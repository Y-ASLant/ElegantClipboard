// 视频悬浮预览 hook（复用 image-preview 窗口）

import { useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useShallow } from "zustand/react/shallow";
import {
  getPreviewBounds,
  acquireImagePreviewLease,
  revokeImagePreviewLease,
  isImagePreviewLeaseCurrent,
  isImagePreviewWanted,
} from "@/hooks/useImagePreview";
import { logError } from "@/lib/logger";
import { useClipboardStore } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";

// ============ 常量 ============

const VIDEO_PREVIEW_W = 480;
const VIDEO_PREVIEW_H = 360;

// ============ 全局 window-hidden 清理注册表 ============

const videoPreviewCleanupCallbacks = new Set<() => void>();
let _videoWindowHiddenListenerInit = false;

function ensureVideoWindowHiddenListener() {
  if (_videoWindowHiddenListenerInit) return;
  _videoWindowHiddenListenerInit = true;
  listen("window-hidden", () => {
    videoPreviewCleanupCallbacks.forEach((cb) => cb());
  });
}

// ============ Hook ============

export function useVideoPreview(videoPath?: string) {
  const {
    videoPreviewEnabled, videoPreviewDuration,
    previewPosition, hoverPreviewDelay,
  } = useUISettings(useShallow((s) => ({
    videoPreviewEnabled: s.videoPreviewEnabled,
    videoPreviewDuration: s.videoPreviewDuration,
    previewPosition: s.previewPosition,
    hoverPreviewDelay: s.hoverPreviewDelay,
  })));

  const batchMode = useClipboardStore((s) => s.batchMode);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const hoveringRef = useRef(false);
  const reqIdRef = useRef(0);
  const leaseRef = useRef<number | null>(null);

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const hidePreview = useCallback(() => {
    hoveringRef.current = false;
    reqIdRef.current += 1;
    const closingLease = leaseRef.current;
    if (closingLease !== null) {
      revokeImagePreviewLease(closingLease);
      leaseRef.current = null;
    }
    clearTimer();
    if (closingLease !== null) {
      invoke("hide_image_preview", { token: closingLease }).catch((e) =>
        logError("Failed to hide video preview:", e),
      );
    }
  }, [clearTimer]);

  // 主窗口隐藏时取消预览
  useEffect(() => {
    ensureVideoWindowHiddenListener();
    videoPreviewCleanupCallbacks.add(hidePreview);
    return () => { videoPreviewCleanupCallbacks.delete(hidePreview); };
  }, [hidePreview]);

  const showPreview = useCallback(async (reqId: number, lease: number, path: string) => {
    if (!containerRef.current) return;
    if (!hoveringRef.current || reqId !== reqIdRef.current || !isImagePreviewLeaseCurrent(lease)) return;

    const bounds = await getPreviewBounds(previewPosition, containerRef.current);
    if (!hoveringRef.current || reqId !== reqIdRef.current || !isImagePreviewLeaseCurrent(lease)) return;

    const maxCssW = bounds.maxW / bounds.scale;
    const maxCssH = bounds.maxH / bounds.scale;

    // 视频预览尺寸，适配可用空间
    let cssW = VIDEO_PREVIEW_W;
    let cssH = VIDEO_PREVIEW_H;
    if (cssW > maxCssW || cssH > maxCssH) {
      const ratio = Math.min(maxCssW / cssW, maxCssH / cssH);
      cssW *= ratio;
      cssH *= ratio;
    }
    cssW = Math.max(200, cssW);
    cssH = Math.max(150, cssH);

    const winW = Math.max(1, Math.round(cssW * bounds.scale));
    const winH = Math.max(1, Math.round(cssH * bounds.scale));
    const winX = bounds.side === "left" ? bounds.anchorX - winW : bounds.anchorX;
    const centeredY = Math.round(bounds.cardCenterY - winH / 2);
    const winY = Math.max(bounds.monY, Math.min(centeredY, bounds.monBottom - winH));
    const offsetY = 0; // 视频从顶部开始
    const align = bounds.side === "left" ? "right" : "left";

    try {
      await invoke("show_video_preview", {
        videoPath: path,
        width: cssW,
        height: cssH,
        offsetY,
        winX,
        winY,
        winWidth: winW,
        winHeight: winH,
        align,
        duration: videoPreviewDuration,
        token: lease,
      });

      if (!hoveringRef.current || reqId !== reqIdRef.current || !isImagePreviewLeaseCurrent(lease)) {
        if (!isImagePreviewWanted()) {
          invoke("hide_image_preview", { token: lease }).catch((e) =>
            logError("Failed to hide stale video preview:", e),
          );
        }
      }
    } catch (e) {
      logError("Failed to show video preview:", e);
    }
  }, [previewPosition, videoPreviewDuration]);

  const handleMouseEnter = useCallback(() => {
    if (!videoPath || !videoPreviewEnabled || batchMode) return;
    hoveringRef.current = true;
    reqIdRef.current += 1;
    const reqId = reqIdRef.current;
    const lease = acquireImagePreviewLease();
    leaseRef.current = lease;
    clearTimer();
    timerRef.current = setTimeout(() => {
      void showPreview(reqId, lease, videoPath);
    }, hoverPreviewDelay);
  }, [videoPath, videoPreviewEnabled, batchMode, clearTimer, showPreview, hoverPreviewDelay]);

  useEffect(() => {
    if (!videoPreviewEnabled || batchMode) {
      hidePreview();
    }
  }, [videoPreviewEnabled, batchMode, hidePreview]);

  useEffect(() => {
    return () => {
      hidePreview();
    };
  }, [hidePreview]);

  return {
    containerRef,
    handleMouseEnter,
    hidePreview,
  };
}
