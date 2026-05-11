// 图片悬浮预览 hook（从 CardContentRenderers.tsx 提取）

import { useCallback, useEffect, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { useShallow } from "zustand/react/shallow";
import { logError } from "@/lib/logger";
import { useClipboardStore } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";

// ============ 常量 ============

const PREVIEW_GAP = 12;
const MIN_SCALE = 0.3;
const MAX_SCALE_BOUNDED = 5.0;
const MAX_SCALE_UNBOUNDED = 5.0;
const BASE_PREVIEW_W = 600;
const BASE_PREVIEW_H = 500;

// ============ 租约管理 ============

let imagePreviewLease = 0;
let imagePreviewWanted = false;

export function acquireImagePreviewLease(): number {
  imagePreviewLease += 1;
  imagePreviewWanted = true;
  return imagePreviewLease;
}

export function revokeImagePreviewLease(lease: number): void {
  if (imagePreviewLease === lease) {
    imagePreviewLease += 1;
    imagePreviewWanted = false;
  }
}

export function isImagePreviewLeaseCurrent(lease: number): boolean {
  return imagePreviewLease === lease;
}

export function isImagePreviewWanted(): boolean {
  return imagePreviewWanted;
}

// ============ 全局 window-hidden 清理注册表 ============

const imagePreviewCleanupCallbacks = new Set<() => void>();
let _imageWindowHiddenListenerInit = false;

function ensureImageWindowHiddenListener() {
  if (_imageWindowHiddenListenerInit) return;
  _imageWindowHiddenListenerInit = true;
  listen("window-hidden", () => {
    imagePreviewCleanupCallbacks.forEach((cb) => cb());
  });
}

// ============ 预览边界计算 ============

/** 预览窗口定位边界（物理像素） */
export interface PreviewBounds {
  /** 可用宽度（物理 px） */
  maxW: number;
  /** 可用高度（物理 px） */
  maxH: number;
  /** 预览锚点 X（物理 px） */
  anchorX: number;
  /** 卡片中心 Y（物理 px） */
  cardCenterY: number;
  /** 显示器顶部 Y（物理 px） */
  monY: number;
  /** 显示器底部 Y（物理 px） */
  monBottom: number;
  scale: number;
  side: "left" | "right";
}

/** 获取主窗口侧边可用空间边界 */
export async function getPreviewBounds(
  position: "auto" | "left" | "right",
  cardElement?: HTMLElement | null,
): Promise<PreviewBounds> {
  // 并行获取物理坐标以减少延迟
  const appWindow = getCurrentWindow();
  const [monitor, outerPos, outerSize] = await Promise.all([
    currentMonitor(),
    appWindow.outerPosition(),
    appWindow.outerSize(),
  ]);
  const monX = monitor?.position.x ?? 0;
  const monY = monitor?.position.y ?? 0;
  const scale = monitor?.scaleFactor ?? 1;
  const physWinX = outerPos.x;
  const physWinY = outerPos.y;
  const physMainW = outerSize.width;
  const physMainH = outerSize.height;

  // 计算任务栏偏移量
  const scr = window.screen as Screen & {
    availTop?: number; availLeft?: number;
    left?: number; top?: number;
  };
  // screen.left/top 为显示器逻辑坐标（Chromium），不可用时回退为 0
  const hasScreenLeft = scr.left != null;
  const hasScreenTop = scr.top != null;
  const workOffsetX = hasScreenLeft && scr.availLeft != null
    ? Math.round((scr.availLeft - scr.left!) * scale)
    : 0;
  const workOffsetY = hasScreenTop && scr.availTop != null
    ? Math.round((scr.availTop - scr.top!) * scale)
    : 0;
  const workX = monX + workOffsetX;
  const workY = monY + workOffsetY;
  const workW = Math.round((scr.availWidth ?? scr.width) * scale);
  const workH = Math.round((scr.availHeight ?? scr.height) * scale);

  const physGap = Math.round(PREVIEW_GAP * scale);
  const physMinW = Math.round(200 * scale);

  // 卡片中心 Y：窗口物理位置 + 视口内偏移
  let cardCenterY = physWinY + Math.round(physMainH / 2);
  if (cardElement) {
    const rect = cardElement.getBoundingClientRect();
    cardCenterY = physWinY + Math.round((rect.top + rect.height / 2) * scale);
  }

  const leftSpace = physWinX - workX - physGap;
  const rightSpace = workX + workW - (physWinX + physMainW) - physGap;

  const useLeft =
    position === "left"
      ? true
      : position === "right"
        ? false
        : leftSpace >= rightSpace && leftSpace >= physMinW;

  if (useLeft) {
    return {
      maxW: Math.max(physMinW, leftSpace),
      maxH: workH,
      anchorX: physWinX - physGap, // 左侧可用空间右边缘
      cardCenterY,
      monY: workY,
      monBottom: workY + workH,
      scale,
      side: "left",
    };
  }
  return {
    maxW: Math.max(physMinW, rightSpace),
    maxH: workH,
    anchorX: physWinX + physMainW + physGap, // 右侧可用空间左边缘
    cardCenterY,
    monY: workY,
    monBottom: workY + workH,
    scale,
    side: "right",
  };
}

// ============ 辅助函数 ============

/** 计算指定缩放比例下的图片 CSS 尺寸 */
function calcImageSize(
  imgW: number,
  imgH: number,
  scale: number,
  maxW?: number,
  maxH?: number,
) {
  // 按 scale=1 适配基准尺寸
  let baseW = imgW;
  let baseH = imgH;
  if (baseW > BASE_PREVIEW_W || baseH > BASE_PREVIEW_H) {
    const ratio = Math.min(BASE_PREVIEW_W / baseW, BASE_PREVIEW_H / baseH);
    baseW *= ratio;
    baseH *= ratio;
  }
  let w = baseW * scale;
  let h = baseH * scale;
  // 限制在可用空间内（有界模式）
  if (maxW != null && maxH != null && (w > maxW || h > maxH)) {
    const ratio = Math.min(maxW / w, maxH / h);
    w *= ratio;
    h *= ratio;
  }
  return { width: Math.max(100, w), height: Math.max(80, h) };
}

interface PreviewState {
  visible: boolean;
  scale: number;
  imgNatural: { w: number; h: number };
  currentPath: string | undefined;
  /** 缓存的边界，供缩放同步处理 */
  bounds: PreviewBounds | null;
  /** 当前预览窗口 CSS 尺寸 */
  windowCss: { w: number; h: number } | null;
}

const defaultPreviewState = (): PreviewState => ({
  visible: false,
  scale: 1.0,
  imgNatural: { w: BASE_PREVIEW_W, h: BASE_PREVIEW_H },
  currentPath: undefined,
  bounds: null,
  windowCss: null,
});

// ============ Hook ============

export function useImagePreview(imagePath?: string) {
  const {
    imagePreviewEnabled, previewUnboundedMode, previewZoomStep,
    previewPosition, imageAutoHeight, cardMaxLines, imageMaxHeight,
    hoverPreviewDelay,
  } = useUISettings(useShallow((s) => ({
    imagePreviewEnabled: s.imagePreviewEnabled,
    previewUnboundedMode: s.previewUnboundedMode,
    previewZoomStep: s.previewZoomStep,
    previewPosition: s.previewPosition,
    imageAutoHeight: s.imageAutoHeight,
    cardMaxLines: s.cardMaxLines,
    imageMaxHeight: s.imageMaxHeight,
    hoverPreviewDelay: s.hoverPreviewDelay,
  })));
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const previewHoveringRef = useRef(false);
  const previewReqIdRef = useRef(0);
  const previewLeaseRef = useRef<number | null>(null);
  const zoomEmitRafRef = useRef<number | null>(null);
  const pendingZoomPayloadRef = useRef<{
    width: number;
    height: number;
    offsetY: number;
    percent: number;
    active: boolean;
    align: "left" | "right";
  } | null>(null);
  const ps = useRef<PreviewState>(defaultPreviewState());

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const hidePreview = useCallback(() => {
    previewHoveringRef.current = false;
    previewReqIdRef.current += 1;
    const closingLease = previewLeaseRef.current;
    if (closingLease !== null) {
      revokeImagePreviewLease(closingLease);
      previewLeaseRef.current = null;
    }
    clearTimer();
    if (zoomEmitRafRef.current !== null) {
      cancelAnimationFrame(zoomEmitRafRef.current);
      zoomEmitRafRef.current = null;
    }
    pendingZoomPayloadRef.current = null;
    ps.current.currentPath = undefined;
    if (closingLease !== null) {
      ps.current.visible = false;
      invoke("hide_image_preview", { token: closingLease }).catch((e) =>
        logError("Failed to hide preview:", e),
      );
    } else if (ps.current.visible) {
      ps.current.visible = false;
      invoke("hide_image_preview").catch((e) =>
        logError("Failed to hide preview:", e),
      );
    }
    ps.current.scale = 1.0;
    ps.current.bounds = null;
    ps.current.windowCss = null;
  }, [clearTimer]);

  // 主窗口隐藏时取消预览
  useEffect(() => {
    ensureImageWindowHiddenListener();
    imagePreviewCleanupCallbacks.add(hidePreview);
    return () => { imagePreviewCleanupCallbacks.delete(hidePreview); };
  }, [hidePreview]);

  // 显示预览
  const showPreview = useCallback(async (reqId: number, lease: number, path: string) => {
    if (!containerRef.current) return;
    if (!previewHoveringRef.current || reqId !== previewReqIdRef.current || !isImagePreviewLeaseCurrent(lease)) return;
    const bounds = await getPreviewBounds(previewPosition, containerRef.current);
    if (!previewHoveringRef.current || reqId !== previewReqIdRef.current || !isImagePreviewLeaseCurrent(lease)) return;
    const { imgNatural } = ps.current;
    const boundedMaxCssW = bounds.maxW / bounds.scale;
    const boundedMaxCssH = bounds.maxH / bounds.scale;
    const { width, height } = previewUnboundedMode
      ? calcImageSize(imgNatural.w, imgNatural.h, 1.0)
      : calcImageSize(imgNatural.w, imgNatural.h, 1.0, boundedMaxCssW, boundedMaxCssH);

    const maxUnbounded = calcImageSize(imgNatural.w, imgNatural.h, MAX_SCALE_UNBOUNDED);
    const windowCssW = previewUnboundedMode ? maxUnbounded.width : boundedMaxCssW;
    const windowCssH = previewUnboundedMode ? maxUnbounded.height : boundedMaxCssH;
    const winW = Math.max(1, Math.round(windowCssW * bounds.scale));
    const winH = Math.max(1, Math.round(windowCssH * bounds.scale));
    const winX = bounds.side === "left" ? bounds.anchorX - winW : bounds.anchorX;
    const winY = previewUnboundedMode
      ? Math.round(bounds.cardCenterY - winH / 2)
      : bounds.monY;

    // 图片在预览窗口内的垂直偏移
    const cardOffsetInWindow = (bounds.cardCenterY - bounds.monY) / bounds.scale;
    const offsetY = previewUnboundedMode
      ? Math.max(0, (windowCssH - height) / 2)
      : Math.max(0, Math.min(cardOffsetInWindow - height / 2, windowCssH - height));

    ps.current.visible = true;
    ps.current.scale = 1.0;
    ps.current.bounds = bounds;
    ps.current.windowCss = { w: windowCssW, h: windowCssH };
    const align = bounds.side === "left" ? "right" : "left";
    try {
      await invoke("show_image_preview", {
        imagePath: path,
        imgWidth: width,
        imgHeight: height,
        offsetY,
        winX,
        winY,
        winWidth: winW,
        winHeight: winH,
        align,
        token: lease,
      });
      if (!previewHoveringRef.current || reqId !== previewReqIdRef.current || !isImagePreviewLeaseCurrent(lease)) {
        ps.current.visible = false;
        ps.current.bounds = null;
        ps.current.windowCss = null;
        if (!isImagePreviewWanted()) {
          invoke("hide_image_preview", { token: lease }).catch((e) =>
            logError("Failed to hide preview:", e),
          );
        }
        return;
      }
      ps.current.visible = true;
    } catch {
      ps.current.visible = false;
      ps.current.bounds = null;
      ps.current.windowCss = null;
    }
  }, [previewPosition, previewUnboundedMode]);

  const batchMode = useClipboardStore((s) => s.batchMode);

  const handleMouseEnter = useCallback(() => {
    if (!imagePath || !imagePreviewEnabled || batchMode) return;
    previewHoveringRef.current = true;
    previewReqIdRef.current += 1;
    const reqId = previewReqIdRef.current;
    const lease = acquireImagePreviewLease();
    previewLeaseRef.current = lease;
    ps.current.currentPath = imagePath;
    clearTimer();
    timerRef.current = setTimeout(() => {
      void showPreview(reqId, lease, imagePath);
    }, hoverPreviewDelay);
  }, [imagePath, imagePreviewEnabled, batchMode, clearTimer, showPreview, hoverPreviewDelay]);

  useEffect(() => {
    if (!imagePreviewEnabled || batchMode) {
      hidePreview();
    }
  }, [imagePreviewEnabled, batchMode, hidePreview]);

  // Ctrl+滚轮缩放
  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      if (!e.ctrlKey || !ps.current.visible || !ps.current.bounds) return;
      e.preventDefault();
      e.stopPropagation();

      const bounds = ps.current.bounds;
      const windowCss = ps.current.windowCss;
      if (!windowCss) return;
      const maxCssW = bounds.maxW / bounds.scale;
      const maxCssH = bounds.maxH / bounds.scale;
      const step = previewZoomStep / 100;
      const delta = e.deltaY > 0 ? -step : step;

      // 计算 scale=1 时的基准尺寸
      const { imgNatural } = ps.current;
      let baseW = imgNatural.w;
      let baseH = imgNatural.h;
      if (baseW > BASE_PREVIEW_W || baseH > BASE_PREVIEW_H) {
        const r = Math.min(BASE_PREVIEW_W / baseW, BASE_PREVIEW_H / baseH);
        baseW *= r;
        baseH *= r;
      }
      const maxEffective = previewUnboundedMode
        ? MAX_SCALE_UNBOUNDED
        : Math.min(maxCssW / baseW, maxCssH / baseH, MAX_SCALE_BOUNDED);

      ps.current.scale = Math.max(
        MIN_SCALE,
        Math.min(maxEffective, ps.current.scale + delta),
      );

      const { width, height } = previewUnboundedMode
        ? calcImageSize(imgNatural.w, imgNatural.h, ps.current.scale)
        : calcImageSize(imgNatural.w, imgNatural.h, ps.current.scale, maxCssW, maxCssH);

      const zoomAlign = bounds.side === "left" ? "right" : "left";
      let offsetY = 0;
      if (previewUnboundedMode) {
        offsetY = Math.max(0, (windowCss.h - height) / 2);
      } else {
        const windowCssH = bounds.maxH / bounds.scale;
        const cardOffsetInWindow = (bounds.cardCenterY - bounds.monY) / bounds.scale;
        offsetY = Math.max(0, Math.min(
          cardOffsetInWindow - height / 2,
          windowCssH - height,
        ));
      }

      const percent = Math.round(ps.current.scale * 100);
      pendingZoomPayloadRef.current = {
        width,
        height,
        offsetY,
        percent,
        active: true,
        align: zoomAlign,
      };

      if (zoomEmitRafRef.current === null) {
        zoomEmitRafRef.current = requestAnimationFrame(() => {
          zoomEmitRafRef.current = null;
          const payload = pendingZoomPayloadRef.current;
          if (!payload) return;
          pendingZoomPayloadRef.current = null;
          emitTo("image-preview", "image-preview-zoom", payload).catch((err) =>
            logError("Failed to emit zoom:", err),
          );
        });
      }
    },
    [previewZoomStep, previewUnboundedMode],
  );

  const handleImgLoad = useCallback(
    (e: React.SyntheticEvent<HTMLImageElement>) => {
      const img = e.currentTarget;
      if (img.naturalWidth > 0) {
        ps.current.imgNatural = { w: img.naturalWidth, h: img.naturalHeight };
      }
    },
    [],
  );

  useEffect(() => {
    return () => {
      hidePreview();
    };
  }, [hidePreview]);

  // 非自适应模式时按 cardMaxLines 计算高度
  const containerStyle = useMemo(() => {
    if (imageAutoHeight) {
      return { maxHeight: `${imageMaxHeight}px` };
    }
    return { maxHeight: `${cardMaxLines * 1.5}rem` };
  }, [imageAutoHeight, cardMaxLines, imageMaxHeight]);

  const imgClass = useMemo(() => {
    return imageAutoHeight
      ? "max-w-full h-auto object-contain"
      : "w-full h-full object-contain";
  }, [imageAutoHeight]);

  const imgStyle = useMemo(() => {
    return imageAutoHeight ? { maxHeight: `${imageMaxHeight}px` } : {};
  }, [imageAutoHeight, imageMaxHeight]);

  return {
    containerRef,
    handleMouseEnter,
    hidePreview,
    handleWheel,
    handleImgLoad,
    containerStyle,
    imgClass,
    imgStyle,
  };
}
