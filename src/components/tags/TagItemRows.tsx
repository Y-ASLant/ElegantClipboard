import { useMemo } from "react";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  Dismiss16Regular,
  Document16Regular,
  Edit16Regular,
  Folder16Regular,
  ReOrderDotsVertical16Regular,
  Video16Regular,
  Warning16Regular,
} from "@fluentui/react-icons";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { ImagePreview } from "@/components/CardContentRenderers";
import { HighlightText } from "@/components/HighlightText";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useTextHoverPreview } from "@/hooks/useTextHoverPreview";
import {
  contentTypeConfig,
  formatSize,
  formatTime,
  getFileNameFromPath,
  parseFilePaths,
} from "@/lib/format";
import { cn } from "@/lib/utils";
import type { ClipboardItem } from "@/stores/clipboard";

type TimeFormat = "relative" | "absolute";

interface TagItemRowContentProps {
  item: ClipboardItem;
  timeFormat: TimeFormat;
}

function TagItemRowContent({ item, timeFormat }: TagItemRowContentProps) {
  const typeConfig = contentTypeConfig[item.content_type] ?? contentTypeConfig.text;
  const preview = item.preview ?? item.text_content ?? "";
  const truncated = preview.length > 120 ? `${preview.slice(0, 120)}…` : preview;
  const filePaths = (item.content_type === "files" || item.content_type === "video") ? parseFilePaths(item.file_paths) : [];
  const filesInvalid = (item.content_type === "files" || item.content_type === "video") && item.files_valid === false;
  const isVideo = item.content_type === "video";
  const logicalLabel = isVideo ? contentTypeConfig.video.label : typeConfig.label;
  const { anchorRef: textRef, handleMouseEnter: onTextEnter, handleMouseLeave: onTextLeave, handleWheel: onTextWheel } = useTextHoverPreview(item);

  if (item.content_type === "image" && item.image_path) {
    return (
      <>
        <ImagePreview
          src={convertFileSrc(item.image_path)}
          alt="Preview"
          onError={() => {}}
          imagePath={item.image_path}
        />
        <div className="flex items-center gap-1.5 mt-1.5">
          <span className="inline-flex items-center text-[10px] text-muted-foreground px-1 py-px rounded bg-muted/60">
            {typeConfig.label}
          </span>
          <span className="text-[10px] text-muted-foreground/60">
            {formatTime(item.created_at, timeFormat)}
          </span>
          <span className="ml-auto text-[10px] text-muted-foreground/50">
            {formatSize(item.byte_size)}
          </span>
        </div>
      </>
    );
  }

  if (item.content_type === "files" || item.content_type === "video") {
    return (
      <>
        <div className="flex items-center gap-2">
          <div className={cn(
            "shrink-0 w-8 h-8 rounded-md flex items-center justify-center",
            filesInvalid ? "bg-red-50 dark:bg-red-950" : isVideo ? "bg-purple-50 dark:bg-purple-950" : "bg-blue-50 dark:bg-blue-950",
          )}>
            {filesInvalid ? (
              <Warning16Regular className="w-4 h-4 text-red-500" />
            ) : isVideo ? (
              filePaths.length > 1 ? (
                <Folder16Regular className="w-4 h-4 text-purple-500" />
              ) : (
                <Video16Regular className="w-4 h-4 text-purple-500" />
              )
            ) : filePaths.length > 1 ? (
              <Folder16Regular className="w-4 h-4 text-blue-500" />
            ) : (
              <Document16Regular className="w-4 h-4 text-blue-500" />
            )}
          </div>
          <div className="flex-1 min-w-0">
            <p className={cn(
              "text-xs font-medium truncate",
              filesInvalid ? "text-red-500 line-through" : "text-foreground/90",
            )}>
              {filePaths.length > 1
                ? `${filePaths.length} 个${isVideo ? "视频" : "文件"}`
                : getFileNameFromPath(filePaths[0] || preview)}
              {filesInvalid && <span className="ml-1 text-[10px] font-normal">(已失效)</span>}
            </p>
            {filePaths.length > 1 ? (
              <p className={cn(
                "text-[10px] truncate mt-0.5",
                filesInvalid ? "text-red-400 line-through" : "text-muted-foreground/60",
              )}>
                {filePaths.map(p => getFileNameFromPath(p)).slice(0, 3).join(", ")}{filePaths.length > 3 ? "..." : ""}
              </p>
            ) : (
              <p className={cn(
                "text-[10px] truncate mt-0.5",
                filesInvalid ? "text-red-400 line-through" : "text-muted-foreground/60",
              )}>
                {filePaths[0] || preview || ""}
              </p>
            )}
          </div>
        </div>
        <div className="flex items-center gap-1.5 mt-1">
          <span className="inline-flex items-center text-[10px] text-muted-foreground px-1 py-px rounded bg-muted/60">
            {logicalLabel}
          </span>
          <span className="text-[10px] text-muted-foreground/60">
            {formatTime(item.created_at, timeFormat)}
          </span>
          <span className="ml-auto text-[10px] text-muted-foreground/50">
            {formatSize(item.byte_size)}
          </span>
        </div>
      </>
    );
  }

  return (
    <div
      ref={textRef}
      onMouseEnter={onTextEnter}
      onMouseLeave={onTextLeave}
      onWheel={onTextWheel}
    >
      <p className="text-xs leading-relaxed line-clamp-2 break-all text-foreground/90"><HighlightText text={truncated || "(空)"} /></p>
      <div className="flex items-center gap-1.5 mt-1">
        <span className="inline-flex items-center text-[10px] text-muted-foreground px-1 py-px rounded bg-muted/60">
          {typeConfig.label}
        </span>
        <span className="text-[10px] text-muted-foreground/60">
          {formatTime(item.created_at, timeFormat)}
        </span>
      </div>
    </div>
  );
}

interface TagItemRowProps {
  item: ClipboardItem;
  timeFormat: TimeFormat;
  onPaste: () => void;
  onRemove: () => void;
  isLast: boolean;
}

export function SortableTagItemRow({
  item,
  timeFormat,
  onPaste,
  onRemove,
  isLast,
}: TagItemRowProps) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: item.id });

  const style = useMemo(() => ({
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : undefined,
  }), [transform, transition, isDragging]);

  const isEditable = item.content_type === "text" || item.content_type === "html" || item.content_type === "rtf";

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cn(
        "flex items-start gap-1.5 px-2 py-2 hover:bg-accent/40 cursor-default group/row transition-colors duration-100",
        !isLast && "border-b border-border/50",
      )}
      onClick={onPaste}
    >
      <button
        {...attributes}
        {...listeners}
        className="shrink-0 mt-0.5 w-5 h-5 flex items-center justify-center text-muted-foreground/40 cursor-grab active:cursor-grabbing touch-none"
        onClick={(e) => e.stopPropagation()}
      >
        <ReOrderDotsVertical16Regular className="w-3.5 h-3.5" />
      </button>
      <div className="flex-1 min-w-0">
        <TagItemRowContent item={item} timeFormat={timeFormat} />
      </div>
      <ItemActionButtons itemId={item.id} isEditable={isEditable} onRemove={onRemove} />
    </div>
  );
}

export function TagItemRow({
  item,
  timeFormat,
  onPaste,
  onRemove,
  isLast,
}: TagItemRowProps) {
  const isEditable = item.content_type === "text" || item.content_type === "html" || item.content_type === "rtf";

  return (
    <div
      className={cn(
        "flex items-start gap-2.5 px-3 py-2 hover:bg-accent/40 cursor-default group/row transition-colors duration-100",
        !isLast && "border-b border-border/50",
      )}
      onClick={onPaste}
    >
      <div className="flex-1 min-w-0">
        <TagItemRowContent item={item} timeFormat={timeFormat} />
      </div>
      <ItemActionButtons itemId={item.id} isEditable={isEditable} onRemove={onRemove} />
    </div>
  );
}

function ItemActionButtons({ itemId, isEditable, onRemove }: { itemId: number; isEditable: boolean; onRemove: () => void }) {
  return (
    <div className="shrink-0 flex flex-col gap-0.5 mt-0.5 opacity-0 group-hover/row:opacity-100 transition-opacity duration-150">
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={(e) => { e.stopPropagation(); onRemove(); }}
            className="w-5 h-5 flex items-center justify-center rounded-md text-muted-foreground/50 hover:text-destructive hover:bg-destructive/10 transition-colors"
          >
            <Dismiss16Regular className="w-3 h-3" />
          </button>
        </TooltipTrigger>
        <TooltipContent>移除标签</TooltipContent>
      </Tooltip>
      {isEditable && (
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={(e) => {
                e.stopPropagation();
                invoke("open_text_editor_window", { id: itemId }).catch(() => {});
              }}
              className="w-5 h-5 flex items-center justify-center rounded-md text-muted-foreground/50 hover:text-primary hover:bg-primary/10 transition-colors"
            >
              <Edit16Regular className="w-3 h-3" />
            </button>
          </TooltipTrigger>
          <TooltipContent>编辑</TooltipContent>
        </Tooltip>
      )}
    </div>
  );
}