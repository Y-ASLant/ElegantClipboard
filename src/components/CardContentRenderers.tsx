// Content-type-specific renderers for clipboard item cards
// Handles: image preview, file content, and shared footer

import { memo, useCallback, useEffect, useRef, useState, useMemo } from "react";
import {
  Document16Regular,
  Folder16Regular,
  Warning16Regular,
} from "@fluentui/react-icons";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { HighlightText } from "@/components/HighlightText";
import { getFileNameFromPath, isImageFile } from "@/lib/format";
import { logError } from "@/lib/logger";
import { cn } from "@/lib/utils";
import { useUISettings } from "@/stores/ui-settings";

// ============ Card Footer ============

interface CardFooterProps {
  metaItems: string[];
  index?: number;
  showBadge?: boolean;
  isDragOverlay?: boolean;
  sourceAppName?: string | null;
  sourceAppIcon?: string | null;
}

export const CardFooter = ({
  metaItems,
  index,
  showBadge = true,
  isDragOverlay,
  sourceAppName,
  sourceAppIcon,
}: CardFooterProps) => (
  <div className="flex items-center justify-between gap-1.5 text-xs text-muted-foreground mt-1.5">
    <div className="flex items-center gap-1.5 min-w-0">
      {metaItems.map((info, i) => (
        <span key={i} className="flex items-center gap-1.5">
          {i > 0 && <span className="text-muted-foreground/50">·</span>}
          {info}
        </span>
      ))}
    </div>
    <div className="flex items-center gap-1.5 flex-shrink-0">
      {sourceAppIcon && (
        <img
          src={convertFileSrc(sourceAppIcon)}
          alt=""
          className="w-3.5 h-3.5 flex-shrink-0"
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
);

// ============ Shared Image Preview with Hover Enlarge (Native Window) ============

const PREVIEW_GAP = 12;
const MIN_SCALE = 0.3;
const MAX_SCALE_BOUNDED = 5.0;
const MAX_SCALE_UNBOUNDED = 5.0;
const BASE_PREVIEW_W = 600;
const BASE_PREVIEW_H = 500;

/** Positioning bounds for the preview window (physical pixels) */
interface PreviewBounds {
  /** Available width (physical px) */
  maxW: number;
  /** Available height (physical px) */
  maxH: number;
  /** Left edge X for right-side preview, or right edge X for left-side preview (physical px) */
  anchorX: number;
  /** Card center Y on screen (physical px) */
  cardCenterY: number;
  /** Monitor top Y (physical px) */
  monY: number;
  /** Monitor bottom Y (physical px) */
  monBottom: number;
  scale: number;
  side: "left" | "right";
}

/** Get the available space bounds beside the main window for positioning the preview */
export async function getPreviewBounds(
  position: "auto" | "left" | "right",
  cardElement?: HTMLElement | null,
): Promise<PreviewBounds> {
  // Fetch physical coordinates in parallel to reduce latency
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

  // Work area: compute taskbar offset within the monitor from screen.availLeft/Top vs screen.left/top
  const scr = window.screen as Screen & {
    availTop?: number; availLeft?: number;
    left?: number; top?: number;
  };
  // screen.left/top = monitor logical position (Chromium-specific)
  // If unavailable, fall back to 0 offset (assume no taskbar inset) rather than
  // using availLeft/availTop which are global coords and would produce a wrong delta.
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

  // Card center Y: use physical window position + viewport-relative card offset
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
      anchorX: physWinX - physGap, // right edge of available left space
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
    anchorX: physWinX + physMainW + physGap, // left edge of available right space
    cardCenterY,
    monY: workY,
    monBottom: workY + workH,
    scale,
    side: "right",
  };
}

/** Calculate image CSS size at a given scale; optional max bounds for bounded mode. */
function calcImageSize(
  imgW: number,
  imgH: number,
  scale: number,
  maxW?: number,
  maxH?: number,
) {
  // Fit to base bounds at scale=1
  let baseW = imgW;
  let baseH = imgH;
  if (baseW > BASE_PREVIEW_W || baseH > BASE_PREVIEW_H) {
    const ratio = Math.min(BASE_PREVIEW_W / baseW, BASE_PREVIEW_H / baseH);
    baseW *= ratio;
    baseH *= ratio;
  }
  let w = baseW * scale;
  let h = baseH * scale;
  // Clamp to available space in bounded mode.
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
  /** Cached bounds from showPreview so zoom handler stays synchronous */
  bounds: PreviewBounds | null;
  /** Current preview window size in CSS px */
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

const ImagePreview = memo(function ImagePreview({
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
  const imagePreviewEnabled = useUISettings((s) => s.imagePreviewEnabled);
  const previewUnboundedMode = useUISettings((s) => s.previewUnboundedMode);
  const previewZoomStep = useUISettings((s) => s.previewZoomStep);
  const previewPosition = useUISettings((s) => s.previewPosition);
  const imageAutoHeight = useUISettings((s) => s.imageAutoHeight);
  const cardMaxLines = useUISettings((s) => s.cardMaxLines);
  const imageMaxHeight = useUISettings((s) => s.imageMaxHeight);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
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
    clearTimer();
    if (zoomEmitRafRef.current !== null) {
      cancelAnimationFrame(zoomEmitRafRef.current);
      zoomEmitRafRef.current = null;
    }
    pendingZoomPayloadRef.current = null;
    if (ps.current.visible) {
      ps.current.visible = false;
      invoke("hide_image_preview").catch((e) =>
        logError("Failed to hide preview:", e),
      );
    }
    ps.current.scale = 1.0;
    ps.current.windowCss = null;
  }, [clearTimer]);

  // Show preview: bounded mode uses screen work area; unbounded mode uses a fixed large window.
  const showPreview = useCallback(async () => {
    if (!containerRef.current || !ps.current.currentPath) return;
    const bounds = await getPreviewBounds(previewPosition, containerRef.current);
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

    // Image vertical offset inside preview window.
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
        imagePath: ps.current.currentPath,
        imgWidth: width,
        imgHeight: height,
        offsetY,
        winX,
        winY,
        winWidth: winW,
        winHeight: winH,
        align,
      });
    } catch {
      ps.current.visible = false;
    }
  }, [previewPosition, previewUnboundedMode]);

  const hoverPreviewDelay = useUISettings((s) => s.hoverPreviewDelay);

  const handleMouseEnter = useCallback(() => {
    if (!imagePath || !imagePreviewEnabled) return;
    ps.current.currentPath = imagePath;
    clearTimer();
    timerRef.current = setTimeout(showPreview, hoverPreviewDelay);
  }, [imagePath, imagePreviewEnabled, clearTimer, showPreview, hoverPreviewDelay]);

  // Ctrl+Scroll zoom. Coalesce cross-window events to one emit per animation frame.
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

      // Compute base size at scale=1 (same as initial display)
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
        // Keep native window fixed; animate image within it for smooth zoom.
        offsetY = Math.max(0, (windowCss.h - height) / 2);
      } else {
        // Recompute vertical offset for bounded mode.
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
      clearTimer();
      if (zoomEmitRafRef.current !== null) {
        cancelAnimationFrame(zoomEmitRafRef.current);
        zoomEmitRafRef.current = null;
      }
      pendingZoomPayloadRef.current = null;
      if (ps.current.visible)
        invoke("hide_image_preview").catch((e) =>
          logError("Failed to hide preview:", e),
        );
    };
  }, [clearTimer]);

  // Calculate height based on cardMaxLines when imageAutoHeight is false
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
      ? "w-full h-auto object-contain"
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

// ============ Image Card (content_type: "image") ============

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

// ============ File Image Preview (single image file with fallback) ============

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
  const fileName = getFileNameFromPath(filePath);

  // 虚拟列表复用组件时，filePath 变化需重置错误状态
  useEffect(() => setImgError(false), [filePath]);

  if (imgError) {
    return (
      <div className="flex-1 min-w-0 px-3 py-2.5">
        <div className="flex items-start gap-2.5">
          <div className="flex-shrink-0 w-10 h-10 rounded-lg flex items-center justify-center bg-blue-50 dark:bg-blue-950">
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
          <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/50 to-transparent px-2 py-1">
            <p className="text-[11px] text-white truncate">{fileName}</p>
          </div>
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

// ============ File Content (content_type: "files") ============

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
            "flex-shrink-0 w-10 h-10 rounded-lg flex items-center justify-center",
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
                  filesInvalid ? "text-red-500" : "text-foreground",
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
                  filesInvalid ? "text-red-400" : "text-muted-foreground",
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
                  filesInvalid ? "text-red-500" : "text-foreground",
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

