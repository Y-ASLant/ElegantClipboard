// 视频内容卡片渲染器（从 CardContentRenderers.tsx 提取）

import { memo, useEffect, useState } from "react";
import {
  Video16Regular,
  Play16Filled,
} from "@fluentui/react-icons";
import { convertFileSrc } from "@tauri-apps/api/core";
import { CardFooter } from "@/components/CardContentRenderers";
import { FileIconLayout } from "@/components/FileIconLayout";
import { useVideoPreview } from "@/hooks/useVideoPreview";
import { getFileNameFromPath } from "@/lib/format";

// ============ 视频缩略图 LRU 缓存 ============

const THUMB_CACHE_MAX = 50;
const thumbCache = new Map<string, string>();

function getThumbFromCache(path: string): string | null {
  const url = thumbCache.get(path);
  if (url) {
    thumbCache.delete(path);
    thumbCache.set(path, url);
    return url;
  }
  return null;
}

function setThumbToCache(path: string, url: string): void {
  if (thumbCache.size >= THUMB_CACHE_MAX) {
    const oldest = thumbCache.keys().next().value;
    if (oldest !== undefined) thumbCache.delete(oldest);
  }
  thumbCache.set(path, url);
}

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
  const [thumbUrl, setThumbUrl] = useState<string | null>(() => getThumbFromCache(firstPath));
  const [thumbError, setThumbError] = useState(false);

  // 从视频文件生成缩略图（优先使用缓存）
  useEffect(() => {
    if (filesInvalid || !firstPath || isMultiple) {
      setThumbUrl(null);
      setThumbError(false);
      return;
    }

    const cached = getThumbFromCache(firstPath);
    if (cached) {
      setThumbUrl(cached);
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
          const dataUrl = canvas.toDataURL("image/jpeg", 0.7);
          setThumbToCache(firstPath, dataUrl);
          setThumbUrl(dataUrl);
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

  // 视频悬浮预览 hook（单个有效视频文件时启用）
  const videoPreviewPath = (!isMultiple && !filesInvalid && firstPath) ? firstPath : undefined;
  const {
    containerRef: videoPreviewRef,
    handleMouseEnter: handleVideoMouseEnter,
    hidePreview: hideVideoPreview,
  } = useVideoPreview(videoPreviewPath);

  // 有缩略图且单个视频文件时显示预览
  if (!isMultiple && !filesInvalid && thumbUrl && !thumbError) {
    return (
      <div
        ref={videoPreviewRef}
        className="flex-1 min-w-0 px-3 py-2.5"
        onMouseEnter={handleVideoMouseEnter}
        onMouseLeave={hideVideoPreview}
      >
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
    <div
      ref={videoPreviewRef}
      onMouseEnter={handleVideoMouseEnter}
      onMouseLeave={hideVideoPreview}
    >
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
        colorScheme="purple"
        singleIcon={Video16Regular}
        multiLabel="个视频"
      />
    </div>
  );
});
