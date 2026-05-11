// 剪贴板卡片内容渲染器：图片预览、文件内容、卡片底栏

import { memo, useEffect, useState, useMemo } from "react";
import {
  Document16Regular,
  Warning16Regular,
} from "@fluentui/react-icons";
import { convertFileSrc } from "@tauri-apps/api/core";
import { FileIconLayout } from "@/components/FileIconLayout";
import { HighlightText } from "@/components/HighlightText";
import { useImagePreview } from "@/hooks/useImagePreview";
import { getFileNameFromPath, isImageFile } from "@/lib/format";
import { cn } from "@/lib/utils";
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

export const CardFooter = memo(function CardFooter({
  metaItems,
  index,
  showBadge = true,
  isDragOverlay,
  sourceAppName,
  sourceAppIcon,
}: CardFooterProps) {
  const iconSrc = useMemo(
    () => (sourceAppIcon ? convertFileSrc(sourceAppIcon) : undefined),
    [sourceAppIcon],
  );

  return (
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
        {iconSrc && (
          <img
            src={iconSrc}
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
  );
});

// ============ 图片悬浮预览（逻辑已提取到 useImagePreview hook） ============

export { getPreviewBounds } from "@/hooks/useImagePreview";

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
    containerRef,
    handleMouseEnter,
    hidePreview,
    handleWheel,
    handleImgLoad,
    containerStyle,
    imgClass,
    imgStyle,
  } = useImagePreview(imagePath);

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

  const imgSrc = useMemo(() => convertFileSrc(image_path), [image_path]);

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
          src={imgSrc}
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
  const fileName = useMemo(() => getFileNameFromPath(filePath), [filePath]);

  // 虚拟列表复用组件时，filePath 变化需重置错误状态
  useEffect(() => setImgError(false), [filePath]);

  const imgSrc = useMemo(() => convertFileSrc(filePath), [filePath]);

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
        src={imgSrc}
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
    <FileIconLayout
      filePaths={filePaths}
      filesInvalid={filesInvalid}
      preview={preview}
      metaItems={metaItems}
      index={index}
      showBadge={showBadge}
      isDragOverlay={isDragOverlay}
      sourceAppName={sourceAppName}
      sourceAppIcon={sourceAppIcon}
      colorScheme="blue"
      singleIcon={Document16Regular}
      multiLabel="个文件"
    />
  );
});

// ============ 视频内容（已提取到 VideoCard.tsx） ============
export { VideoContent } from "@/components/VideoCard";
