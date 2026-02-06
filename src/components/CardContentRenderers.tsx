// Content-type-specific renderers for clipboard item cards
// Handles: image preview, file content, and shared footer

import { memo, useState } from "react";
import {
  Document16Regular,
  Folder16Regular,
  Warning16Regular,
} from "@fluentui/react-icons";
import { convertFileSrc } from "@tauri-apps/api/core";
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

// ============ Shared Image Preview ============

const ImagePreview = memo(function ImagePreview({
  src,
  alt,
  onError,
  overlay,
}: {
  src: string;
  alt: string;
  onError: () => void;
  overlay?: React.ReactNode;
}) {
  return (
    <div className="relative w-full h-20 rounded overflow-hidden bg-muted/30 flex items-center justify-center">
      <img
        src={src}
        alt={alt}
        loading="lazy"
        className="w-full h-full object-contain"
        onError={onError}
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
