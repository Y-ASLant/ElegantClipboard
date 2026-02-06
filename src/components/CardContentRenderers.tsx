// Content-type-specific renderers for clipboard item cards
// Handles: image preview, file content, and shared footer

import { memo, useCallback, useEffect, useRef, useState } from "react";
import {
  Document16Regular,
  Folder16Regular,
  Warning16Regular,
} from "@fluentui/react-icons";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { getFileNameFromPath, isImageFile } from "@/lib/format";
import { cn } from "@/lib/utils";

// ============ Card Footer ============

interface CardFooterProps {
  metaItems: string[];
  index?: number;
  isDragOverlay?: boolean;
}

export const CardFooter = ({ metaItems, index, isDragOverlay }: CardFooterProps) => (
  <div className="flex items-center justify-between gap-1.5 text-xs text-muted-foreground mt-1.5">
    <div className="flex items-center gap-1.5">
      {metaItems.map((info, i) => (
        <span key={i} className="flex items-center gap-1.5">
          {i > 0 && <span className="text-muted-foreground/50">·</span>}
          {info}
        </span>
      ))}
    </div>
    {index !== undefined && index >= 0 && !isDragOverlay && (
      <span className="min-w-5 h-5 px-1.5 rounded-full bg-primary/10 flex items-center justify-center text-[10px] font-semibold text-primary">
        {index + 1}
      </span>
    )}
  </div>
);

// ============ Shared Image Preview with Hover Enlarge (Native Window) ============

const HOVER_DELAY_MS = 300;
const PREVIEW_GAP = 12;
const MIN_SCALE = 0.3;
const MAX_SCALE = 5.0;
const SCALE_STEP = 0.15;
const BASE_PREVIEW_W = 600;
const BASE_PREVIEW_H = 500;

/** Get available space on each side of the main window */
function getAvailableSpace() {
  const winX = window.screenX ?? window.screenLeft ?? 0;
  const mainW = window.innerWidth || 380;
  const screenW = window.screen.availWidth;

  const leftSpace = winX - PREVIEW_GAP;
  const rightSpace = screenW - (winX + mainW) - PREVIEW_GAP;

  // Prefer left; fall back to right
  if (leftSpace >= 200) {
    return { side: "left" as const, maxW: leftSpace };
  }
  return { side: "right" as const, maxW: rightSpace };
}

/** Calculate the scaled window size, clamped to available space */
function calcPreviewSize(imgW: number, imgH: number, scale: number, maxAvailW: number) {
  let baseW = imgW;
  let baseH = imgH;
  if (baseW > BASE_PREVIEW_W || baseH > BASE_PREVIEW_H) {
    const ratio = Math.min(BASE_PREVIEW_W / baseW, BASE_PREVIEW_H / baseH);
    baseW *= ratio;
    baseH *= ratio;
  }

  let w = baseW * scale;
  let h = baseH * scale;

  const absMaxH = window.screen.availHeight * 0.95;
  if (w > maxAvailW || h > absMaxH) {
    const ratio = Math.min(maxAvailW / w, absMaxH / h);
    w *= ratio;
    h *= ratio;
  }

  return { width: Math.max(200, w), height: Math.max(150, h) };
}

/** Calculate preview x position based on side */
function calcPreviewX(side: "left" | "right", w: number) {
  const winX = window.screenX ?? window.screenLeft ?? 0;
  if (side === "left") {
    return Math.max(0, winX - w - PREVIEW_GAP);
  }
  return winX + (window.innerWidth || 380) + PREVIEW_GAP;
}

/** Clamp x so preview never overlaps main window */
function clampPreviewX(x: number, w: number, side: "left" | "right") {
  if (side === "left") {
    const maxRight = (window.screenX ?? 0) - PREVIEW_GAP;
    return Math.max(0, Math.min(x, maxRight - w));
  }
  const minLeft = (window.screenX ?? 0) + (window.innerWidth || 380) + PREVIEW_GAP;
  return Math.max(minLeft, x);
}

// ---- Preview state stored in a single ref to reduce ref count ----
interface PreviewState {
  visible: boolean;
  scale: number;
  center: { cx: number; cy: number };
  lastSize: { w: number; h: number };
  imgNatural: { w: number; h: number };
  currentPath: string | undefined;
}

const defaultPreviewState = (): PreviewState => ({
  visible: false,
  scale: 1.0,
  center: { cx: 0, cy: 0 },
  lastSize: { w: 0, h: 0 },
  imgNatural: { w: BASE_PREVIEW_W, h: BASE_PREVIEW_H },
  currentPath: undefined,
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
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const ps = useRef<PreviewState>(defaultPreviewState());

  const clearTimer = useCallback(() => {
    if (timerRef.current) { clearTimeout(timerRef.current); timerRef.current = null; }
  }, []);

  const hidePreview = useCallback(() => {
    clearTimer();
    if (ps.current.visible) {
      ps.current.visible = false;
      invoke("hide_image_preview").catch(() => {});
    }
    ps.current.scale = 1.0;
  }, [clearTimer]);

  const updatePreview = useCallback(async (withImage: boolean) => {
    if (!containerRef.current) return;
    const { imgNatural, scale, visible } = ps.current;
    const space = getAvailableSpace();
    const { width, height } = calcPreviewSize(imgNatural.w, imgNatural.h, scale, space.maxW);

    // For resize: skip if size didn't actually change (hit clamp)
    if (visible && Math.abs(width - ps.current.lastSize.w) < 1 && Math.abs(height - ps.current.lastSize.h) < 1) {
      return;
    }

    let x: number, y: number;
    const screenH = window.screen.availHeight;

    if (!visible) {
      // Initial show: position relative to card
      const rect = containerRef.current.getBoundingClientRect();
      const winY = window.screenY ?? window.screenTop ?? 0;
      x = calcPreviewX(space.side, width);
      y = winY + rect.top + rect.height / 2 - height / 2;
      y = Math.max(0, Math.min(y, screenH - height));
      ps.current.center = { cx: x + width / 2, cy: y + height / 2 };
    } else {
      // Resize: expand/shrink around stored center
      x = clampPreviewX(ps.current.center.cx - width / 2, width, space.side);
      y = Math.max(0, Math.min(ps.current.center.cy - height / 2, screenH - height));
    }

    ps.current.lastSize = { w: width, h: height };

    if (!visible) {
      ps.current.visible = true;
      try {
        await invoke("show_image_preview", {
          imagePath: withImage ? ps.current.currentPath : null,
          x, y, width, height,
        });
      } catch {
        ps.current.visible = false;
      }
    } else {
      invoke("show_image_preview", {
        imagePath: null, x, y, width, height,
      }).catch(() => {});
    }
  }, []);

  const handleMouseEnter = useCallback(() => {
    if (!imagePath) return;
    ps.current.currentPath = imagePath;
    clearTimer();
    timerRef.current = setTimeout(() => updatePreview(true), HOVER_DELAY_MS);
  }, [imagePath, clearTimer, updatePreview]);

  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      if (!e.ctrlKey || !ps.current.visible) return;
      e.preventDefault();
      e.stopPropagation();
      const delta = e.deltaY > 0 ? -SCALE_STEP : SCALE_STEP;
      ps.current.scale = Math.max(MIN_SCALE, Math.min(MAX_SCALE, ps.current.scale + delta));
      updatePreview(false);
    },
    [updatePreview],
  );

  const handleImgLoad = useCallback((e: React.SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    if (img.naturalWidth > 0) {
      ps.current.imgNatural = { w: img.naturalWidth, h: img.naturalHeight };
    }
  }, []);

  useEffect(() => {
    return () => {
      clearTimer();
      if (ps.current.visible) invoke("hide_image_preview").catch(() => {});
    };
  }, [clearTimer]);

  return (
    <div
      ref={containerRef}
      className="relative w-full h-20 rounded overflow-hidden bg-muted/30 flex items-center justify-center"
      onMouseEnter={handleMouseEnter}
      onMouseLeave={hidePreview}
      onWheel={handleWheel}
    >
      <img
        src={src}
        alt={alt}
        loading="lazy"
        className="w-full h-full object-contain"
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
  isDragOverlay?: boolean;
}

export const ImageCard = memo(function ImageCard({
  image_path,
  metaItems,
  index,
  isDragOverlay,
}: ImageCardProps) {
  const [error, setError] = useState(false);

  return (
    <div className="flex-1 min-w-0 px-3 py-2.5">
      {error ? (
        <div className="relative w-full h-20 rounded overflow-hidden bg-muted/30 flex items-center justify-center">
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
      <CardFooter metaItems={metaItems} index={index} isDragOverlay={isDragOverlay} />
    </div>
  );
});

// ============ File Image Preview (single image file with fallback) ============

const FileImagePreview = memo(function FileImagePreview({
  filePath,
  metaItems,
  index,
  isDragOverlay,
}: {
  filePath: string;
  metaItems: string[];
  index?: number;
  isDragOverlay?: boolean;
}) {
  const [imgError, setImgError] = useState(false);
  const fileName = getFileNameFromPath(filePath);

  if (imgError) {
    return (
      <div className="flex-1 min-w-0 px-3 py-2.5">
        <div className="flex items-start gap-2.5">
          <div className="flex-shrink-0 w-10 h-10 rounded-lg flex items-center justify-center bg-blue-50 dark:bg-blue-950">
            <Document16Regular className="w-5 h-5 text-blue-500" />
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-sm font-medium truncate text-foreground">{fileName}</p>
            <p className="text-xs truncate mt-0.5 text-muted-foreground">{filePath}</p>
          </div>
        </div>
        <CardFooter metaItems={metaItems} index={index} isDragOverlay={isDragOverlay} />
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
      <CardFooter metaItems={metaItems} index={index} isDragOverlay={isDragOverlay} />
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
  isDragOverlay?: boolean;
}

export const FileContent = memo(function FileContent({
  filePaths,
  filesInvalid,
  preview,
  metaItems,
  index,
  isDragOverlay,
}: FileContentProps) {
  const isMultiple = filePaths.length > 1;
  const isSingleImage =
    !isMultiple && filePaths.length === 1 && !filesInvalid && isImageFile(filePaths[0]);

  if (isSingleImage) {
    return (
      <FileImagePreview
        filePath={filePaths[0]}
        metaItems={metaItems}
        index={index}
        isDragOverlay={isDragOverlay}
      />
    );
  }

  return (
    <div className="flex-1 min-w-0 px-3 py-2.5">
      <div className="flex items-start gap-2.5">
        <div
          className={cn(
            "flex-shrink-0 w-10 h-10 rounded-lg flex items-center justify-center",
            filesInvalid ? "bg-red-50 dark:bg-red-950" : "bg-blue-50 dark:bg-blue-950"
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
              <p className={cn("text-sm font-medium", filesInvalid ? "text-red-500" : "text-foreground")}>
                {filePaths.length} 个文件
                {filesInvalid && <span className="ml-1.5 text-xs font-normal">(已失效)</span>}
              </p>
              <p className={cn("text-xs truncate mt-0.5", filesInvalid ? "text-red-400" : "text-muted-foreground")}>
                {filePaths.map((p) => getFileNameFromPath(p)).slice(0, 3).join(", ")}
                {filePaths.length > 3 && "..."}
              </p>
            </>
          ) : (
            <>
              <p className={cn("text-sm font-medium truncate", filesInvalid ? "text-red-500" : "text-foreground")}>
                {getFileNameFromPath(filePaths[0] || preview || "")}
                {filesInvalid && <span className="ml-1.5 text-xs font-normal">(已失效)</span>}
              </p>
              <p className={cn("text-xs truncate mt-0.5", filesInvalid ? "text-red-400 line-through" : "text-muted-foreground")}>
                {filePaths[0] || preview}
              </p>
            </>
          )}
        </div>
      </div>
      <CardFooter metaItems={metaItems} index={index} isDragOverlay={isDragOverlay} />
    </div>
  );
});
