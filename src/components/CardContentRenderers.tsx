// 剪贴板卡片内容渲染器：图片预览、文件内容、卡片底栏

import { memo, useCallback, useEffect, useRef, useState, useMemo } from "react";
import {
  Document16Regular,
  Folder16Regular,
  Warning16Regular,
  Video16Regular,
  Play16Filled,
} from "@fluentui/react-icons";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { useShallow } from "zustand/react/shallow";
import { HighlightText } from "@/components/HighlightText";
import { getFileNameFromPath, isImageFile } from "@/lib/format";
import { logError } from "@/lib/logger";
import { cn } from "@/lib/utils";
import { useClipboardStore } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";

// ============ 卡片底栏 ============

interface CardFooterProps {
  metaItems: string[];
  index?: number;
  showBadge?: boolean;
  isDragOverlay?: boolean;
  sourceAppName?: string | null;
  sourceAppIcon?: string | null;
}

export const CardFooter = memo(({
  metaItems,
  index,
  showBadge = true,
  isDragOverlay,
  sourceAppName,
  sourceAppIcon,
}: CardFooterProps) => (
  <div className="flex items-center justify-between gap-1.5 text-xs text-muted-foreground mt-1.5 min-h-5">
    <div className="flex items-center gap-1.5 min-w-0">
      {metaItems.map((info, i) => (
        <span key={i} className="flex items-center gap-1.5">
          {i > 0 && <span className="text-muted-foreground/50">·</span>}
          {info}
        </span>
      ))}
    </div>
    <div className="flex items-center gap-1.5 shrink-0">
      {sourceAppIcon && (
        <img
          src={convertFileSrc(sourceAppIcon)}
          alt=""
          className="w-3.5 h-3.5 shrink-0"
          draggable={false}
        />
      )}
      {sourceAppName && (
        <span className="truncate max-w-[128px]">{sourceAppName}</span>
      )}
      {index !== undefined && index >= 0 && !isDragOverlay && (
        <span
          className={cn(
            "min-w-5 h-5 px-1.5 rounded-full bg-primary/10 flex items-center justify-center text-[10px] font-semibold text-primary transition-opacity duration-150",
            showBadge ? "opacity-100" : "opacity-0",
          )}
        >
          {index + 1}
        </span>
      )}
    </div>
  </div>
));

// ============ 图片悬浮预览（原生窗口） ============

const PREVIEW_GAP = 12;
const MIN_SCALE = 0.3;
const MAX_SCALE_BOUNDED = 5.0;
const MAX_SCALE_UNBOUNDED = 5.0;
const BASE_PREVIEW_W = 600;
const BASE_PREVIEW_H = 500;
let imagePreviewLease = 0;
let imagePreviewWanted = false;

function acquireImagePreviewLease(): number {
  imagePreviewLease += 1;
  imagePreviewWanted = true;
  return imagePreviewLease;
}

function revokeImagePreviewLease(lease: number): void {
  if (imagePreviewLease === lease) {
    imagePreviewLease += 1;
    imagePreviewWanted = false;
  }
}

function isImagePreviewLeaseCurrent(lease: number): boolean {
  return imagePreviewLease === lease;
}

function isImagePreviewWanted(): boolean {
  return imagePreviewWanted;
}

// ============ 全局 window-hidden 清理注册表（图片预览） ============
const imagePreviewCleanupCallbacks = new Set<() => void>();
let _imageWindowHiddenListenerInit = false;

function ensureImageWindowHiddenListener() {
  if (_imageWindowHiddenListenerInit) return;
  _imageWindowHiddenListenerInit = true;
  listen("window-hidden", () => {
    imagePreviewCleanupCallbacks.forEach((cb) => cb());
  });
}

/** 预览窗口定位边界（物理像素） */
interface PreviewBounds {
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

export const ImagePreview = memo(function ImagePreview({
  src,
  alt,
  onError,
  overlay,
  imagePath,
}: {
  src: string;
  alt: string;
  onError: () => void;
  overlay?: React.ReactNode;
  imagePath?: string;
}) {
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

  // 主窗口隐藏时取消预览（通过全局清理注册表，避免每个组件单独订阅 Tauri 事件）
  useEffect(() => {
    ensureImageWindowHiddenListener();
    imagePreviewCleanupCallbacks.add(hidePreview);
    return () => { imagePreviewCleanupCallbacks.delete(hidePreview); };
  }, [hidePreview]);

  // 显示预览：有界模式用屏幕工作区，无界模式用固定大窗口
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

  // Ctrl+滚轮缩放，合并跨窗口事件为每帧一次
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
        // 固定原生窗口，窗口内动画缩放
        offsetY = Math.max(0, (windowCss.h - height) / 2);
      } else {
        // 重算有界模式垂直偏移
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
      // 自适应模式：使用用户设置的最大高度
      return { maxHeight: `${imageMaxHeight}px` };
    }
    // 固定模式：跟随 cardMaxLines
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

  return (
    <div
      ref={containerRef}
      className="relative w-full rounded-sm overflow-hidden bg-muted/30 flex items-center justify-center"
      style={containerStyle}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={hidePreview}
      onWheel={handleWheel}
    >
      <img
        src={src}
        alt={alt}
        loading="lazy"
        className={imgClass}
        style={imgStyle}
        onError={onError}
        onLoad={handleImgLoad}
      />
      {overlay}
    </div>
  );
});

// ============ 图片卡片 ============

interface ImageCardProps {
  image_path: string;
  metaItems: string[];
  index?: number;
  showBadge?: boolean;
  isDragOverlay?: boolean;
  sourceAppName?: string | null;
  sourceAppIcon?: string | null;
}

export const ImageCard = memo(function ImageCard({
  image_path,
  metaItems,
  index,
  showBadge,
  isDragOverlay,
  sourceAppName,
  sourceAppIcon,
}: ImageCardProps) {
  const [error, setError] = useState(false);

  // 虚拟列表复用组件时，image_path 变化需重置错误状态
  useEffect(() => setError(false), [image_path]);

  return (
    <div className="flex-1 min-w-0 px-3 py-2.5">
      {error ? (
        <div className="relative w-full h-32 rounded-sm overflow-hidden bg-muted/30 flex items-center justify-center">
          <div className="text-center">
            <Warning16Regular className="w-6 h-6 text-muted-foreground/40 mx-auto mb-1" />
            <p className="text-xs text-muted-foreground/60">图片加载失败</p>
          </div>
        </div>
      ) : (
        <ImagePreview
          src={convertFileSrc(image_path)}
          alt="Preview"
          onError={() => setError(true)}
          imagePath={image_path}
        />
      )}
      <CardFooter
        metaItems={metaItems}
        index={index}
        showBadge={showBadge}
        isDragOverlay={isDragOverlay}
        sourceAppName={sourceAppName}
        sourceAppIcon={sourceAppIcon}
      />
    </div>
  );
});

// ============ 文件图片预览（单图片文件，失败回退） ============

const FileImagePreview = memo(function FileImagePreview({
  filePath,
  metaItems,
  index,
  showBadge,
  isDragOverlay,
  sourceAppName,
  sourceAppIcon,
}: {
  filePath: string;
  metaItems: string[];
  index?: number;
  showBadge?: boolean;
  isDragOverlay?: boolean;
  sourceAppName?: string | null;
  sourceAppIcon?: string | null;
}) {
  const [imgError, setImgError] = useState(false);
  const showImageFileName = useUISettings((s) => s.showImageFileName);
  const fileName = getFileNameFromPath(filePath);

  // 虚拟列表复用组件时，filePath 变化需重置错误状态
  useEffect(() => setImgError(false), [filePath]);

  if (imgError) {
    return (
      <div className="flex-1 min-w-0 px-3 py-2.5">
        <div className="flex items-start gap-2.5">
          <div className="shrink-0 w-10 h-10 rounded-lg flex items-center justify-center bg-blue-50 dark:bg-blue-950">
            <Document16Regular className="w-5 h-5 text-blue-500" />
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-sm font-medium truncate text-foreground">
              <HighlightText text={fileName} />
            </p>
            <p className="text-xs truncate mt-0.5 text-muted-foreground">
              <HighlightText text={filePath} />
            </p>
          </div>
        </div>
        <CardFooter
          metaItems={metaItems}
          index={index}
          showBadge={showBadge}
          isDragOverlay={isDragOverlay}
          sourceAppName={sourceAppName}
          sourceAppIcon={sourceAppIcon}
        />
      </div>
    );
  }

  return (
    <div className="flex-1 min-w-0 px-3 py-2.5">
      <ImagePreview
        src={convertFileSrc(filePath)}
        alt={fileName}
        onError={() => setImgError(true)}
        imagePath={filePath}
        overlay={
          showImageFileName ? (
            <div className="absolute bottom-0 left-0 right-0 bg-linear-to-t from-black/50 to-transparent px-2 py-1">
              <p className="text-[11px] text-white truncate">{fileName}</p>
            </div>
          ) : undefined
        }
      />
      <CardFooter
        metaItems={metaItems}
        index={index}
        showBadge={showBadge}
        isDragOverlay={isDragOverlay}
        sourceAppName={sourceAppName}
        sourceAppIcon={sourceAppIcon}
      />
    </div>
  );
});

// ============ 文件内容 ============

interface FileContentProps {
  filePaths: string[];
  filesInvalid: boolean;
  preview: string | null;
  metaItems: string[];
  index?: number;
  showBadge?: boolean;
  isDragOverlay?: boolean;
  sourceAppName?: string | null;
  sourceAppIcon?: string | null;
}

export const FileContent = memo(function FileContent({
  filePaths,
  filesInvalid,
  preview,
  metaItems,
  index,
  showBadge,
  isDragOverlay,
  sourceAppName,
  sourceAppIcon,
}: FileContentProps) {
  const isMultiple = filePaths.length > 1;
  const isSingleImage =
    !isMultiple &&
    filePaths.length === 1 &&
    !filesInvalid &&
    isImageFile(filePaths[0]);

  if (isSingleImage) {
    return (
      <FileImagePreview
        filePath={filePaths[0]}
        metaItems={metaItems}
        index={index}
        showBadge={showBadge}
        isDragOverlay={isDragOverlay}
        sourceAppName={sourceAppName}
        sourceAppIcon={sourceAppIcon}
      />
    );
  }

  return (
    <div className="flex-1 min-w-0 px-3 py-2.5">
      <div className="flex items-start gap-2.5">
        <div
          className={cn(
            "shrink-0 w-10 h-10 rounded-lg flex items-center justify-center",
            filesInvalid
              ? "bg-red-50 dark:bg-red-950"
              : "bg-blue-50 dark:bg-blue-950",
          )}
        >
          {filesInvalid ? (
            <Warning16Regular className="w-5 h-5 text-red-500" />
          ) : isMultiple ? (
            <Folder16Regular className="w-5 h-5 text-blue-500" />
          ) : (
            <Document16Regular className="w-5 h-5 text-blue-500" />
          )}
        </div>
        <div className="flex-1 min-w-0">
          {isMultiple ? (
            <>
              <p
                className={cn(
                  "text-sm font-medium",
                  filesInvalid ? "text-red-500 line-through" : "text-foreground",
                )}
              >
                {filePaths.length} 个文件
                {filesInvalid && (
                  <span className="ml-1.5 text-xs font-normal">(已失效)</span>
                )}
              </p>
              <p
                className={cn(
                  "text-xs truncate mt-0.5",
                  filesInvalid ? "text-red-400 line-through" : "text-muted-foreground",
                )}
              >
                <HighlightText
                  text={
                    filePaths
                      .map((p) => getFileNameFromPath(p))
                      .slice(0, 3)
                      .join(", ") + (filePaths.length > 3 ? "..." : "")
                  }
                />
              </p>
            </>
          ) : (
            <>
              <p
                className={cn(
                  "text-sm font-medium truncate",
                  filesInvalid ? "text-red-500 line-through" : "text-foreground",
                )}
              >
                <HighlightText
                  text={getFileNameFromPath(filePaths[0] || preview || "")}
                />
                {filesInvalid && (
                  <span className="ml-1.5 text-xs font-normal">(已失效)</span>
                )}
              </p>
              <p
                className={cn(
                  "text-xs truncate mt-0.5",
                  filesInvalid
                    ? "text-red-400 line-through"
                    : "text-muted-foreground",
                )}
              >
                <HighlightText text={filePaths[0] || preview || ""} />
              </p>
            </>
          )}
        </div>
      </div>
      <CardFooter
        metaItems={metaItems}
        index={index}
        showBadge={showBadge}
        isDragOverlay={isDragOverlay}
        sourceAppName={sourceAppName}
        sourceAppIcon={sourceAppIcon}
      />
    </div>
  );
});

// ============ 视频内容 ============

interface VideoContentProps {
  filePaths: string[];
  filesInvalid: boolean;
  preview: string | null;
  metaItems: string[];
  index?: number;
  showBadge?: boolean;
  isDragOverlay?: boolean;
  sourceAppName?: string | null;
  sourceAppIcon?: string | null;
}

export const VideoContent = memo(function VideoContent({
  filePaths,
  filesInvalid,
  preview,
  metaItems,
  index,
  showBadge,
  isDragOverlay,
  sourceAppName,
  sourceAppIcon,
}: VideoContentProps) {
  const isMultiple = filePaths.length > 1;
  const firstPath = filePaths[0] || "";
  const fileName = getFileNameFromPath(firstPath || preview || "");
  const [thumbUrl, setThumbUrl] = useState<string | null>(null);
  const [thumbError, setThumbError] = useState(false);

  // 从视频文件生成缩略图
  useEffect(() => {
    if (filesInvalid || !firstPath || isMultiple) {
      setThumbUrl(null);
      setThumbError(false);
      return;
    }

    let cancelled = false;
    const video = document.createElement("video");
    video.preload = "metadata";
    video.muted = true;
    video.playsInline = true;
    video.src = convertFileSrc(firstPath);

    video.addEventListener("loadeddata", () => {
      if (cancelled) return;
      video.currentTime = Math.min(1, video.duration * 0.1);
    });

    video.addEventListener("seeked", () => {
      if (cancelled) return;
      try {
        const canvas = document.createElement("canvas");
        canvas.width = video.videoWidth;
        canvas.height = video.videoHeight;
        const ctx = canvas.getContext("2d");
        if (ctx) {
          ctx.drawImage(video, 0, 0);
          setThumbUrl(canvas.toDataURL("image/jpeg", 0.7));
        } else {
          setThumbError(true);
        }
      } catch {
        setThumbError(true);
      }
    });

    video.addEventListener("error", () => {
      if (!cancelled) setThumbError(true);
    });

    return () => {
      cancelled = true;
      video.src = "";
      video.load();
    };
  }, [firstPath, filesInvalid, isMultiple]);

  // 有缩略图且单个视频文件时显示预览
  if (!isMultiple && !filesInvalid && thumbUrl && !thumbError) {
    return (
      <div className="flex-1 min-w-0 px-3 py-2.5">
        <div className="relative w-full rounded-sm overflow-hidden bg-muted/30">
          <img
            src={thumbUrl}
            alt={fileName}
            className="w-full h-auto max-h-48 object-cover"
          />
          <div className="absolute inset-0 flex items-center justify-center">
            <div className="w-10 h-10 rounded-full bg-black/50 flex items-center justify-center">
              <Play16Filled className="w-5 h-5 text-white ml-0.5" />
            </div>
          </div>
          <div className="absolute bottom-0 left-0 right-0 bg-linear-to-t from-black/50 to-transparent px-2 py-1">
            <p className="text-[11px] text-white truncate">{fileName}</p>
          </div>
        </div>
        <CardFooter
          metaItems={metaItems}
          index={index}
          showBadge={showBadge}
          isDragOverlay={isDragOverlay}
          sourceAppName={sourceAppName}
          sourceAppIcon={sourceAppIcon}
        />
      </div>
    );
  }

  // 无缩略图、多文件、或失效时回退为图标+文件名样式
  return (
    <div className="flex-1 min-w-0 px-3 py-2.5">
      <div className="flex items-start gap-2.5">
        <div
          className={cn(
            "shrink-0 w-10 h-10 rounded-lg flex items-center justify-center",
            filesInvalid
              ? "bg-red-50 dark:bg-red-950"
              : "bg-purple-50 dark:bg-purple-950",
          )}
        >
          {filesInvalid ? (
            <Warning16Regular className="w-5 h-5 text-red-500" />
          ) : isMultiple ? (
            <Folder16Regular className="w-5 h-5 text-purple-500" />
          ) : (
            <Video16Regular className="w-5 h-5 text-purple-500" />
          )}
        </div>
        <div className="flex-1 min-w-0">
          {isMultiple ? (
            <>
              <p
                className={cn(
                  "text-sm font-medium",
                  filesInvalid ? "text-red-500 line-through" : "text-foreground",
                )}
              >
                {filePaths.length} 个视频
                {filesInvalid && (
                  <span className="ml-1.5 text-xs font-normal">(已失效)</span>
                )}
              </p>
              <p
                className={cn(
                  "text-xs truncate mt-0.5",
                  filesInvalid ? "text-red-400 line-through" : "text-muted-foreground",
                )}
              >
                <HighlightText
                  text={
                    filePaths
                      .map((p) => getFileNameFromPath(p))
                      .slice(0, 3)
                      .join(", ") + (filePaths.length > 3 ? "..." : "")
                  }
                />
              </p>
            </>
          ) : (
            <>
              <p
                className={cn(
                  "text-sm font-medium truncate",
                  filesInvalid ? "text-red-500 line-through" : "text-foreground",
                )}
              >
                <HighlightText text={fileName} />
                {filesInvalid && (
                  <span className="ml-1.5 text-xs font-normal">(已失效)</span>
                )}
              </p>
              <p
                className={cn(
                  "text-xs truncate mt-0.5",
                  filesInvalid
                    ? "text-red-400 line-through"
                    : "text-muted-foreground",
                )}
              >
                <HighlightText text={firstPath || preview || ""} />
              </p>
            </>
          )}
        </div>
      </div>
      <CardFooter
        metaItems={metaItems}
        index={index}
        showBadge={showBadge}
        isDragOverlay={isDragOverlay}
        sourceAppName={sourceAppName}
        sourceAppIcon={sourceAppIcon}
      />
    </div>
  );
});
