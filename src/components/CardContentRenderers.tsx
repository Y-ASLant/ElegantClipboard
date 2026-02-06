// Content-type-specific renderers for clipboard item cards
// Handles: image preview, file content, and shared footer

import { memo, useCallback, useEffect, useRef, useState } from "react";
import {
  Document16Regular,
  Folder16Regular,
  Warning16Regular,
} from "@fluentui/react-icons";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getFileNameFromPath, isImageFile } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useUISettings } from "@/stores/ui-settings";

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
const BASE_PREVIEW_W = 600;
const BASE_PREVIEW_H = 500;

/** Get the fixed window rect: fills the available space beside the main window */
function getPreviewWindowRect(position: "auto" | "left" | "right") {
  const winX = window.screenX ?? window.screenLeft ?? 0;
  const mainW = window.innerWidth || 380;
  const screenW = window.screen.availWidth;
  const screenH = window.screen.availHeight;

  const leftSpace = winX - PREVIEW_GAP;
  const rightSpace = screenW - (winX + mainW) - PREVIEW_GAP;

  const useLeft =
    position === "left" ? true :
    position === "right" ? false :
    leftSpace >= rightSpace && leftSpace >= 200;

  if (useLeft) {
    return { x: 0, y: 0, w: Math.max(200, leftSpace), h: screenH };
  }
  return { x: winX + mainW + PREVIEW_GAP, y: 0, w: Math.max(200, rightSpace), h: screenH };
}

/** Calculate image CSS size at a given scale, fitted into available space */
function calcImageSize(imgW: number, imgH: number, scale: number, maxW: number, maxH: number) {
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
  // Clamp to available space
  if (w > maxW || h > maxH) {
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
}

const defaultPreviewState = (): PreviewState => ({
  visible: false,
  scale: 1.0,
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
  const imagePreviewEnabled = useUISettings((s) => s.imagePreviewEnabled);
  const previewZoomStep = useUISettings((s) => s.previewZoomStep);
  const previewPosition = useUISettings((s) => s.previewPosition);
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

  // Show preview: open fixed-size window, send image + initial CSS size
  const showPreview = useCallback(async () => {
    if (!containerRef.current || !ps.current.currentPath) return;
    const rect = getPreviewWindowRect(previewPosition);
    const { imgNatural } = ps.current;
    const { width, height } = calcImageSize(imgNatural.w, imgNatural.h, 1.0, rect.w, rect.h);

    ps.current.visible = true;
    ps.current.scale = 1.0;
    try {
      await invoke("show_image_preview", {
        imagePath: ps.current.currentPath,
        imgWidth: width,
        imgHeight: height,
        winX: rect.x,
        winY: rect.y,
        winWidth: rect.w,
        winHeight: rect.h,
      });
    } catch {
      ps.current.visible = false;
    }
  }, [previewPosition]);

  const handleMouseEnter = useCallback(() => {
    if (!imagePath || !imagePreviewEnabled) return;
    ps.current.currentPath = imagePath;
    clearTimer();
    timerRef.current = setTimeout(showPreview, HOVER_DELAY_MS);
  }, [imagePath, imagePreviewEnabled, clearTimer, showPreview]);

  // Ctrl+Scroll: emit CSS size change (smooth transition in webview, no window resize)
  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      if (!e.ctrlKey || !ps.current.visible) return;
      e.preventDefault();
      e.stopPropagation();

      const step = previewZoomStep / 100;
      const delta = e.deltaY > 0 ? -step : step;
      ps.current.scale = Math.max(MIN_SCALE, Math.min(MAX_SCALE, ps.current.scale + delta));

      const rect = getPreviewWindowRect(previewPosition);
      const { width, height } = calcImageSize(
        ps.current.imgNatural.w, ps.current.imgNatural.h,
        ps.current.scale, rect.w, rect.h,
      );
      const percent = Math.round(ps.current.scale * 100);
      emit("image-preview-zoom", { width, height, percent, active: true }).catch(() => {});
    },
    [previewZoomStep, previewPosition],
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
